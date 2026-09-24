//! Transactional normalized snapshots. Volatile observation times and surrogate
//! event IDs are excluded so identical refreshes do not manufacture revisions.

pub async fn lock_game(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: i64,
) -> Result<(), sqlx::Error> {
    crate::provenance::set_transaction(tx).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("pucksdata:game:{game_id}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn events(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"SELECT history.record('events', $1::text,
        COALESCE(jsonb_agg(
            (to_jsonb(e) - 'id') || jsonb_build_object(
                'goal', to_jsonb(g) - 'event_id',
                'shot', to_jsonb(s) - 'event_id',
                'hit', to_jsonb(h) - 'event_id',
                'block', to_jsonb(b) - 'event_id',
                'penalty', to_jsonb(p) - 'event_id',
                'faceoff', to_jsonb(f) - 'event_id'
            ) ORDER BY e.event_id_in_game), '[]'::jsonb))
        FROM events e LEFT JOIN goals g ON g.event_id = e.id
        LEFT JOIN shots s ON s.event_id = e.id LEFT JOIN hits h ON h.event_id = e.id
        LEFT JOIN blocks b ON b.event_id = e.id LEFT JOIN penalties p ON p.event_id = e.id
        LEFT JOIN faceoffs f ON f.event_id = e.id WHERE e.game_id = $1::bigint"#,
    )
    .bind(game_id.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn official_games(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: i64,
) -> Result<(), sqlx::Error> {
    official_snapshot(tx, game_id, "normalized-v1").await
}

/// Preserve the currently accepted state on first replacement after upgrading.
/// This is an observation now, never a backdated NHL source observation.
pub async fn official_baseline(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: i64,
) -> Result<(), sqlx::Error> {
    let needed: bool = sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM history.snapshots WHERE dataset='official_games' AND entity_key=$1::text)
        AND (EXISTS (SELECT 1 FROM analytics.official_skater_games WHERE game_id=$1::bigint)
             OR EXISTS (SELECT 1 FROM analytics.official_goalie_games WHERE game_id=$1::bigint))")
        .bind(game_id.to_string()).fetch_one(&mut **tx).await?;
    if needed {
        official_snapshot(tx, game_id, "existing-v1").await?;
    }
    Ok(())
}

async fn official_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: i64,
    method: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"SELECT history.record('official_games', $1::text, jsonb_build_object(
        'skaters', (SELECT COALESCE(jsonb_agg(to_jsonb(s) - ARRAY['source_observed_at','updated_at'] ORDER BY player_id), '[]')
                    FROM analytics.official_skater_games s WHERE game_id = $1::bigint),
        'goalies', (SELECT COALESCE(jsonb_agg(to_jsonb(g) - ARRAY['source_observed_at','updated_at'] ORDER BY player_id), '[]')
                    FROM analytics.official_goalie_games g WHERE game_id = $1::bigint),
        'scoring', (SELECT COALESCE(jsonb_agg(jsonb_build_object('player_id',player_id,'stat_code',stat_code,'stat_value',stat_value)
                    ORDER BY player_id, stat_code), '[]') FROM analytics.official_player_game_stats WHERE game_id = $1::bigint)
    ), $2)"#).bind(game_id.to_string()).bind(method).execute(&mut **tx).await?;
    Ok(())
}

pub async fn shifts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"SELECT history.record('shifts', $1::text,
        COALESCE(jsonb_agg(to_jsonb(s) - 'ingested_at' ORDER BY source_shift_id), '[]'))
        FROM shifts s WHERE game_id = $1::bigint"#,
    )
    .bind(game_id.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}
