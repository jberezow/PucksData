//! Transactional persistence of complete NHL current-roster observations.

use crate::models::CurrentRosterObservation;

const SOURCE: &str = "https://api-web.nhle.com/v1/roster/{team}/current";

/// Insert one complete roster snapshot and all of its memberships.
///
/// A partial fetch is rejected so downstream consumers can safely treat the
/// latest persisted snapshot as the complete draft-eligibility source.
pub async fn insert_roster_snapshot(
    pool: &sqlx::PgPool,
    observation: &CurrentRosterObservation,
) -> Result<i64, sqlx::Error> {
    if !observation.is_complete() || observation.memberships.is_empty() {
        return Err(sqlx::Error::Protocol(
            "refusing to persist an incomplete or empty roster observation".to_string(),
        ));
    }

    let team_abbrevs: Vec<&str> = observation
        .memberships
        .iter()
        .map(|membership| membership.team_abbrev.as_str())
        .collect();
    let player_ids: Vec<i64> = observation
        .memberships
        .iter()
        .map(|membership| membership.player_id)
        .collect();
    let roster_groups: Vec<&str> = observation
        .memberships
        .iter()
        .map(|membership| membership.roster_group.as_str())
        .collect();
    let position_codes: Vec<Option<&str>> = observation
        .memberships
        .iter()
        .map(|membership| membership.position_code.as_deref())
        .collect();
    let sweater_numbers: Vec<Option<i16>> = observation
        .memberships
        .iter()
        .map(|membership| membership.sweater_number)
        .collect();

    let mut transaction = pool.begin().await?;
    let snapshot_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO roster_snapshots (source, team_count, player_count)
           VALUES ($1, $2, $3)
           RETURNING snapshot_id"#,
    )
    .bind(SOURCE)
    .bind(observation.fetched_team_count as i32)
    .bind(observation.memberships.len() as i32)
    .fetch_one(&mut *transaction)
    .await?;

    let inserted = sqlx::query(
        r#"INSERT INTO roster_memberships
               (snapshot_id, team_id, player_id, roster_group, position_code, sweater_number)
           SELECT $1, teams.team_id, source.player_id, source.roster_group,
                  source.position_code, source.sweater_number
           FROM unnest(
               $2::text[], $3::bigint[], $4::text[], $5::text[], $6::smallint[]
           ) AS source(team_abbrev, player_id, roster_group, position_code, sweater_number)
           JOIN teams ON teams.abbrev = source.team_abbrev"#,
    )
    .bind(snapshot_id)
    .bind(&team_abbrevs)
    .bind(&player_ids)
    .bind(&roster_groups)
    .bind(&position_codes)
    .bind(&sweater_numbers)
    .execute(&mut *transaction)
    .await?
    .rows_affected();

    if inserted != observation.memberships.len() as u64 {
        return Err(sqlx::Error::Protocol(format!(
            "roster snapshot expected {} memberships but matched {inserted} known team rows",
            observation.memberships.len()
        )));
    }

    transaction.commit().await?;
    Ok(snapshot_id)
}
