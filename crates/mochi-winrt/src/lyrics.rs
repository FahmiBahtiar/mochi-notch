use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

pub use mochi_lyrics::port::LyricLine;

use crate::http::{http_request, url_encode};
use crate::paths::AppPaths;

#[derive(Debug, Clone)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
}

impl Lyrics {
    pub fn parse(lrc: &str) -> Option<Self> {
        let lines = mochi_lyrics::port::parse_lyrics(lrc)?;
        if lines.is_empty() {
            None
        } else {
            Some(Self { lines })
        }
    }

    pub fn index_at(&self, position_seconds: f64) -> isize {
        mochi_lyrics::port::lyrics_index_at(&self.lines, position_seconds)
    }

    pub fn until_next_line(&self, position_seconds: f64) -> Option<f64> {
        mochi_lyrics::port::lyrics_until_next_line(&self.lines, position_seconds)
    }
}

pub struct LyricsService {
    cache_dir: PathBuf,
    pruned: AtomicBool,
}

impl LyricsService {
    pub fn new() -> Self {
        Self {
            cache_dir: AppPaths::lyrics_dir(),
            pruned: AtomicBool::new(false),
        }
    }

    pub fn seed(&self, artist: &str, title: &str, lrc: &str) {
        let key = mochi_lyrics::port::lyrics_cache_key(artist, title);
        self.write_cache(&self.cache_dir.join(format!("{key}.lrc")), lrc);
    }

    pub fn find(
        &self,
        title: &str,
        artist: &str,
        source_app: &str,
        duration_secs: f64,
    ) -> Option<Lyrics> {
        let q = mochi_lyrics::port::track_query_from(title, artist, source_app);
        if q.title.is_empty() {
            return None;
        }

        let key = mochi_lyrics::port::lyrics_cache_key(&q.artist, &q.title);
        let cache_file = self.cache_dir.join(format!("{key}.lrc"));
        if let Some(cached) = Self::try_read_cache(&cache_file) {
            return if cached.is_empty() {
                None
            } else {
                Lyrics::parse(&cached)
            };
        }

        let max_diff = if q.from_browser { f64::MAX } else { 4.0 };
        let mut queries = Vec::new();
        if !q.artist.is_empty() {
            queries.push(format!(
                "https://lrclib.net/api/search?track_name={}&artist_name={}",
                url_encode(&q.title),
                url_encode(&q.artist)
            ));
        }
        queries.push(format!(
            "https://lrclib.net/api/search?q={}",
            url_encode(format!("{} {}", q.artist, q.title).trim())
        ));

        let mut failed = false;
        let mut found_lrc: Option<String> = None;
        for url in queries {
            match http_request("GET", &url, &[], None, 10_000) {
                Ok(resp) if (200..300).contains(&resp.status) => {
                    if let Some(best) =
                        mochi_lyrics::port::pick_best_lyrics(&resp.body, duration_secs, max_diff)
                    {
                        found_lrc = Some(best);
                        failed = false;
                        break;
                    }
                }
                _ => failed = true,
            }
        }

        if found_lrc.is_some() || !failed {
            self.write_cache(&cache_file, found_lrc.as_deref().unwrap_or(""));
        }
        found_lrc.as_deref().and_then(Lyrics::parse)
    }

    fn try_read_cache(file: &PathBuf) -> Option<String> {
        let meta = fs::metadata(file).ok()?;
        if meta.len() == 0
            && let Ok(modified) = meta.modified()
            && SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default()
                > Duration::from_secs(3 * 86400)
        {
            return None;
        }
        fs::read_to_string(file).ok()
    }

    fn write_cache(&self, file: &PathBuf, content: &str) {
        let _ = fs::create_dir_all(&self.cache_dir);
        let _ = fs::write(file, content);
        if !self.pruned.swap(true, Ordering::Relaxed) {
            let dir = self.cache_dir.clone();
            std::thread::spawn(move || {
                if let Ok(entries) = fs::read_dir(&dir) {
                    let mut files: Vec<(SystemTime, PathBuf)> = entries
                        .flatten()
                        .filter(|e| e.path().extension().is_some_and(|x| x == "lrc"))
                        .filter_map(|e| {
                            let m = e.metadata().ok()?.modified().ok()?;
                            Some((m, e.path()))
                        })
                        .collect();
                    files.sort_by_key(|a| std::cmp::Reverse(a.0));
                    for (_, old) in files.into_iter().skip(2000) {
                        let _ = fs::remove_file(old);
                    }
                }
            });
        }
    }
}

impl Default for LyricsService {
    fn default() -> Self {
        Self::new()
    }
}
