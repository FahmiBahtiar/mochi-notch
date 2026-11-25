//! Pure release version parsing, asset selection, and checksum verification helpers.

#![forbid(unsafe_code)]

use serde_json::Value;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub installer_name: String,
    pub installer_url: String,
    pub installer_size: u64,
    pub checksums_url: String,
}

pub fn is_trusted_release_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return false;
    }
    let Ok(parsed) = Url::parse(trimmed) else {
        return false;
    };
    if !parsed.scheme().eq_ignore_ascii_case("https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host_lower = host.to_ascii_lowercase();
    host_lower == "github.com"
        || host_lower == "api.github.com"
        || host_lower.ends_with(".githubusercontent.com")
}

pub fn parse_latest_release(json_str: &str) -> Option<ReleaseInfo> {
    let doc: Value = serde_json::from_str(json_str).ok()?;
    let raw_tag = doc.get("tag_name").and_then(Value::as_str)?;
    let version = raw_tag.trim().trim_start_matches(['v', 'V']).to_string();
    parse_dotnet_version(&version)?;
    let assets = doc.get("assets").and_then(Value::as_array)?;

    let mut checksums_url: Option<String> = None;
    let mut versioned_installer: Option<(String, String, u64)> = None;
    let mut generic_installer: Option<(String, String, u64)> = None;
    let expected_versioned = format!("Mochi-Setup-{version}.exe");

    for asset in assets {
        let Some(name) = asset.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) else {
            continue;
        };
        if !is_trusted_release_url(url) {
            continue;
        }
        let size = asset.get("size").and_then(Value::as_u64).unwrap_or(0);
        if name.eq_ignore_ascii_case("SHA256SUMS.txt") {
            checksums_url = Some(url.to_string());
        } else if name.eq_ignore_ascii_case(&expected_versioned) {
            versioned_installer = Some((name.to_string(), url.to_string(), size));
        } else if name.eq_ignore_ascii_case("Mochi-Setup.exe") {
            generic_installer = Some((name.to_string(), url.to_string(), size));
        }
    }

    let (installer_name, installer_url, installer_size) =
        versioned_installer.or(generic_installer)?;
    let checksums_url = checksums_url?;

    Some(ReleaseInfo {
        version,
        installer_name,
        installer_url,
        installer_size,
        checksums_url,
    })
}

pub fn parse_sha256_for_file(sums_text: &str, filename: &str) -> Option<String> {
    for raw_line in sums_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let hash = parts.next()?.trim();
        let rest = parts.next()?.trim().trim_start_matches('*').trim();
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        if rest.eq_ignore_ascii_case(filename) {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

fn parse_dotnet_version(s: &str) -> Option<(i32, i32, i32, i32)> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('.').collect();
    if !(2..=4).contains(&parts.len()) {
        return None;
    }
    let mut nums = [0i32; 4];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        nums[i] = p.parse::<i32>().ok()?;
    }
    let build = if parts.len() >= 3 { nums[2] } else { -1 };
    let rev = if parts.len() == 4 { nums[3] } else { -1 };
    Some((nums[0], nums[1], build, rev))
}

pub fn is_newer_version(tag: Option<&str>, current_version: &str) -> bool {
    let Some(cur) = parse_dotnet_version(current_version) else {
        return false;
    };
    let Some(tag) = tag else {
        return false;
    };
    let cleaned = tag.trim().trim_start_matches(['v', 'V']);
    let Some(ver) = parse_dotnet_version(cleaned) else {
        return false;
    };
    let norm = (ver.0, ver.1, ver.2.max(0), -1);
    norm > cur
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_tags_against_current_version() {
        assert!(is_newer_version(Some("v0.2.0"), "0.1.0"));
        assert!(!is_newer_version(Some("v0.1.0"), "0.1.0"));
        assert!(!is_newer_version(None, "0.1.0"));
    }

    #[test]
    fn parses_release_assets_and_rejects_untrusted_urls() {
        let json = r#"{
            "tag_name": "v0.1.1",
            "assets": [
                {
                    "name": "Mochi-Setup-0.1.1.exe",
                    "size": 2993154,
                    "browser_download_url": "https://github.com/FahmiBahtiar/mochi-notch/releases/download/v0.1.1/Mochi-Setup-0.1.1.exe"
                },
                {
                    "name": "SHA256SUMS.txt",
                    "size": 330,
                    "browser_download_url": "https://github.com/FahmiBahtiar/mochi-notch/releases/download/v0.1.1/SHA256SUMS.txt"
                }
            ]
        }"#;
        let info = parse_latest_release(json).expect("valid release");
        assert_eq!(info.version, "0.1.1");
        assert_eq!(info.installer_name, "Mochi-Setup-0.1.1.exe");
        assert_eq!(info.installer_size, 2_993_154);

        let bad_json = json.replace("https://github.com", "https://evil.example.com");
        assert!(parse_latest_release(&bad_json).is_none());
        assert!(!is_trusted_release_url("http://github.com/foo"));
        assert!(!is_trusted_release_url("https://user@github.com/foo"));
        assert!(is_trusted_release_url(
            "https://release-assets.githubusercontent.com/github-production-release-asset/123"
        ));
    }

    #[test]
    fn parses_sha256sums_for_installer() {
        let sums = "F61F33A610DD26525FD773F56D5C5BFE4F538DCB701B7A514BFDDC0B8337CFF5  Mochi-Setup-0.1.1.exe\r\n\
                    654c29ae514868cc6ccd2f8de61d837665b19e58a8620813c5c3c5b606aa334c  Mochi.exe\r\n";
        assert_eq!(
            parse_sha256_for_file(sums, "Mochi-Setup-0.1.1.exe").as_deref(),
            Some("f61f33a610dd26525fd773f56d5c5bfe4f538dcb701b7a514bfddc0b8337cff5")
        );
        assert!(parse_sha256_for_file(sums, "missing.exe").is_none());
    }
}
