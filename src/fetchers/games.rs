//! Fetches game metadata (stats + boxscore) and builds the team-ID-to-franchise-ID map.
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    api::{fetch_api_json, ApiError},
    models::DbGame,
    AnyError,
};

/// Generic paginated response wrapper for the NHL stats REST API.
#[derive(Deserialize)]
pub struct StatsApiResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
}

/// Stats /en/game endpoint record.
///
/// The stats API names the visitor fields `visitingTeamId` and
/// `visitingScore`, unlike the boxscore API's nested `awayTeam` object.
#[derive(Deserialize)]
pub struct StatsGameRecord {
    pub id: i64,
    pub season: i32,
    #[serde(rename = "gameDate")]
    pub game_date: String, // "YYYY-MM-DD"
    #[serde(rename = "gameType")]
    pub game_type: i16,
    #[serde(rename = "homeTeamId")]
    pub home_team_id: i64,
    #[serde(rename = "visitingTeamId")]
    pub away_team_id: i64,
    #[serde(rename = "homeScore")]
    pub home_score: Option<i16>,
    #[serde(rename = "visitingScore")]
    pub away_score: Option<i16>,
}

/// Localized name object used by the web API (e.g. venue, venueLocation).
#[derive(Deserialize)]
pub struct LocalizedName {
    pub default: String,
}

/// Team sub-object within a boxscore response.
#[derive(Deserialize)]
pub struct BoxscoreTeam {
    pub id: i64,
    pub score: Option<i16>,
}

/// Boxscore /v1/gamecenter/{id}/boxscore endpoint.
///
/// The web API uses nested `awayTeam` and `homeTeam` objects.
#[derive(Deserialize)]
pub struct BoxscoreGame {
    pub id: i64,
    #[serde(rename = "startTimeUTC")]
    pub start_time_utc: Option<String>, // ISO 8601 UTC string
    #[serde(rename = "gameState")]
    pub game_state: Option<String>,
    pub venue: Option<LocalizedName>,
    #[serde(rename = "venueLocation")]
    pub venue_location: Option<LocalizedName>,
    #[serde(rename = "homeTeam")]
    pub home_team: BoxscoreTeam,
    #[serde(rename = "awayTeam")]
    pub away_team: BoxscoreTeam,
}

/// Deserialization record for the /team endpoint used to build the ID map.
#[derive(Deserialize)]
struct TeamIdRecord {
    id: i64,
    #[serde(rename = "franchiseId")]
    franchise_id: Option<i64>,
}

/// Fetch a map of NHL team ID → franchise ID.
///
/// The stats /game endpoint returns `homeTeamId`/`visitingTeamId` in the NHL team ID space
/// (e.g. VGK=54, SEA=55), but the `teams` table is keyed by franchise ID (e.g. VGK=38, SEA=39).
/// This map is used to translate game team IDs to franchise IDs before DB insertion.
pub async fn fetch_team_id_to_franchise_id_map() -> Result<HashMap<i64, i64>, AnyError> {
    #[derive(Deserialize)]
    struct TeamListResponse {
        data: Vec<TeamIdRecord>,
    }
    let json = fetch_api_json("https://api.nhle.com/stats/rest/en/team?limit=-1").await?;
    let resp: TeamListResponse = serde_json::from_str(&json)?;
    let map = resp
        .data
        .into_iter()
        .filter_map(|r| r.franchise_id.map(|fid| (r.id, fid)))
        .collect();
    Ok(map)
}

