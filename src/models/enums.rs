/// Database enum mappings and utilities

/// Database enum values and mapping utilities
pub struct DbEnums;

impl DbEnums {
    /// Map NHL API handedness to database enum
    pub fn map_handedness(api_value: Option<&str>) -> &'static str {
        match api_value.unwrap_or("NA") {
            "L" => "L",
            "R" => "R",
            _ => "NA",
        }
    }

    /// Map NHL API position to database role enum
    pub fn map_role(api_value: Option<&str>) -> &'static str {
        match api_value.unwrap_or("NA") {
            "C" => "C",
            "L" | "LW" => "LW",
            "R" | "RW" => "RW",
            "D" => "D",
            "G" => "G",
            _ => "NA",
        }
    }

    /// Map NHL API shot type to database enum
    pub fn map_shot_type(api_value: &str) -> &'static str {
        match api_value.to_lowercase().as_str() {
            "slap shot" | "slap-shot" | "slap" => "slap",
            "wrist shot" | "wrist-shot" | "wrist" => "wrist",
            "tip-in" | "tip in" => "tip-in",
            "backhand" => "backhand",
            "snap shot" | "snap-shot" | "snap" => "snap",
            "deflection" | "deflected" => "deflected",
            "wrap-around" | "wrap around" => "wrap-around",
            "cradle" | "michigan" => "cradle",
            _ => "other",
        }
    }

    /// Map NHL API strength to database enum
    pub fn map_strength(api_value: &str) -> &'static str {
        match api_value.to_lowercase().as_str() {
            "even" | "even strength" | "ev" | "5v5" => "ev",
            "power play" | "powerplay" | "pp" | "5v4" | "5v3" | "4v3" => "pp",
            "short handed" | "shorthanded" | "sh" | "4v5" | "3v5" | "3v4" => "sh",
            _ => "na",
        }
    }

    /// Map NHL API goal modifier to database enum
    pub fn map_goal_modifier(api_value: &str) -> &'static str {
        match api_value.to_lowercase().as_str() {
            "" | "none" => "none",
            "empty net" | "empty-net" | "emptynet" => "empty-net",
            "penalty shot" | "penalty-shot" | "penaltyshot" => "penalty-shot",
            _ => "other",
        }
    }

    /// Map NHL API period type to database enum
    pub fn map_period_type(api_value: &str) -> &'static str {
        match api_value.to_lowercase().as_str() {
            "reg" | "regulation" => "REG",
            "ot" | "overtime" => "OT", 
            "so" | "shootout" => "SO",
            _ => "NA",
        }
    }

    /// Map entity type to database enum
    pub fn map_entity_type(api_value: &str) -> &'static str {
        match api_value.to_lowercase().as_str() {
            "team" => "team",
            "player" => "player", 
            "game" => "game",
            "event" => "event",
            _ => "na"
        }
    }
}

/// Enum validation utilities
pub struct EnumValidator;

impl EnumValidator {
    /// Validate handedness enum value
    pub fn is_valid_handedness(value: &str) -> bool {
        matches!(value, "L" | "R" | "NA")
    }

    /// Validate role enum value
    pub fn is_valid_role(value: &str) -> bool {
        matches!(value, "C" | "LW" | "RW" | "D" | "G" | "NA")
    }

    /// Validate shot_type enum value
    pub fn is_valid_shot_type(value: &str) -> bool {
        matches!(value, "slap" | "wrist" | "tip-in" | "backhand" | "snap" | "deflected" | "wrap-around" | "cradle" | "other")
    }

    /// Validate strength enum value
    pub fn is_valid_strength(value: &str) -> bool {
        matches!(value, "ev" | "pp" | "sh" | "na")
    }

    /// Validate goal_modifier enum value
    pub fn is_valid_goal_modifier(value: &str) -> bool {
        matches!(value, "empty-net" | "penalty-shot" | "none" | "other")
    }

    /// Validate period_type enum value
    pub fn is_valid_period_type(value: &str) -> bool {
        matches!(value, "REG" | "OT" | "SO" | "NA" | "REGULATION" | "OVERTIME" | "SHOOTOUT")
    }

    /// Validate entity_type enum value
    pub fn is_valid_entity_type(value: &str) -> bool {
        matches!(value, "team" | "player" | "game" | "event" | "na")
    }
}