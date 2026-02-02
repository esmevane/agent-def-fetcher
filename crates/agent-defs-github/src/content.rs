use serde::Deserialize;

/// Response from GitHub's Contents API.
/// `GET /repos/{owner}/{repo}/contents/{path}`
#[derive(Debug, Deserialize)]
pub struct ContentResponse {
    pub name: String,
    pub path: String,
    pub content: Option<String>,
    pub encoding: Option<String>,
}