/// Fetch all games for a season from the stats endpoint (paginates at limit=500).
///
/// Query field names are `season` and `id`; the otherwise common
/// `seasonId` and `gameId` forms return HTTP 400 here.
pub async fn fetch_games_for_season(season_year: i32) -> Result<Vec<StatsGameRecord>, AnyError> {
    let mut all_games: Vec<StatsGameRecord> = Vec::new();
    let mut start: usize = 0;
    let limit: usize = 500;
    let mut expected_total = None;
    loop {
        let url = format!(
            "https://api.nhle.com/stats/rest/en/game?limit={limit}&start={start}&sort=id&dir=asc&cayenneExp=season%3D{season_year}"
        );
        let json = fetch_api_json(&url).await?;
        let resp: StatsApiResponse<StatsGameRecord> = serde_json::from_str(&json)?;
        if resp.total < 0 || expected_total.is_some_and(|total| total != resp.total) {
            return Err("game catalog changed during pagination; retry required".into());
        }
        expected_total = Some(resp.total);
        let batch_len = resp.data.len();
        if batch_len == 0 && all_games.len() < resp.total as usize {
            return Err("truncated game catalog".into());
        }
        all_games.extend(resp.data);
        if batch_len == 0 || all_games.len() >= resp.total as usize {
            break;
        }
        start += limit;
    }
    let unique: std::collections::HashSet<_> = all_games.iter().map(|g| g.id).collect();
    if unique.len() != all_games.len()
        || all_games.iter().any(|g| g.season != season_year)
        || all_games.len() != expected_total.unwrap_or(0) as usize
    {
        return Err("inconsistent game catalog identity/count".into());
    }
    Ok(all_games)
}

/// Fetch the boxscore for a single game. On serde parse failure returns ApiError::Other(0).
pub async fn fetch_game_boxscore(game_id: i64) -> Result<BoxscoreGame, ApiError> {
    let url = format!("https://api-web.nhle.com/v1/gamecenter/{game_id}/boxscore");
    let json = fetch_api_json(&url).await?;
    serde_json::from_str(&json).map_err(|_e| ApiError::Other(0))
}

/// Fetch the list of all season year integers from the seasons endpoint (for --all mode).
pub async fn fetch_seasons_list() -> Result<Vec<i32>, AnyError> {
    let json = fetch_api_json("https://api-web.nhle.com/v1/season").await?;
    let years: Vec<i32> = serde_json::from_str(&json)?;
    Ok(years)
}

/// Merge a stats record and an optional boxscore into a DbGame.
///
/// `team_id_map` translates NHL team IDs (as returned by the stats /game endpoint's
/// `homeTeamId`/`visitingTeamId` fields) to franchise IDs, which are the primary keys
/// stored in the `teams` table. Without this translation the FK constraint on
/// `games(home_team_id)` / `games(away_team_id)` will fire for any team whose NHL team ID
/// differs from its franchise ID (e.g. VGK: team_id=54, franchise_id=38).
pub fn transform_game(
    stats: &StatsGameRecord,
    boxscore: Option<&BoxscoreGame>,
    team_id_map: &HashMap<i64, i64>,
) -> Result<DbGame, AnyError> {
    use time::macros::format_description;
    let game_date = time::Date::parse(
        &stats.game_date,
        format_description!("[year]-[month]-[day]"),
    )?;

    let (start_time_utc, venue, venue_location, game_state) = match boxscore {
        Some(bs) => {
            if bs.id != stats.id
                || bs.home_team.id != stats.home_team_id
                || bs.away_team.id != stats.away_team_id
            {
                return Err("boxscore game/team identity mismatch".into());
            }
            let ts = bs
                .start_time_utc
                .as_deref()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());
            let v = bs.venue.as_ref().map(|v| v.default.clone());
            let vl = bs.venue_location.as_ref().map(|v| v.default.clone());
            let gs = bs.game_state.clone();
            (ts, v, vl, gs)
        }
        None => (None, None, None, None),
    };

    // Translate NHL team IDs → franchise IDs (teams table primary key).
    //
    // Fallback to raw ID is intentionally NOT used here: non-NHL team IDs (e.g. European
    // exhibition clubs like EHC Red Bull München / id=7509) are absent from both the /team
    // endpoint and the teams table. Inserting the raw ID would always produce an FK violation.
    // Instead we surface an Err so the caller can skip + warn for that game.
    let home_team_id = team_id_map
        .get(&stats.home_team_id)
        .copied()
        .ok_or_else(|| {
            format!(
                "unmapped home_team_id {} for game {}",
                stats.home_team_id, stats.id
            )
        })?;
    let away_team_id = team_id_map
        .get(&stats.away_team_id)
        .copied()
        .ok_or_else(|| {
            format!(
                "unmapped away_team_id {} for game {}",
                stats.away_team_id, stats.id
            )
        })?;

    Ok(DbGame {
        game_id: stats.id,
        season: stats.season,
        game_date,
        start_time_utc,
        home_team_id,
        away_team_id,
        game_type: stats.game_type,
        venue,
        venue_location,
        game_state,
        home_score: stats.home_score,
        away_score: stats.away_score,
    })
}

