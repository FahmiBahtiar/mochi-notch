//! Pure release version parsing and update comparison.

#![forbid(unsafe_code)]

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

pub fn is_newer_version(tag: Option<String>, current_version: String) -> Result<bool, String> {
    let Some(cur) = parse_dotnet_version(&current_version) else {
        return Ok(false);
    };
    let Some(tag) = tag else {
        return Ok(false);
    };
    let cleaned = tag.trim().trim_start_matches(['v', 'V']);
    let Some(ver) = parse_dotnet_version(cleaned) else {
        return Ok(false);
    };
    let norm = (ver.0, ver.1, ver.2.max(0), -1);
    Ok(norm > cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_tags_against_current_version() {
        assert!(is_newer_version(Some("v0.2.0".into()), "0.1.0".into()).unwrap());
        assert!(!is_newer_version(Some("v0.1.0".into()), "0.1.0".into()).unwrap());
        assert!(!is_newer_version(None, "0.1.0".into()).unwrap());
    }
}
