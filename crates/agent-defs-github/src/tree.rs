use serde::Deserialize;

/// Response from GitHub's Git Trees API.
/// `GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1`
#[derive(Debug, Deserialize)]
pub struct TreeResponse {
    pub sha: String,
    pub tree: Vec<TreeEntry>,
    #[serde(default)]
    pub truncated: bool,
}

/// A single entry in the tree.
#[derive(Debug, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub entry_type: String,
}
