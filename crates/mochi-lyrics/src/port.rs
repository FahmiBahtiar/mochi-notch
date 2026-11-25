//! The Rust port of `lyrics/`.

#![forbid(unsafe_code)]

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

const TICKS_PER_SECOND: f64 = 10_000_000.0;
const MAX_TIMESPAN_TICKS: i128 = i64::MAX as i128;

static TIMESTAMP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([0-9]+):([0-9]+(?:\.[0-9]+)?)\]").expect("valid regex"));

static NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\s*[\(\[][^\)\]]*\b(official|video|lyrics?|lirik|audio|visuali[sz]er|klip|clip|remaster(ed)?|hd|4k|mv)\b[^\)\]]*[\)\]]",
    )
    .expect("valid regex")
});

static FEATURING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+[\(\[]?(ft\.?|feat\.?|featuring)\s.*$").expect("valid regex")
});

static VERSION_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+-\s+.*\b(remaster(ed)?|live|version|versi|edit|mix|mono|stereo)\b.*$")
        .expect("valid regex")
});

static CHANNEL_NOISE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(\s*-\s*Topic|VEVO|\s+Official)$").expect("valid regex"));

static BROWSER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)chrome|msedge|firefox|brave|opera|vivaldi").expect("valid regex")
});

#[derive(Debug, Clone)]
struct LyricLine {
    ticks: i128,
    text: String,
}

fn seconds_to_ticks(seconds: f64) -> Result<i128, String> {
    if seconds.is_nan() {
        return Err("TimeSpan does not accept floating point Not-a-Number values.".to_string());
    }
    if seconds.is_infinite() {
        return Err("TimeSpan overflowed because the duration is too long.".to_string());
    }
    let ticks = (seconds * TICKS_PER_SECOND).round();
    if ticks < (i64::MIN as f64) || ticks > (i64::MAX as f64) {
        return Err("TimeSpan overflowed because the duration is too long.".to_string());
    }
    Ok(ticks as i128)
}

fn ticks_to_seconds(ticks: i128) -> f64 {
    (ticks as f64) / TICKS_PER_SECOND
}

fn parse_lines(lrc: &str) -> Option<Vec<LyricLine>> {
    let mut lines = Vec::new();
    for raw in lrc.split('\n') {
        let mut stamps = Vec::new();
        let mut last_end = 0usize;
        for cap in TIMESTAMP_RE.captures_iter(raw) {
            let full = cap.get(0)?;
            last_end = full.end();
            let mins: i64 = cap.get(1)?.as_str().parse().ok()?;
            let secs: f64 = cap.get(2)?.as_str().parse().ok()?;
            let total_secs = (mins as f64) * 60.0 + secs;
            let ticks = seconds_to_ticks(total_secs).ok()?;
            stamps.push(ticks);
        }
        if stamps.is_empty() {
            continue;
        }
        let text = raw[last_end..].trim().to_string();
        for ticks in stamps {
            lines.push(LyricLine {
                ticks,
                text: text.clone(),
            });
        }
    }
    if lines.is_empty() {
        None
    } else {
        lines.sort_by_key(|l| l.ticks);
        Some(lines)
    }
}

