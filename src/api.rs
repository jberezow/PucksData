use reqwest;

pub fn fetch_api_json(url: &str) -> Result<String, reqwest::Error> {
    let response = reqwest::blocking::get(url)?;
    response.text()
}
