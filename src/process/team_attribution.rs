//! Keep each writer on one accepted, current NHL franchise mapping.
use crate::{fetchers::teams::TeamIdentity, AnyError};
use std::{collections::HashMap, future::Future};

tokio::task_local! {
    static CURRENT_MAPPING: HashMap<i64, i64>;
}

pub fn current_mapping() -> Option<HashMap<i64, i64>> {
    CURRENT_MAPPING.try_with(Clone::clone).ok()
}

/// Fail before metadata or fact writes if a known source identity was reassigned,
/// removed, or lost its franchise. New identities are safe to insert.
pub fn validate_mapping(stored: &[(i64, i64)], source: &[TeamIdentity]) -> Result<(), AnyError> {
    let mut mapping = HashMap::new();
    for row in source {
        if mapping.insert(row.id, row.franchise_id).is_some() {
            return Err(format!("duplicate NHL team identity {}", row.id).into());
        }
    }
    if mapping.is_empty() {
        return Err("NHL team identity response is empty".into());
    }
    let mut changes = Vec::new();
    for &(team, franchise) in stored {
        let current = mapping.get(&team).copied().flatten();
        if current != Some(franchise) {
            changes.push(format!(
                "team {team}: stored franchise {franchise}, NHL {current:?}"
            ));
        }
    }
    if !changes.is_empty() {
        changes.sort();
        return Err(format!(
            "NHL franchise attribution changed: {}. Reconcile affected games and event owners before ingestion; see docs/team-attribution.md",
            changes.join("; ")
        ).into());
    }
    Ok(())
}

/// Called inside the existing writer lease and attempt scope. Fetch once, reject
/// drift, refresh safe identity metadata, then reuse exactly that map throughout
/// the command. Event workers receive the parent's explicit map as before.
pub async fn with_current_mapping<T>(
    pool: &sqlx::PgPool,
    operation: impl Future<Output = Result<T, AnyError>>,
) -> Result<T, AnyError> {
    let identities = crate::fetchers::teams::fetch_team_identities().await?;
    crate::loaders::teams::upsert_team_identities(pool, &identities).await?;
    let mapping = identities
        .into_iter()
        .filter_map(|row| row.franchise_id.map(|franchise| (row.id, franchise)))
        .collect();
    CURRENT_MAPPING.scope(mapping, operation).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(id: i64, franchise: Option<i64>) -> TeamIdentity {
        TeamIdentity {
            id,
            franchise_id: franchise,
            tri_code: "TST".into(),
            full_name: "Test".into(),
        }
    }

    #[test]
    fn rejects_reassignment_removal_null_and_duplicates() {
        for source in [
            vec![identity(33, Some(35))],
            vec![identity(52, Some(35))],
            vec![identity(33, None)],
            vec![identity(33, Some(28)), identity(33, Some(28))],
        ] {
            assert!(validate_mapping(&[(33, 28)], &source).is_err());
        }
        assert!(validate_mapping(&[], &[]).is_err());
        assert!(validate_mapping(
            &[(33, 35)],
            &[identity(33, Some(35)), identity(68, Some(40))]
        )
        .is_ok());
    }

    #[tokio::test]
    async fn scoped_mapping_is_reused_and_does_not_escape() {
        assert!(current_mapping().is_none());
        CURRENT_MAPPING
            .scope(HashMap::from([(33, 35)]), async {
                // No HTTP or database is needed when a writer already has its map.
                let mapping = crate::fetchers::games::fetch_team_id_to_franchise_id_map()
                    .await
                    .unwrap();
                assert_eq!(mapping, HashMap::from([(33, 35)]));
            })
            .await;
        assert!(current_mapping().is_none());
    }
}
