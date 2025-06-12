use crate::storage::DbPool;
use time::OffsetDateTime;

pub async fn insert_raw_data(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
    data: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO raw_data (endpoint, parameters, data)
        VALUES ($1, $2, $3)
        "#
    )
    .bind(endpoint_name)
    .bind(params)
    .bind(data)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn raw_data_exists(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*) FROM raw_data
        WHERE endpoint = $1 AND parameters = $2
        "#,
    )
    .bind(endpoint_name)
    .bind(params)
    .fetch_one(pool)
    .await?;

    Ok(count.0 > 0)
}

/// Get raw data from the database
pub async fn get_raw_data(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT data FROM raw_data WHERE endpoint = $1 AND parameters = $2 LIMIT 1",
        endpoint_name,
        params
    )
    .fetch_one(pool)
    .await?;
    
    Ok(row.data)
}

/// Debug function to show what's in the raw_data table
pub async fn inspect_raw_data_table(pool: &DbPool) -> Result<(), sqlx::Error> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM raw_data")
        .fetch_one(pool)
        .await?;
    
    println!("🔍 Total rows in raw_data table: {}", count.0);
    
    // Show breakdown by endpoint
    let endpoint_counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT endpoint, COUNT(*) as count FROM raw_data GROUP BY endpoint ORDER BY count DESC"
    )
    .fetch_all(pool)
    .await?;
    
    println!("📊 Breakdown by endpoint:");
    for (endpoint, count) in endpoint_counts {
        println!("   {} : {} rows", endpoint, count);
    }
    
    // Show some recent entries
    let recent_entries: Vec<(String, serde_json::Value, OffsetDateTime)> = sqlx::query_as(
        "SELECT endpoint, parameters, created_at FROM raw_data ORDER BY created_at DESC LIMIT 10"
    )
    .fetch_all(pool)
    .await?;
    
    println!("🕐 Recent entries:");
    for (endpoint, params, created_at) in recent_entries {
        println!("   {} - {} - {}", created_at, endpoint, params);
    }
    
    Ok(())
} 