use clap::Parser;
use sqlx::{
    migrate::{Migrate, Migrator},
    Connection, PgConnection,
};
use std::error::Error;

#[derive(Parser)]
#[command(about = "Apply the fresh baseline or verified legacy migrations, then pending upgrades")]
struct Args {
    /// Validate the ledger and list pending migrations without changing the database.
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(Args::parse()).await {
        eprintln!("Migration failed: {error}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let url = std::env::var("MIGRATION_DATABASE_URL")
        .map_err(|_| "MIGRATION_DATABASE_URL must be set to a direct PostgreSQL connection")?;
    let mut connection = PgConnection::connect(&url).await?;
    // Keep selection and execution under the same SQLx advisory lock.
    connection.lock().await?;
    let result = migrate(&mut connection, args.dry_run).await;
    connection.unlock().await?;
    result
}

async fn migrate(connection: &mut PgConnection, dry_run: bool) -> Result<(), Box<dyn Error>> {
    let baseline = sqlx::migrate!("./schema/baseline");
    let legacy = sqlx::migrate!("./migrations/legacy");
    let ongoing = sqlx::migrate!("./migrations");
    let has_ledger: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(&mut *connection)
            .await?;
    let applied = if has_ledger {
        if let Some(version) = connection.dirty_version("_sqlx_migrations").await? {
            return Err(format!(
                "Migration {version} is dirty; inspect the failed migration before proceeding"
            )
            .into());
        }
        connection
            .list_applied_migrations("_sqlx_migrations")
            .await?
    } else {
        Vec::new()
    };
    let fresh = applied.is_empty();
    if fresh {
        let nonempty: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema' \
             AND NOT (n.nspname='public' AND c.relname LIKE '_sqlx_migrations%')) \
             OR EXISTS (SELECT 1 FROM pg_namespace WHERE nspname IN ('analytics','history','ingestion','observability')) \
             OR EXISTS (SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public')"
        ).fetch_one(&mut *connection).await?;
        if nonempty {
            return Err("Database has objects but no migration history; refusing to guess its schema version".into());
        }
    }
    let uses_baseline = fresh
        || applied.iter().any(|a| {
            baseline
                .iter()
                .any(|m| m.version == a.version && m.checksum == a.checksum)
        });
    let selected = if uses_baseline { &baseline } else { &legacy };
    let mut migrations: Vec<_> = selected.iter().cloned().collect();
    migrations.extend(ongoing.iter().cloned());
    let mut migrator = Migrator::with_migrations(migrations);
    // Validate before writing anything; also reject missing earlier ledger entries.
    for (index, recorded) in applied.iter().enumerate() {
        let expected = migrator
            .iter()
            .nth(index)
            .ok_or("Migration ledger contains an unknown version")?;
        if recorded.version != expected.version || recorded.checksum != expected.checksum {
            return Err(format!(
                "Migration ledger mismatch at version {}; expected unchanged version {}",
                recorded.version, expected.version
            )
            .into());
        }
    }
    println!(
        "Migration path: {}",
        if uses_baseline { "baseline" } else { "legacy" }
    );
    for migration in migrator.iter().skip(applied.len()) {
        println!(
            "{} {}: {}",
            if dry_run { "Pending" } else { "Applying" },
            migration.version,
            migration.description
        );
    }
    if dry_run {
        println!("Dry run: ledger validated; pending SQL has not been executed or tested.");
    } else {
        migrator.set_locking(false).run(&mut *connection).await?;
        println!("Database is up to date.");
    }
    Ok(())
}
