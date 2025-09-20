use std::collections::HashMap;

use once_cell::sync::Lazy;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use tokio::sync::OnceCell;

use crate::endpoints::{DataType, Endpoint};
use crate::AnyError;

static POOL: OnceCell<PgPool> = OnceCell::const_new();
static MAX_POOL_CONNECTIONS: Lazy<u32> = Lazy::new(|| {
    std::env::var("DB_POOL_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(5)
});

#[derive(Debug, Clone)]
pub struct PayloadRecord {
    pub storage_key: String,
    pub checksum: String,
    pub file_size: i64,
}

#[derive(Debug, Clone)]
pub struct EntityContext {
    pub entity_type: String,
    pub nhl_id: i64,
}

pub async fn get_db_pool() -> Result<&'static PgPool, AnyError> {
    POOL.get_or_try_init(|| async {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL not found in environment")?;

        PgPoolOptions::new()
            .max_connections(*MAX_POOL_CONNECTIONS)
            .connect(&database_url)
            .await
            .map_err(|e| -> AnyError { Box::new(e) })
    })
    .await
}

pub async fn find_payload_record(
    pool: &PgPool,
    entity_type: &str,
    endpoint: &str,
    nhl_id: i64,
) -> Result<Option<PayloadRecord>, AnyError> {
    let nhl_id_i32: i32 = nhl_id
        .try_into()
        .map_err(|_| "nhl_id exceeds 32-bit integer range")?;

    let row = sqlx::query(
        "SELECT storage_key, checksum, file_size\n         FROM raw.payloads\n         WHERE entity_type = $1::raw.entities\n           AND endpoint = $2\n           AND nhl_id = $3::int4",
    )
    .bind(entity_type)
    .bind(endpoint)
    .bind(nhl_id_i32)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| PayloadRecord {
        storage_key: r.get("storage_key"),
        checksum: r.get("checksum"),
        file_size: r.get("file_size"),
    }))
}

pub async fn upsert_payload_record(
    pool: &PgPool,
    context: &EntityContext,
    endpoint: &str,
    storage_key: &str,
    checksum: &str,
    file_size: i64,
) -> Result<(), AnyError> {
    let nhl_id_i32: i32 = context
        .nhl_id
        .try_into()
        .map_err(|_| "nhl_id exceeds 32-bit integer range")?;

    sqlx::query(
        "INSERT INTO raw.payloads (nhl_id, endpoint, storage_key, file_size, checksum, entity_type)\n         VALUES ($1::int4, $2, $3, $4, $5, $6::raw.entities)\n         ON CONFLICT (entity_type, nhl_id, endpoint)\n         DO UPDATE SET\n             storage_key = EXCLUDED.storage_key,\n             file_size = EXCLUDED.file_size,\n             checksum = EXCLUDED.checksum,\n             updated_at = now()",
    )
    .bind(nhl_id_i32)
    .bind(endpoint)
    .bind(storage_key)
    .bind(file_size)
    .bind(checksum)
    .bind(&context.entity_type)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn resolve_entity_context(
    endpoint: &Endpoint,
    params: &HashMap<String, String>,
    pool: &PgPool,
) -> Result<EntityContext, AnyError> {
    let entity_type = endpoint.data_type.as_entity_type().to_string();
    let nhl_id = match endpoint.data_type {
        DataType::Games => params
            .get("game_id")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or_default(),
        DataType::Players => params
            .get("player_id")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or_default(),
        DataType::Teams => resolve_team_identifier(params, pool).await?,
    };

    Ok(EntityContext {
        entity_type,
        nhl_id,
    })
}

async fn resolve_team_identifier(
    params: &HashMap<String, String>,
    pool: &PgPool,
) -> Result<i64, AnyError> {
    if let Some(code) = params.get("team_code") {
        if let Some(nhl_id) = lookup_team_nhl_id(pool, code).await? {
            return Ok(nhl_id as i64);
        }
    }

    Ok(0)
}

async fn lookup_team_nhl_id(pool: &PgPool, team_code: &str) -> Result<Option<i32>, AnyError> {
    let result = sqlx::query("SELECT nhl_id FROM public.teams WHERE abbreviation = $1 LIMIT 1")
        .bind(team_code)
        .fetch_optional(pool)
        .await?;

    Ok(result.map(|row| row.get("nhl_id")))
}