// ── Bulk enriched fetch ───────────────────────────────────────────────────────

const MAX_CONCURRENT_BOXSCORES: usize = 10;

/// Game types represented by NHL-franchise teams in this database.
fn is_supported_game_type(game_type: i16) -> bool {
    matches!(game_type, 1..=4)
}

/// Enumerate all games for a season, concurrently fetch their boxscores (10-permit semaphore),
/// transform and return game records. Unexpected failures abort the catalog refresh.
pub async fn fetch_games_for_season_enriched(
    season_year: i32,
    pb: &indicatif::ProgressBar,
) -> Result<Vec<DbGame>, AnyError> {
    let team_id_map = fetch_team_id_to_franchise_id_map().await?;
    let mut stats_records = fetch_games_for_season(season_year).await?;
    let fetched_count = stats_records.len();
    stats_records.retain(|game| is_supported_game_type(game.game_type));
    let unsupported_count = fetched_count - stats_records.len();
    if unsupported_count > 0 {
        pb.suspend(|| {
            eprintln!(
                "info: skipped {unsupported_count} out-of-scope game(s) for season {season_year}"
            )
        });
    }

    enrich_games(stats_records, team_id_map, pb).await
}

async fn enrich_games(
    stats_records: Vec<StatsGameRecord>,
    team_id_map: HashMap<i64, i64>,
    pb: &indicatif::ProgressBar,
) -> Result<Vec<DbGame>, AnyError> {
    pb.set_length(stats_records.len() as u64);
    pb.set_message("game enrichment");

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_BOXSCORES));
    let team_id_map = std::sync::Arc::new(team_id_map);
    let mut join_set: tokio::task::JoinSet<
        Result<(StatsGameRecord, Option<BoxscoreGame>), ApiError>,
    > = tokio::task::JoinSet::new();

    for stats in stats_records {
        let sem = semaphore.clone();
        join_set.spawn(crate::provenance::inherit(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            let bs = match fetch_game_boxscore(stats.id).await {
                Ok(b) => Some(b),
                // 404 is expected for unplayed games (e.g. playoff series that ended
                // before game 6 or 7). Silently use stats-only data — no warning needed.
                Err(crate::api::ApiError::NotFound) => None,
                Err(e) => return Err(e),
            };
            Ok((stats, bs))
        }));
    }

    let mut games = Vec::new();
    while let Some(res) = join_set.join_next().await {
        pb.inc(1);
        match res {
            Ok(Ok((stats, bs))) => match transform_game(&stats, bs.as_ref(), &team_id_map) {
                Ok(game) => games.push(game),
                Err(e) if stats.game_type == 1 => pb.suspend(|| {
                    eprintln!("excluded out-of-scope exhibition game {}: {e}", stats.id)
                }),
                Err(e) => return Err(e),
            },
            Ok(Err(e)) => return Err(e.into()),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(games)
}

#[derive(sqlx::FromRow)]
struct StoredSchedule {
    game_id: i64,
    season: i32,
    game_date: time::Date,
    home_team_id: i64,
    away_team_id: i64,
    game_type: i16,
    home_score: Option<i16>,
    away_score: Option<i16>,
    game_state: Option<String>,
    checked_at: Option<time::OffsetDateTime>,
}

fn needs_enrichment(
    fresh: &DbGame,
    stored: Option<&StoredSchedule>,
    audit_from: time::Date,
    today: time::Date,
) -> bool {
    let Some(old) = stored else {
        return true;
    };
    let catalog_changed = (
        fresh.season,
        fresh.game_date,
        fresh.home_team_id,
        fresh.away_team_id,
        fresh.game_type,
        fresh.home_score,
        fresh.away_score,
    ) != (
        old.season,
        old.game_date,
        old.home_team_id,
        old.away_team_id,
        old.game_type,
        old.home_score,
        old.away_score,
    );
    let near_game =
        fresh.game_date >= audit_from && fresh.game_date <= today + time::Duration::days(14);
    let unresolved = fresh.game_date <= today
        && !old
            .game_state
            .as_deref()
            .is_some_and(crate::process::sync::is_game_completed)
        && old.checked_at.is_none_or(|checked| checked.date() < today);
    catalog_changed || near_game || unresolved
}

/// Discover the entire catalog, but enrich only changed/new/nearby/unresolved
/// games plus at most 100 older checks per season. Periodic checks cover fields
/// absent from the catalog (start time, venue, state), including distant games.
/// Checkpoints are persisted by the caller only after accepted game writes.
pub async fn fetch_incremental_games(
    pool: &sqlx::PgPool,
    season: i32,
    audit_from: time::Date,
    today: time::Date,
) -> Result<Vec<DbGame>, AnyError> {
    let map = fetch_team_id_to_franchise_id_map().await?;
    let stats = fetch_games_for_season(season).await?;
    let stored: Vec<StoredSchedule> = sqlx::query_as(
        "SELECT g.game_id,g.season,g.game_date,g.home_team_id,g.away_team_id,
                g.game_type,g.home_score,g.away_score,g.game_state,c.checked_at
         FROM games g LEFT JOIN ingestion.schedule_checks c USING(game_id) WHERE g.season=$1",
    )
    .bind(season)
    .fetch_all(pool)
    .await?;
    let stored: HashMap<_, _> = stored
        .into_iter()
        .map(|game| (game.game_id, game))
        .collect();
    let selected = select_enrichment(stats, &map, &stored, audit_from, today)?;
    enrich_games(selected, map, &indicatif::ProgressBar::hidden()).await
}

fn select_enrichment(
    stats: Vec<StatsGameRecord>,
    map: &HashMap<i64, i64>,
    stored: &HashMap<i64, StoredSchedule>,
    audit_from: time::Date,
    today: time::Date,
) -> Result<Vec<StatsGameRecord>, AnyError> {
    let mut immediate = Vec::new();
    let mut periodic = Vec::new();
    let catalog_count = stats.len();
    for record in stats
        .into_iter()
        .filter(|s| is_supported_game_type(s.game_type))
    {
        let fresh = match transform_game(&record, None, map) {
            Ok(game) => game,
            Err(error) if record.game_type == 1 => {
                eprintln!(
                    "excluded out-of-scope exhibition game {}: {error}",
                    record.id
                );
                continue;
            }
            Err(error) => return Err(error),
        };
        let old = stored.get(&record.id);
        if needs_enrichment(&fresh, old, audit_from, today) {
            immediate.push(record);
        } else {
            let checked = old.and_then(|old| old.checked_at);
            if checked.is_none_or(|checked| checked.date() <= today - time::Duration::days(7)) {
                periodic.push((checked, record));
            }
        }
    }
    periodic.sort_by_key(|(checked, record)| (*checked, record.id));
    let urgent_count = immediate.len();
    immediate.extend(periodic.into_iter().take(100).map(|(_, record)| record));
    println!(
        "[schedule] catalog={catalog_count} immediate={} periodic={} boxscores={}",
        urgent_count,
        immediate.len() - urgent_count,
        immediate.len()
    );
    Ok(immediate)
}

// ── Single game fetch ─────────────────────────────────────────────────────────

/// Fetch a single game by ID (for --game mode). Fetches stats + boxscore and transforms.
pub async fn fetch_single_game(game_id: i64) -> Result<DbGame, AnyError> {
    let team_id_map = fetch_team_id_to_franchise_id_map().await?;
    let url = format!("https://api.nhle.com/stats/rest/en/game?cayenneExp=id%3D{game_id}");
    let json = fetch_api_json(&url).await?;
    let resp: StatsApiResponse<StatsGameRecord> = serde_json::from_str(&json)?;
    let stats = resp
        .data
        .into_iter()
        .next()
        .ok_or_else(|| format!("game {game_id} not found in stats API"))?;
    if stats.id != game_id {
        return Err("game catalog returned a different game".into());
    }
    let bs = match fetch_game_boxscore(game_id).await {
        Ok(value) => Some(value),
        Err(ApiError::NotFound) => None,
        Err(error) => return Err(error.into()),
    };
    transform_game(&stats, bs.as_ref(), &team_id_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    fn catalog(id: i64, day: &str) -> StatsGameRecord {
        StatsGameRecord {
            id,
            season: 20252026,
            game_date: day.into(),
            game_type: 2,
            home_team_id: 1,
            away_team_id: 2,
            home_score: Some(3),
            away_score: Some(2),
        }
    }

    fn stored(stats: &StatsGameRecord) -> StoredSchedule {
        StoredSchedule {
            game_id: stats.id,
            season: stats.season,
            game_date: time::Date::parse(
                &stats.game_date,
                time::macros::format_description!("[year]-[month]-[day]"),
            )
            .unwrap(),
            home_team_id: 11,
            away_team_id: 22,
            game_type: stats.game_type,
            home_score: stats.home_score,
            away_score: stats.away_score,
            game_state: Some("OFF".into()),
            checked_at: Some(datetime!(2026-09-25 12:00 UTC)),
        }
    }

    #[test]
    fn enrichment_keeps_new_changed_recent_upcoming_and_unresolved_games() {
        let map = HashMap::from([(1, 11), (2, 22)]);
        let stats = catalog(1, "2026-06-01");
        let fresh = transform_game(&stats, None, &map).unwrap();
        let mut old = stored(&stats);
        let from = date!(2026 - 09 - 23);
        let today = date!(2026 - 09 - 26);
        assert!(needs_enrichment(&fresh, None, from, today));
        assert!(!needs_enrichment(&fresh, Some(&old), from, today));
        old.home_score = Some(4);
        assert!(needs_enrichment(&fresh, Some(&old), from, today));
        old.home_score = fresh.home_score;
        old.game_state = Some("PPD".into());
        assert!(needs_enrichment(&fresh, Some(&old), from, today));
        old.checked_at = Some(datetime!(2026-09-26 01:00 UTC));
        assert!(!needs_enrichment(&fresh, Some(&old), from, today));
        for day in ["2026-09-23", "2026-09-26", "2026-10-10"] {
            let stats = catalog(2, day);
            let fresh = transform_game(&stats, None, &map).unwrap();
            assert!(needs_enrichment(&fresh, Some(&stored(&stats)), from, today));
        }
    }

    #[test]
    fn periodic_enrichment_is_bounded_oldest_first_and_does_not_limit_urgent_games() {
        let map = HashMap::from([(1, 11), (2, 22)]);
        let mut stats: Vec<_> = (1..=105).map(|id| catalog(id, "2026-12-01")).collect();
        let mut existing: HashMap<_, _> = stats
            .iter()
            .map(|s| {
                let mut old = stored(s);
                old.checked_at = None;
                (s.id, old)
            })
            .collect();
        existing.get_mut(&1).unwrap().checked_at = Some(datetime!(2026-09-01 12:00 UTC));
        stats.push(catalog(200, "2026-12-01")); // New: does not consume audit budget.
        let selected = select_enrichment(
            stats,
            &map,
            &existing,
            date!(2026 - 09 - 23),
            date!(2026 - 09 - 26),
        )
        .unwrap();
        assert_eq!(selected.len(), 101);
        assert_eq!(selected[0].id, 200);
        assert_eq!(selected[1].id, 2); // Never checked precedes even an old check.
        assert_eq!(selected[100].id, 101);
    }

    #[test]
    fn supported_game_types_exclude_international_games() {
        for game_type in 1..=4 {
            assert!(is_supported_game_type(game_type));
        }
        assert!(!is_supported_game_type(9));
        assert!(!is_supported_game_type(18));
    }
}
