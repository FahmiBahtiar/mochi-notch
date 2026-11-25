use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use mochi_updates::port::{
    ReleaseInfo, is_newer_version, is_trusted_release_url, parse_latest_release,
    parse_sha256_for_file,
};
use sha2::{Digest, Sha256};

use crate::http::{http_download_bytes, http_download_to_file, http_request};
use crate::paths::AppPaths;

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/FahmiBahtiar/mochi-notch/releases/latest";
const MAX_CHECKSUMS_BYTES: usize = 64 * 1024;
const MAX_INSTALLER_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DownloadedUpdate {
    pub version: String,
    pub installer_path: PathBuf,
}

pub fn fetch_latest_release() -> Option<ReleaseInfo> {
    let resp = http_request(
        "GET",
        LATEST_RELEASE_API,
        &[("Accept", "application/vnd.github+json")],
        None,
        10_000,
    )
    .ok()?;
    if !(200..300).contains(&resp.status) {
        return None;
    }
    parse_latest_release(&resp.body)
}

pub fn fetch_latest_release_tag() -> Option<String> {
    fetch_latest_release().map(|r| r.version)
}

fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

fn validate_expected_installer_size(expected: u64) -> Result<(), String> {
    if expected > MAX_INSTALLER_BYTES as u64 {
        return Err("Installer exceeds size limit".to_string());
    }
    Ok(())
}

fn validate_downloaded_installer_size(expected: u64, actual: u64) -> Result<(), String> {
    if expected > 0 && actual != expected {
        return Err("Installer size mismatch".to_string());
    }
    Ok(())
}

pub fn download_and_verify_update(current_version: &str) -> Result<DownloadedUpdate, String> {
    download_and_verify_update_into(current_version, &AppPaths::updates_dir())
}

fn download_and_verify_update_into(
    current_version: &str,
    updates_dir: &Path,
) -> Result<DownloadedUpdate, String> {
    let info = fetch_latest_release()
        .ok_or_else(|| "Could not fetch latest release metadata".to_string())?;
    if !is_newer_version(Some(&info.version), current_version) {
        return Err("Already up to date".to_string());
    }
    validate_expected_installer_size(info.installer_size)?;

    let sums_bytes = http_download_bytes(
        &info.checksums_url,
        &[("Accept", "application/octet-stream")],
        15_000,
        MAX_CHECKSUMS_BYTES,
        is_trusted_release_url,
    )?;
    let sums_text = String::from_utf8_lossy(&sums_bytes);
    let expected_sha = parse_sha256_for_file(&sums_text, &info.installer_name)
        .ok_or_else(|| format!("Missing SHA-256 entry for {}", info.installer_name))?;

    fs::create_dir_all(updates_dir).map_err(|e| e.to_string())?;
    let final_path = updates_dir.join(&info.installer_name);
    let part_path = updates_dir.join(format!("{}.part", info.installer_name));
    let _ = fs::remove_file(&part_path);

    let downloaded_bytes = http_download_to_file(
        &info.installer_url,
        &part_path,
        &[("Accept", "application/octet-stream")],
        60_000,
        MAX_INSTALLER_BYTES,
        is_trusted_release_url,
    )?;
    if let Err(e) = validate_downloaded_installer_size(info.installer_size, downloaded_bytes) {
        let _ = fs::remove_file(&part_path);
        return Err(e);
    }

    let actual_sha = match sha256_file_hex(&part_path) {
        Ok(h) => h,
        Err(e) => {
            let _ = fs::remove_file(&part_path);
            return Err(e);
        }
    };
    if actual_sha != expected_sha {
        let _ = fs::remove_file(&part_path);
        return Err("Installer checksum verification failed".to_string());
    }

    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }
    if let Err(e) = fs::rename(&part_path, &final_path) {
        let _ = fs::remove_file(&part_path);
        return Err(e.to_string());
    }

    Ok(DownloadedUpdate {
        version: info.version,
        installer_path: final_path,
    })
}

pub fn launch_silent_installer(installer_path: &Path) -> Result<(), String> {
    let updates_dir = AppPaths::updates_dir();
    let canon_dir = fs::canonicalize(&updates_dir).map_err(|e| e.to_string())?;
    let canon_exe = fs::canonicalize(installer_path).map_err(|e| e.to_string())?;
    if !canon_exe.starts_with(&canon_dir)
        || !canon_exe
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return Err("Invalid installer path".to_string());
    }
    Command::new(&canon_exe)
        .args(["/SILENT", "/UPDATE=1"])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cleanup_old_installers() {
    let dir = AppPaths::updates_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_file_hex_matches_known_digest() {
        let dir = std::env::temp_dir().join(format!("mochi-upd-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("sample.bin");
        fs::write(&file, b"abc").unwrap();
        let hex = sha256_file_hex(&file).unwrap();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_silent_installer_rejects_paths_outside_updates_dir() {
        let outside = std::env::temp_dir().join("not-in-updates.exe");
        let _ = fs::write(&outside, b"");
        let err = launch_silent_installer(&outside).unwrap_err();
        let _ = fs::remove_file(&outside);
        assert!(err.contains("Invalid") || err.contains("cannot find"));
    }

    #[test]
    fn installer_size_validation_enforces_limit_and_exact_match() {
        assert!(validate_expected_installer_size(3_000_000).is_ok());
        assert!(validate_expected_installer_size((MAX_INSTALLER_BYTES as u64) + 1).is_err());
        assert!(validate_downloaded_installer_size(0, 1234).is_ok());
        assert!(validate_downloaded_installer_size(3_000_000, 3_000_000).is_ok());
        assert!(validate_downloaded_installer_size(3_000_000, 2_999_999).is_err());
    }

    #[test]
    #[ignore = "hits live GitHub Releases API and downloads installer"]
    fn live_download_and_verify_latest_release() {
        let dir = std::env::temp_dir().join(format!("mochi-live-upd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let res = download_and_verify_update_into("0.0.1", &dir).expect("download and verify");
        assert!(res.installer_path.exists());
        assert!(fs::metadata(&res.installer_path).unwrap().len() > 1_000_000);
        let _ = fs::remove_dir_all(&dir);
    }
}
