use std::collections::HashMap;

/// Lightweight wrapper for endpoint parameter handling.
pub struct ApiParams {
    params: HashMap<String, String>,
}

impl ApiParams {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn add_param(&mut self, key: &str, value: &str) -> &mut Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }

    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    pub fn as_map(&self) -> &HashMap<String, String> {
        &self.params
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!(self.params)
    }
}
