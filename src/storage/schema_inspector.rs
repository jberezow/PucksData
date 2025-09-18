use sqlx::{PgPool, Row};
use std::error::Error;
use serde_json::json;

/// Inspect the current database schema and save to a file
pub async fn inspect_and_save_schema() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("🔍 Connecting to database...");
    
    // Get database URL from environment
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL not found in environment")?;
    
    // Connect to database
    let pool = PgPool::connect(&database_url).await?;
    
    println!("✅ Connected! Inspecting schema...");
    
    // Query to get all table and column information from both public and raw schemas
    let query = r#"
        SELECT 
            table_schema,
            table_name,
            column_name,
            data_type,
            column_default,
            is_nullable,
            character_maximum_length,
            numeric_precision,
            numeric_scale
        FROM information_schema.columns 
        WHERE table_schema IN ('public', 'raw')
        ORDER BY table_schema, table_name, ordinal_position
    "#;
    
    let rows = sqlx::query(query).fetch_all(&pool).await?;
    
    let mut schema_data = Vec::new();
    
    for row in rows {
        let table_schema: String = row.get("table_schema");
        let table_name: String = row.get("table_name");
        let column_name: String = row.get("column_name");
        let data_type: String = row.get("data_type");
        let column_default: Option<String> = row.get("column_default");
        let is_nullable: String = row.get("is_nullable");
        let character_maximum_length: Option<i32> = row.get("character_maximum_length");
        let numeric_precision: Option<i32> = row.get("numeric_precision");
        let numeric_scale: Option<i32> = row.get("numeric_scale");
        
        schema_data.push(json!({
            "table_schema": table_schema,
            "table_name": table_name,
            "column_name": column_name,
            "data_type": data_type,
            "column_default": column_default,
            "is_nullable": is_nullable,
            "character_maximum_length": character_maximum_length,
            "numeric_precision": numeric_precision,
            "numeric_scale": numeric_scale
        }));
    }
    
    // Also get table constraints and indexes
    let constraints_query = r#"
        SELECT 
            tc.table_name,
            tc.constraint_name,
            tc.constraint_type,
            kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu 
            ON tc.constraint_name = kcu.constraint_name
        WHERE tc.table_schema IN ('public', 'raw')
        ORDER BY tc.table_name, tc.constraint_name
    "#;
    
    let constraint_rows = sqlx::query(constraints_query).fetch_all(&pool).await?;
    let mut constraints_data = Vec::new();
    
    for row in constraint_rows {
        let table_name: String = row.get("table_name");
        let constraint_name: String = row.get("constraint_name");
        let constraint_type: String = row.get("constraint_type");
        let column_name: String = row.get("column_name");
        
        constraints_data.push(json!({
            "table_name": table_name,
            "constraint_name": constraint_name,
            "constraint_type": constraint_type,
            "column_name": column_name
        }));
    }
    
    // Create final schema document
    let schema_doc = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "database": "neondb",
        "columns": schema_data,
        "constraints": constraints_data
    });
    
    // Save to file
    let schema_json = serde_json::to_string_pretty(&schema_doc)?;
    tokio::fs::write("current_schema.json", schema_json).await?;
    
    println!("💾 Schema saved to current_schema.json");
    
    // Print summary by schema
    let mut schemas = std::collections::HashMap::new();
    for item in &schema_data {
        if let (Some(schema), Some(table)) = (
            item.get("table_schema").and_then(|v| v.as_str()),
            item.get("table_name").and_then(|v| v.as_str())
        ) {
            schemas.entry(schema).or_insert_with(std::collections::HashSet::new).insert(table);
        }
    }
    
    for (schema_name, tables) in &schemas {
        println!("📊 Schema '{}' - {} tables:", schema_name, tables.len());
        for table in tables {
            println!("  📋 {}.{}", schema_name, table);
        }
    }
    
    pool.close().await;
    Ok(())
}