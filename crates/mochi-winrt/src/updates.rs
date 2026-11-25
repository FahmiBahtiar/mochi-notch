use serde_json::Value;

use crate::http::http_request;

pub fn fetch_latest_release_tag() -> Option<String> {
    let resp = http_request(
        "GET",
        "https://api.github.com/repos/FahmiBahtiar/mochi-notch/releases/latest",
        &[("Accept", "application/vnd.github+json")],
        None,
        10_000,
    )
    .ok()?;
    if !(200..300).contains(&resp.status) {
        return None;
    }
    let doc: Value = serde_json::from_str(&resp.body).ok()?;
    doc.get("tag_name")
        .and_then(Value::as_str)
        .map(|s| s.trim().trim_start_matches(['v', 'V']).to_string())
}
