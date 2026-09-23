//! Streaming season audit and offline replay. No derived database writes.
use super::{analyze, read, types::*};
use crate::AnyError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    pub snapshot_version: u8,
    pub season: i32,
    pub games: usize,
    pub source_snapshot_at: String,
}

#[derive(Default)]
pub struct AuditOptions {
    pub season: i32,
    pub input: Option<PathBuf>,
    pub snapshot_out: Option<PathBuf>,
    pub games_out: Option<PathBuf>,
    pub events_out: Option<PathBuf>,
    pub profile: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Distribution {
    pub compared_players: usize,
    pub signed_min_seconds: Option<i64>,
    pub signed_max_seconds: Option<i64>,
    pub mean_absolute_seconds: Option<f64>,
    pub median_absolute_seconds: Option<i64>,
    pub p95_absolute_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct GameSummary {
    pub game_id: i64,
    pub game_type: i16,
    pub availability: String,
    pub latest_fetch_status: Option<String>,
    pub source_sha256: String,
    pub shifts: usize,
    pub accepted_shifts: usize,
    pub validation_issues: usize,
    pub official_players: usize,
    pub events: usize,
    pub reconstruction: BTreeMap<ReconstructionStatus, usize>,
    pub situation: BTreeMap<Agreement, usize>,
}

#[derive(Debug, Serialize)]
pub struct SeasonAudit {
    pub method: String,
    pub season: i32,
    pub source_snapshot_at: String,
    pub generated_at: String,
    pub source_sha256: String,
    pub snapshot_games: usize,
    pub eligible_games: usize,
    pub unsupported_games: usize,
    pub availability: BTreeMap<String, usize>,
    pub loaded_games_without_events: usize,
    pub loaded_games_without_official_toi: usize,
    pub shift_rows: usize,
    pub accepted_shift_rows: usize,
    pub rejected_shift_rows: usize,
    pub role_fallback_players: usize,
    pub validation_issues: BTreeMap<IssueCode, usize>,
    pub toi_categories: BTreeMap<ToiCategory, usize>,
    pub toi_by_role: BTreeMap<Role, BTreeMap<ToiCategory, usize>>,
    pub toi_difference_histogram: BTreeMap<String, usize>,
    pub toi_distribution: Distribution,
    pub events: usize,
    pub reconstruction: BTreeMap<ReconstructionStatus, usize>,
    pub situation_agreement: BTreeMap<Agreement, usize>,
    pub reconstruction_by_event_type: BTreeMap<String, BTreeMap<ReconstructionStatus, usize>>,
    pub situation_by_event_type: BTreeMap<String, BTreeMap<Agreement, usize>>,
    pub situation_by_context: BTreeMap<String, BTreeMap<Agreement, usize>>,
    pub mismatch_fields: BTreeMap<String, usize>,
    pub incomplete_reasons: BTreeMap<String, usize>,
    pub decoded_field_disagreements: usize,
    pub resolved_with_matching_counts: usize,
    pub resolved_matching_counts_without_source_warnings: usize,
    pub examples: BTreeMap<String, Vec<EventLineup>>,
    pub games: Vec<GameSummary>,
    pub read_ms: f64,
    pub analysis_ms: f64,
    pub wall_ms: f64,
    pub max_batch_source_rows: usize,
    pub query_plans: BTreeMap<String, serde_json::Value>,
}

impl SeasonAudit {
    fn new(header: &SnapshotHeader) -> Self {
        Self {
            method: METHOD_VERSION.into(),
            season: header.season,
            source_snapshot_at: header.source_snapshot_at.clone(),
            generated_at: time::OffsetDateTime::now_utc().to_string(),
            source_sha256: String::new(),
            snapshot_games: header.games,
            eligible_games: 0,
            unsupported_games: 0,
            availability: BTreeMap::new(),
            loaded_games_without_events: 0,
            loaded_games_without_official_toi: 0,
            shift_rows: 0,
            accepted_shift_rows: 0,
            rejected_shift_rows: 0,
            role_fallback_players: 0,
            validation_issues: BTreeMap::new(),
            toi_categories: BTreeMap::new(),
            toi_by_role: BTreeMap::new(),
            toi_difference_histogram: BTreeMap::new(),
            toi_distribution: Distribution::default(),
            events: 0,
            reconstruction: BTreeMap::new(),
            situation_agreement: BTreeMap::new(),
            reconstruction_by_event_type: BTreeMap::new(),
            situation_by_event_type: BTreeMap::new(),
            situation_by_context: BTreeMap::new(),
            mismatch_fields: BTreeMap::new(),
            incomplete_reasons: BTreeMap::new(),
            decoded_field_disagreements: 0,
            resolved_with_matching_counts: 0,
            resolved_matching_counts_without_source_warnings: 0,
            examples: BTreeMap::new(),
            games: vec![],
            read_ms: 0.0,
            analysis_ms: 0.0,
            wall_ms: 0.0,
            max_batch_source_rows: 0,
            query_plans: BTreeMap::new(),
        }
    }

    fn add(&mut self, report: &GameReport, source: &GameSource, differences: &mut Vec<i64>) {
        let availability = if !report.supported {
            "unsupported"
        } else if report.source_rows > 0 {
            "loaded"
        } else {
            match report.game.fetch_status.as_deref() {
                Some("unavailable") => "unavailable",
                Some("failed") => "failed",
                _ => "no_stored_shifts",
            }
        }
        .to_string();
        if report.supported {
            self.eligible_games += 1;
        } else {
            self.unsupported_games += 1;
        }
        *self.availability.entry(availability.clone()).or_default() += 1;
        self.loaded_games_without_events +=
            usize::from(report.source_rows > 0 && report.events.is_empty());
        self.loaded_games_without_official_toi +=
            usize::from(report.source_rows > 0 && source.official_toi.is_empty());
        self.shift_rows += report.source_rows;
        self.accepted_shift_rows += report.accepted_rows;
        self.rejected_shift_rows += report.rejected_rows;
        self.role_fallback_players += report.role_fallback_players;
        for (&key, &count) in &report.validation_counts {
            *self.validation_issues.entry(key).or_default() += count;
        }
        for row in &report.toi {
            *self.toi_categories.entry(row.category).or_default() += 1;
            *self
                .toi_by_role
                .entry(row.role)
                .or_default()
                .entry(row.category)
                .or_default() += 1;
            if let Some(d) = row.difference_seconds {
                // Distribution is restricted to complete accepted interval sets and a unique reference.
                if matches!(
                    row.category,
                    ToiCategory::Exact
                        | ToiCategory::WithinOneSecond
                        | ToiCategory::WithinFiveSeconds
                        | ToiCategory::WithinThirtySeconds
                        | ToiCategory::LargeDifference
                        | ToiCategory::OfficialZeroNoShifts
                ) {
                    differences.push(d);
                    let bin = match d {
                        ..=-31 => "negative_31_plus",
                        -30..=-6 => "negative_6_30",
                        -5..=-2 => "negative_2_5",
                        -1 => "negative_1",
                        0 => "exact",
                        1 => "positive_1",
                        2..=5 => "positive_2_5",
                        6..=30 => "positive_6_30",
                        _ => "positive_31_plus",
                    };
                    *self.toi_difference_histogram.entry(bin.into()).or_default() += 1;
                }
            }
        }
        let mut statuses = BTreeMap::new();
        let mut agreements = BTreeMap::new();
        for event in &report.events {
            self.events += 1;
            *self.reconstruction.entry(event.status).or_default() += 1;
            *statuses.entry(event.status).or_default() += 1;
            *self
                .situation_agreement
                .entry(event.situation_agreement)
                .or_default() += 1;
            *agreements.entry(event.situation_agreement).or_default() += 1;
            *self
                .reconstruction_by_event_type
                .entry(event.event_type.clone())
                .or_default()
                .entry(event.status)
                .or_default() += 1;
            *self
                .situation_by_event_type
                .entry(event.event_type.clone())
                .or_default()
                .entry(event.situation_agreement)
                .or_default() += 1;
            for context in &event.contexts {
                *self
                    .situation_by_context
                    .entry(context.clone())
                    .or_default()
                    .entry(event.situation_agreement)
                    .or_default() += 1;
            }
            for field in &event.mismatch_fields {
                *self.mismatch_fields.entry(field.clone()).or_default() += 1;
            }
            for reason in &event.incomplete_reasons {
                *self.incomplete_reasons.entry(reason.clone()).or_default() += 1;
            }
            self.decoded_field_disagreements +=
                usize::from(event.decoded_fields_agree == Some(false));
            if event.status == ReconstructionStatus::Resolved
                && event.situation_agreement == Agreement::Exact
            {
                self.resolved_with_matching_counts += 1;
                self.resolved_matching_counts_without_source_warnings +=
                    usize::from(event.source_flags.is_empty());
            }
            if event.situation_agreement == Agreement::Mismatch
                || !matches!(
                    event.status,
                    ReconstructionStatus::Resolved | ReconstructionStatus::Ambiguous
                )
            {
                let mut keys = event.contexts.clone();
                keys.push(format!("status_{:?}", event.status).to_lowercase());
                for key in keys {
                    let examples = self.examples.entry(key).or_default();
                    if examples.len() < 3 {
                        examples.push(event.clone());
                    }
                }
            }
        }
        self.games.push(GameSummary {
            game_id: report.game.game_id,
            game_type: report.game.game_type,
            availability,
            latest_fetch_status: report.game.fetch_status.clone(),
            source_sha256: report.source_sha256.clone(),
            shifts: report.source_rows,
            accepted_shifts: report.accepted_rows,
            validation_issues: report.validation.len(),
            official_players: source.official_toi.len(),
            events: report.events.len(),
            reconstruction: statuses,
            situation: agreements,
        });
    }
}

/// Create a new artifact; refusing to overwrite avoids accidentally truncating replay input.
fn writer(path: Option<&Path>) -> Result<Option<BufWriter<File>>, AnyError> {
    path.map(|p| {
        File::options()
            .write(true)
            .create_new(true)
            .open(p)
            .map(BufWriter::new)
    })
    .transpose()
    .map_err(Into::into)
}
fn json_line(writer: &mut Option<BufWriter<File>>, value: &impl Serialize) -> Result<(), AnyError> {
    if let Some(w) = writer {
        serde_json::to_writer(&mut *w, value)?;
        w.write_all(b"\n")?;
    }
    Ok(())
}

struct Accumulator {
    audit: SeasonAudit,
    digest: Sha256,
    differences: Vec<i64>,
    last_game: Option<i64>,
    snapshot: Option<BufWriter<File>>,
    games: Option<BufWriter<File>>,
    events: Option<BufWriter<File>>,
}
impl Accumulator {
    fn new(header: &SnapshotHeader, options: &AuditOptions) -> Result<Self, AnyError> {
        let mut snapshot = writer(options.snapshot_out.as_deref())?;
        json_line(&mut snapshot, header)?;
        Ok(Self {
            audit: SeasonAudit::new(header),
            digest: Sha256::new(),
            differences: vec![],
            last_game: None,
            snapshot,
            games: writer(options.games_out.as_deref())?,
            events: writer(options.events_out.as_deref())?,
        })
    }
    fn add(&mut self, mut source: GameSource) -> Result<(), AnyError> {
        if source.game.season != self.audit.season
            || self.last_game.is_some_and(|id| id >= source.game.game_id)
        {
            return Err(
                "snapshot must contain unique ascending games from the requested season".into(),
            );
        }
        self.last_game = Some(source.game.game_id);
        source.shifts.sort_by_key(|s| s.source_shift_id);
        source.events.sort_by_key(|e| (e.event_id_in_game, e.id));
        source
            .official_toi
            .sort_by_key(|o| (o.player_id, o.player_type.clone()));
        json_line(&mut self.snapshot, &source)?;
        let start = Instant::now();
        let report = analyze(&source);
        self.audit.analysis_ms += start.elapsed().as_secs_f64() * 1000.;
        self.digest.update(report.source_sha256.as_bytes());
        self.audit.add(&report, &source, &mut self.differences);
        for event in &report.events {
            json_line(&mut self.events, event)?;
        }
        // Game details include validation and TOI; event JSONL is independently optional.
        let mut details = report;
        details.events.clear();
        json_line(&mut self.games, &details)?;
        if self.audit.games.len().is_multiple_of(100) {
            eprintln!(
                "audited {} / {} games",
                self.audit.games.len(),
                self.audit.snapshot_games
            );
        }
        Ok(())
    }
    fn finish(mut self, started: Instant) -> Result<SeasonAudit, AnyError> {
        if self.audit.games.len() != self.audit.snapshot_games {
            return Err("incomplete snapshot: game count differs from header".into());
        }
        self.audit.source_sha256 = format!("{:x}", self.digest.finalize());
        if !self.differences.is_empty() {
            let n = self.differences.len();
            let mut abs: Vec<_> = self.differences.iter().map(|d| d.abs()).collect();
            abs.sort_unstable();
            self.audit.toi_distribution = Distribution {
                compared_players: n,
                signed_min_seconds: self.differences.iter().copied().min(),
                signed_max_seconds: self.differences.iter().copied().max(),
                mean_absolute_seconds: Some(abs.iter().sum::<i64>() as f64 / n as f64),
                median_absolute_seconds: Some(abs[(n - 1) / 2]),
                p95_absolute_seconds: Some(abs[(n * 95).div_ceil(100) - 1]),
            };
        }
        for w in [&mut self.snapshot, &mut self.games, &mut self.events]
            .into_iter()
            .flatten()
        {
            w.flush()?;
        }
        self.audit.wall_ms = started.elapsed().as_secs_f64() * 1000.;
        Ok(self.audit)
    }
}

pub async fn run(
    pool: Option<&sqlx::PgPool>,
    options: AuditOptions,
) -> Result<SeasonAudit, AnyError> {
    let started = Instant::now();
    if let Some(path) = &options.input {
        let mut lines = BufReader::new(File::open(path)?).lines();
        let header: SnapshotHeader = serde_json::from_str(&lines.next().ok_or("empty snapshot")??)?;
        if header.snapshot_version != 1 || header.season != options.season {
            return Err("snapshot version/season mismatch".into());
        }
        let mut acc = Accumulator::new(&header, &options)?;
        for line in lines {
            acc.add(serde_json::from_str(&line?)?)?;
        }
        return acc.finish(started);
    }
    let mut tx = read::snapshot(pool.ok_or("database required without --input")?).await?;
    let source_snapshot_at: String = sqlx::query_scalar("SELECT transaction_timestamp()::text")
        .fetch_one(&mut *tx)
        .await?;
    let game_list = read::games(&mut tx, Some(options.season), None).await?;
    let header = SnapshotHeader {
        snapshot_version: 1,
        season: options.season,
        games: game_list.len(),
        source_snapshot_at,
    };
    let mut acc = Accumulator::new(&header, &options)?;
    if options.profile && !game_list.is_empty() {
        acc.audit.query_plans =
            read::profile(&mut tx, &game_list[..game_list.len().min(read::BATCH_SIZE)]).await?;
    }
    for games in game_list.chunks(read::BATCH_SIZE) {
        let start = Instant::now();
        let sources = read::batch(&mut tx, games).await?;
        acc.audit.read_ms += start.elapsed().as_secs_f64() * 1000.;
        acc.audit.max_batch_source_rows = acc.audit.max_batch_source_rows.max(
            sources
                .iter()
                .map(|s| s.shifts.len() + s.events.len() + s.official_toi.len())
                .sum(),
        );
        for source in sources {
            acc.add(source)?;
        }
    }
    tx.commit().await?;
    acc.finish(started)
}

pub async fn game(pool: &sqlx::PgPool, game_id: i64) -> Result<GameReport, AnyError> {
    let mut tx = read::snapshot(pool).await?;
    let games = read::games(&mut tx, None, Some(game_id)).await?;
    if games.is_empty() {
        return Err("completed regular-season or playoff game not found".into());
    }
    let source = read::batch(&mut tx, &games).await?.remove(0);
    let report = analyze(&source);
    tx.commit().await?;
    Ok(report)
}

pub fn write_report(path: Option<&Path>, report: &impl Serialize) -> Result<(), AnyError> {
    if let Some(path) = path {
        let mut w = writer(Some(path))?.unwrap();
        serde_json::to_writer_pretty(&mut w, report)?;
        w.write_all(b"\n")?;
        w.flush()?;
    } else {
        println!("{}", serde_json::to_string_pretty(report)?);
    }
    Ok(())
}
