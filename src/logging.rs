//! Operational diagnostics go to stderr; command results remain on stdout.

pub fn init() -> Result<(), crate::AnyError> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "pucksdata=info".into());
    let format = std::env::var("PUCKSDATA_LOG_FORMAT").unwrap_or_else(|_| "text".into());
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false);
    match format.as_str() {
        "text" => subscriber.try_init()?,
        "json" => subscriber.json().try_init()?,
        _ => return Err("PUCKSDATA_LOG_FORMAT must be text or json".into()),
    }
    Ok(())
}