fn index_at_lines(lines: &[LyricLine], pos_ticks: i128) -> isize {
    let mut lo: isize = 0;
    let mut hi: isize = lines.len() as isize - 1;
    let mut found: isize = -1;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        if lines[mid as usize].ticks <= pos_ticks {
            found = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    found
}

/// Port of `ParseLyrics` (`lyrics/LyricsUnit.cs:22`).
pub fn parse_lyrics(lrc: String) -> Result<Option<Vec<Value>>, String> {
    let Some(lines) = parse_lines(&lrc) else {
        return Ok(None);
    };
    Ok(Some(
        lines
            .into_iter()
            .map(|l| {
                json!({
                    "TimeSeconds": ticks_to_seconds(l.ticks),
                    "Text": l.text,
                })
            })
            .collect(),
    ))
}

/// Port of `LyricsIndexAt` (`lyrics/LyricsUnit.cs:28`).
pub fn lyrics_index_at(lrc: String, position_seconds: f64) -> Result<i64, String> {
    let Some(lines) = parse_lines(&lrc) else {
        return Ok(-1);
    };
    let pos_ticks = seconds_to_ticks(position_seconds)?;
    Ok(index_at_lines(&lines, pos_ticks) as i64)
}

/// Port of `LyricsUntilNextLine` (`lyrics/LyricsUnit.cs:36`).
pub fn lyrics_until_next_line(lrc: String, position_seconds: f64) -> Result<Option<f64>, String> {
    let Some(lines) = parse_lines(&lrc) else {
        return Ok(None);
    };
    let pos_ticks = seconds_to_ticks(position_seconds)?;
    let next = index_at_lines(&lines, pos_ticks) + 1;
    if (next as usize) < lines.len() {
        let diff = lines[next as usize].ticks - pos_ticks;
        if diff.abs() > MAX_TIMESPAN_TICKS {
            return Err("TimeSpan overflowed".to_string());
        }
        Ok(Some(ticks_to_seconds(diff)))
    } else {
        Ok(None)
    }
}

/// Port of `TrackQueryFrom` (`lyrics/LyricsUnit.cs:44`).
pub fn track_query_from(
    title: String,
    artist: String,
    source_app: String,
) -> Result<Value, String> {
    let mut artist = artist.trim().to_string();
    let mut title = title.trim().to_string();

    let is_browser = BROWSER_RE.is_match(&source_app);
    if let Some(dash) = title.find(" - ")
        && dash > 0
        && (is_browser || artist.is_empty())
    {
        artist = title[..dash].to_string();
        title = title[dash + 3..].to_string();
    }

    let step1 = VERSION_SUFFIX_RE.replace_all(&title, "");
    let step2 = NOISE_RE.replace_all(&step1, "");
    let step3 = FEATURING_RE.replace_all(&step2, "");
    title = step3.trim().to_string();
    artist = CHANNEL_NOISE_RE.replace_all(&artist, "").trim().to_string();

    Ok(json!({
        "Artist": artist,
        "Title": title,
        "FromBrowser": is_browser,
    }))
}

fn dotnet_to_lower_invariant(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let mut iter = c.to_lowercase();
        let first = iter.next();
        if iter.next().is_none()
            && let Some(lc) = first
        {
            out.push(lc);
        } else {
            out.push(c);
        }
    }
    out
}

fn sha1_hex_upper(data: &[u8]) -> String {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (i, word) in w[..16].iter_mut().enumerate() {
            let j = i * 4;
            *word = u32::from_be_bytes([chunk[j], chunk[j + 1], chunk[j + 2], chunk[j + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    format!("{h0:08X}{h1:08X}{h2:08X}{h3:08X}{h4:08X}")
}

/// Port of `LyricsCacheKey` (`lyrics/LyricsUnit.cs:60`).
pub fn lyrics_cache_key(artist: String, title: String) -> Result<String, String> {
    let key = dotnet_to_lower_invariant(&format!("{artist}\n{title}"));
    Ok(sha1_hex_upper(key.as_bytes()))
}

/// Port of `PickBestLyrics` (`lyrics/LyricsUnit.cs:63`).
pub fn pick_best_lyrics(
    json_str: String,
    duration_seconds: f64,
    max_duration_diff: f64,
) -> Result<Option<String>, String> {
    let Ok(doc) = serde_json::from_str::<Value>(&json_str) else {
        return Ok(None);
    };
    let Some(arr) = doc.as_array() else {
        return Ok(None);
    };
    let Ok(dur_ticks) = seconds_to_ticks(duration_seconds) else {
        return Ok(None);
    };
    let dur_secs = ticks_to_seconds(dur_ticks);

    let mut best: Option<String> = None;
    let mut best_diff = f64::MAX;
    for item in arr {
        let Some(synced) = item.get("syncedLyrics").and_then(Value::as_str) else {
            continue;
        };
        let mut diff = 0.0;
        if dur_ticks > 0
            && let Some(d) = item.get("duration").and_then(Value::as_f64)
        {
            diff = (d - dur_secs).abs();
        }
        if diff > max_duration_diff {
            continue;
        }
        if diff < best_diff {
            best_diff = diff;
            best = Some(synced.to_string());
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_sorts_multi_stamps() {
        let lines = parse_lyrics("[00:30.00]Chorus\n[00:10.00][00:50.00]Hook\n".into())
            .unwrap()
            .unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["Text"], "Hook");
        assert_eq!(lines[1]["Text"], "Chorus");
        assert_eq!(lines[2]["Text"], "Hook");
    }
}
