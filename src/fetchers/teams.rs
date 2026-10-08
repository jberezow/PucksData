//! Fetches franchise records and abbreviations from the NHL stats API.
use crate::{api::fetch_api_text, models::DbTeam, AnyError};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;

#[derive(serde::Deserialize)]
struct FranchiseRecord {
    id: i64,
    #[serde(rename = "fullName")]
    full_name: String,
    #[serde(rename = "teamCommonName")]
    common_name: String,
    #[serde(rename = "teamPlaceName")]
    place_name: String,
}

#[derive(serde::Deserialize)]
struct TeamAbbrevRecord {
    #[serde(rename = "fullName")]
    full_name: String,
    #[serde(rename = "triCode")]
    tri_code: String,
}

/// NHL identity as distinct from the franchise key used by `teams`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamIdentity {
    pub id: i64,
    pub franchise_id: Option<i64>,
    pub tri_code: String,
    pub full_name: String,
}

pub async fn fetch_team_identities() -> Result<Vec<TeamIdentity>, AnyError> {
    let body = fetch_api_text("https://api.nhle.com/stats/rest/en/team?limit=-1").await?;
    let response: ApiResponse<TeamIdentity> = serde_json::from_str(&body)?;
    if response.data.is_empty() {
        return Err("NHL team identity response is empty".into());
    }
    Ok(response.data)
}

#[derive(serde::Deserialize)]
struct ApiResponse<T> {
    data: Vec<T>,
}

/// Fetch all NHL franchise records from the stats API.
pub async fn fetch_teams() -> Result<Vec<DbTeam>, AnyError> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Fetching NHL teams...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let franchise_json =
        fetch_api_text("https://api.nhle.com/stats/rest/en/franchise?limit=-1").await?;
    let franchise_resp: ApiResponse<FranchiseRecord> = serde_json::from_str(&franchise_json)?;

    pb.set_message("Fetching team abbreviations...");

    let abbrev_json = fetch_api_text("https://api.nhle.com/stats/rest/en/team?limit=-1").await?;
    let abbrev_resp: ApiResponse<TeamAbbrevRecord> = serde_json::from_str(&abbrev_json)?;

    // The team endpoint contains one row per franchise era. Matching the current
    // franchise name selects its current abbreviation without relying on ID order.
    let abbrev_map: HashMap<String, String> = abbrev_resp
        .data
        .into_iter()
        .map(|r| (r.full_name, r.tri_code))
        .collect();

    let mut teams = Vec::new();
    for franchise in franchise_resp.data {
        match abbrev_map.get(&franchise.full_name) {
            Some(abbrev) => {
                teams.push(DbTeam {
                    team_id: franchise.id,
                    full_name: franchise.full_name,
                    common_name: franchise.common_name,
                    place_name: franchise.place_name,
                    abbrev: abbrev.clone(),
                });
            }
            None => {
                tracing::warn!(
                    "Warning: franchise id={} '{}' has no triCode match — skipping",
                    franchise.id,
                    franchise.full_name
                );
            }
        }
    }

    let count = teams.len();
    pb.finish_with_message(format!("Fetched {count} teams"));

    Ok(teams)
}

/// Current NHL-hosted branding, keyed by the source abbreviation.
#[derive(Debug)]
pub struct TeamBranding {
    pub abbrev: String,
    pub logo_url: Option<String>,
    pub dark_logo_url: Option<String>,
}

#[derive(serde::Deserialize)]
struct LocalizedName {
    default: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StandingsTeam {
    team_abbrev: LocalizedName,
    team_logo: Option<String>,
    team_logo_dark: Option<String>,
}

fn checked_logo(value: Option<String>) -> Result<Option<String>, crate::AnyError> {
    let Some(value) = value else { return Ok(None) };
    let url = reqwest::Url::parse(&value)?;
    if url.scheme() != "https"
        || url.host_str() != Some("assets.nhle.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().starts_with("/logos/nhl/svg/")
        || !url.path().ends_with(".svg")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("unexpected NHL team logo URL".into());
    }
    Ok(Some(value))
}

fn parse_team_branding(body: &str) -> Result<Vec<TeamBranding>, crate::AnyError> {
    #[derive(serde::Deserialize)]
    struct Standings {
        standings: Vec<StandingsTeam>,
    }
    let response: Standings = serde_json::from_str(body)?;
    if response.standings.is_empty() {
        return Err("NHL team branding response is empty".into());
    }
    let mut seen = std::collections::HashSet::new();
    response
        .standings
        .into_iter()
        .map(|team| {
            let abbrev = team.team_abbrev.default;
            if !(2..=3).contains(&abbrev.len())
                || !abbrev.bytes().all(|c| c.is_ascii_uppercase())
                || !seen.insert(abbrev.clone())
            {
                return Err("invalid or duplicate NHL team abbreviation".into());
            }
            Ok(TeamBranding {
                abbrev,
                logo_url: checked_logo(team.team_logo)?,
                dark_logo_url: checked_logo(team.team_logo_dark)?,
            })
        })
        .collect()
}

/// Fetch authoritative logo URLs rather than assuming a CDN filename convention.
pub async fn fetch_team_branding() -> Result<Vec<TeamBranding>, crate::AnyError> {
    parse_team_branding(&fetch_api_text("https://api-web.nhle.com/v1/standings/now").await?)
}

#[cfg(test)]
mod branding_tests {
    use super::*;

    #[test]
    fn parses_source_variants_and_missing_logos() {
        let rows = parse_team_branding(r#"{"standings":[{"teamAbbrev":{"default":"UTA"},"teamLogo":"https://assets.nhle.com/logos/nhl/svg/UTA_light.svg","teamLogoDark":"https://assets.nhle.com/logos/nhl/svg/UTA_dark.svg"},{"teamAbbrev":{"default":"TOR"}}]}"#).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0]
            .dark_logo_url
            .as_ref()
            .unwrap()
            .ends_with("UTA_dark.svg"));
        assert!(rows[1].logo_url.is_none());
    }

    #[test]
    fn rejects_untrusted_or_ambiguous_source_data() {
        for url in [
            "https://example.com/logos/nhl/svg/TOR.svg",
            "http://assets.nhle.com/logos/nhl/svg/TOR.svg",
            "https://assets.nhle.com/mugs/test.svg",
            "https://assets.nhle.com:444/logos/nhl/svg/TOR.svg",
        ] {
            assert!(checked_logo(Some(url.into())).is_err());
        }
        assert!(parse_team_branding(r#"{"standings":[]}"#).is_err());
        assert!(parse_team_branding(
            r#"{"standings":[{"teamAbbrev":{"default":"UTA"}},{"teamAbbrev":{"default":"UTA"}}]}"#
        )
        .is_err());
    }
}
