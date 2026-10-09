//! Distinguish rejected source snapshots from database failures.

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("invalid snapshot: {0}")]
    Validation(String),
}

/// Expected temporary condition: recurring jobs retry it on their next pass.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Deferred(pub String);

/// Preserve deferral classification only when every error is expected.
pub fn batch_error(context: &str, errors: Vec<crate::AnyError>) -> crate::AnyError {
    let deferred = errors.iter().all(|error| error.is::<Deferred>());
    let message = format!(
        "{context}: {}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    );
    if deferred {
        Box::new(Deferred(message))
    } else {
        message.into()
    }
}

/// Only recurring commands may turn a temporary deferral into a clean exit.
pub fn scheduled_outcome(
    recurring: bool,
    result: Result<(), crate::AnyError>,
) -> Result<(), crate::AnyError> {
    match result {
        Err(error) if recurring && error.is::<Deferred>() => {
            tracing::warn!(error = %error, "scheduled work deferred; retry on next scheduled pass");
            Ok(())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurring_deferrals_exit_cleanly_but_manual_and_real_failures_do_not() {
        assert!(scheduled_outcome(true, Err(Box::new(Deferred("pending".into())))).is_ok());
        assert!(scheduled_outcome(false, Err(Box::new(Deferred("pending".into())))).is_err());
        assert!(scheduled_outcome(true, Err("database unavailable".into())).is_err());
    }

    #[test]
    fn mixed_batch_keeps_real_failures_visible_regardless_of_order() {
        for reverse in [false, true] {
            let mut errors: Vec<crate::AnyError> = vec![
                Box::new(Deferred("report pending".into())),
                "invalid snapshot".into(),
            ];
            if reverse {
                errors.reverse();
            }
            assert!(scheduled_outcome(true, Err(batch_error("batch", errors))).is_err());
        }
        assert!(scheduled_outcome(
            true,
            Err(batch_error(
                "batch",
                vec![Box::new(Deferred("report pending".into()))]
            ))
        )
        .is_ok());
    }
}
