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
        DataType::Games => parse_required_identifier(params, "game_id", "game")?,
        DataType::Players => parse_required_identifier(params, "player_id", "player")?,
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
    let team_code = params.get("team_code").ok_or_else(|| -> AnyError {
        "Missing required parameter 'team_code' for team endpoint".into()
    })?;

    let nhl_id = lookup_team_nhl_id(pool, team_code).await?;
    handle_team_lookup_result(team_code, nhl_id)
}

async fn lookup_team_nhl_id(pool: &PgPool, team_code: &str) -> Result<Option<i32>, AnyError> {
    let result = sqlx::query("SELECT nhl_id FROM public.teams WHERE abbreviation = $1 LIMIT 1")
        .bind(team_code)
        .fetch_optional(pool)
        .await?;

    Ok(result.map(|row| row.get("nhl_id")))
}

fn parse_required_identifier(
    params: &HashMap<String, String>,
    key: &str,
    entity_name: &str,
) -> Result<i64, AnyError> {
    let raw_value = params.get(key).ok_or_else(|| -> AnyError {
        format!(
            "Missing required parameter '{}' for {} endpoint",
            key, entity_name
        )
        .into()
    })?;

    raw_value.parse::<i64>().map_err(|_| -> AnyError {
        format!("Invalid value '{}' for parameter '{}'", raw_value, key).into()
    })
}

fn handle_team_lookup_result(team_code: &str, nhl_id: Option<i32>) -> Result<i64, AnyError> {
    nhl_id.map(|value| value as i64).ok_or_else(|| -> AnyError {
        format!("No team found for abbreviation '{}'", team_code).into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoints::{DataType, Endpoint};
    use sqlx::postgres::PgPoolOptions;

    fn test_endpoint(data_type: DataType) -> Endpoint {
        Endpoint {
            name: "test",
            url: "https://example.com",
            description: "",
            data_type,
            implemented: true,
            cli_name: "test",
            parameters: vec![],
            test_params: HashMap::new(),
            example: "",
        }
    }

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://postgres:password@localhost/test")
            .expect("lazy pool")
    }

    #[tokio::test]
    async fn rejects_invalid_game_id() {
        let endpoint = test_endpoint(DataType::Games);
        let pool = lazy_pool();
        let mut params = HashMap::new();
        params.insert("game_id".to_string(), "invalid".to_string());

        let err = resolve_entity_context(&endpoint, &params, &pool)
            .await
            .expect_err("invalid game id should error");

        assert!(err
            .to_string()
            .contains("Invalid value 'invalid' for parameter 'game_id'"));
    }

    #[tokio::test]
    async fn rejects_invalid_player_id() {
        let endpoint = test_endpoint(DataType::Players);
        let pool = lazy_pool();
        let mut params = HashMap::new();
        params.insert("player_id".to_string(), "oops".to_string());

        let err = resolve_entity_context(&endpoint, &params, &pool)
            .await
            .expect_err("invalid player id should error");

        assert!(err
            .to_string()
            .contains("Invalid value 'oops' for parameter 'player_id'"));
    }

    #[tokio::test]
    async fn rejects_missing_team_code() {
        let endpoint = test_endpoint(DataType::Teams);
        let pool = lazy_pool();
        let params = HashMap::new();

        let err = resolve_entity_context(&endpoint, &params, &pool)
            .await
            .expect_err("missing team_code should error");

        assert!(err
            .to_string()
            .contains("Missing required parameter 'team_code' for team endpoint"));
    }

    #[test]
    fn rejects_unknown_team_code() {
        let err =
            handle_team_lookup_result("XYZ", None).expect_err("unknown team code should error");

        assert!(err
            .to_string()
            .contains("No team found for abbreviation 'XYZ'"));
    }
}
