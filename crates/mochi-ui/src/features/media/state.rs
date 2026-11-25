use mochi_winrt::lyrics::Lyrics;
use mochi_winrt::media::MediaState;

use crate::d2d::{D2dFactories, FontKind};
use crate::features::companion::state::CompanionMode;
use crate::state::{COMPACT_MEDIA, NotchApp};
use crate::theme::{Color, DEFAULT_ACCENT, MONOCHROME_ACCENT};

impl NotchApp {
    pub(crate) fn media_compact(&self, factories: Option<&D2dFactories>) -> (f64, f64) {
        let text = self
            .compact_incoming
            .as_deref()
            .unwrap_or(&self.compact_text);
        let has_art = self.artwork_frame.is_some();
        let show_music_companion = self.music_companion.mode != CompanionMode::Still;
        let left_w: f64 = if has_art { 20.0 } else { 0.0 };
        let right_w: f64 = if show_music_companion { 24.0 } else { 20.0 };
        let side = left_w.max(right_w);
        let gap = 6.0;
        let text_w = if let Some(f) = factories {
            f64::from(
                f.measure_text(text, FontKind::Ui, 13.0, true, false, 600.0, false)
                    .0,
            )
        } else {
            text.chars().count() as f64 * 7.5
        };
        let min_w = if self.lyrics.is_some() { 180.0 } else { 200.0 };
        let max_w = if self.lyrics.is_some() { 520.0 } else { 380.0 };
        let w = (2.0 * (12.0 + side + gap) + text_w + 10.0).ceil();
        (w.clamp(min_w, max_w), COMPACT_MEDIA.1)
    }

    pub fn apply_media_snapshot(
        &mut self,
        next: Option<MediaState>,
        next_source: Option<String>,
        factories: Option<&D2dFactories>,
    ) {
        self.next_source = next_source;
        let prev_thumb_id = self.media_state.as_ref().map(|m| m.thumbnail_id);
        let next_thumb_id = next.as_ref().map(|m| m.thumbnail_id);
        let prev_track = self
            .media_state
            .as_ref()
            .map(|m| format!("{}\n{}\n{}", m.source_app, m.artist, m.title));
        let next_track = next
            .as_ref()
            .map(|m| format!("{}\n{}\n{}", m.source_app, m.artist, m.title));

        if prev_thumb_id != next_thumb_id {
            if let Some(ref m) = next
                && let Some(ref bytes) = m.thumbnail_bytes
                && let Some(f) = factories
            {
                self.artwork_frame = f.decode_thumbnail_bytes(bytes, 256);
                self.artwork_id = m.thumbnail_id;
                let target = if self.store.data.settings.use_artwork_accent {
                    f.extract_accent_from_bytes(bytes)
                } else {
                    MONOCHROME_ACCENT
                };
                self.set_accent_target(target);
            } else {
                self.artwork_frame = None;
                self.artwork_id = 0;
                let target = if self.store.data.settings.use_artwork_accent {
                    DEFAULT_ACCENT
                } else {
                    MONOCHROME_ACCENT
                };
                self.set_accent_target(target);
            }
        }

        self.media_state = next;
        if let Some(ref mut m) = self.media_state {
            m.thumbnail_bytes = None;
        }
        if self.shots_dir.is_some() || self.reduce_motion {
            let playing = self.media_state.as_ref().is_some_and(|m| m.is_playing);
            self.equalizer_bars = if playing {
                [0.42, 0.88, 0.62, 1.0, 0.48]
            } else {
                [0.18; 5]
            };
        }
        if prev_track != next_track {
            self.lyrics_track = next_track;
            self.set_lyrics(None, factories);
        }
        self.update_lyric_line(factories);
        self.update_compact_media_text(factories);
        self.update_clock(factories);
        self.update_companion();
        self.update_targets(factories);
    }

    pub fn set_accent_target(&mut self, target: Color) {
        if self.accent_to == target {
            return;
        }
        self.accent_from = self.accent;
        self.accent_to = target;
        self.accent_t = if self.reduce_motion { 1.0 } else { 0.0 };
        if self.reduce_motion {
            self.accent = target;
        }
    }

    pub fn set_lyrics(&mut self, lyrics: Option<Lyrics>, factories: Option<&D2dFactories>) {
        let count = lyrics.as_ref().map_or(0, |l| l.lines.len());
        self.lyrics = lyrics;
        self.lyric_index = -1;
        self.lyric_scroll_y = 0.0;
        self.lyric_scroll_from = 0.0;
        self.lyric_scroll_to = 0.0;
        self.lyric_scroll_t = 1.0;
        self.lyric_line_anim = vec![(0.0, 1.0); count];
        self.lyric_line_heights.clear();
        self.lyric_heights_key = (None, 0);
        self.update_lyric_line(factories);
        self.update_compact_media_text(factories);
        self.update_targets(factories);
    }

    pub fn update_lyric_line(&mut self, factories: Option<&D2dFactories>) {
        let (Some(lyrics), Some(media)) = (&self.lyrics, &self.media_state) else {
            return;
        };
        let pos = media.current_position_secs() + 0.2;
        let idx = lyrics.index_at(pos);
        if idx == self.lyric_index {
            return;
        }
        self.lyric_index = idx;
        if self.reduce_motion || !self.expanded {
            for (i, (b, s)) in self.lyric_line_anim.iter_mut().enumerate() {
                *b = if i as isize == idx { 1.0 } else { 0.0 };
                *s = if i as isize == idx { 1.04 } else { 1.0 };
            }
        }
        self.update_compact_media_text(factories);
    }

    pub fn update_compact_media_text(&mut self, factories: Option<&D2dFactories>) {
        let Some(ref m) = self.media_state else {
            return;
        };
        let prev_target = self
            .compact_incoming
            .as_deref()
            .unwrap_or(&self.compact_text)
            .to_string();
        let line = self
            .lyrics
            .as_ref()
            .and_then(|l| {
                if self.lyric_index >= 0 {
                    l.lines
                        .get(self.lyric_index as usize)
                        .map(|x| x.text.as_str())
                } else {
                    None
                }
            })
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&m.title)
            .to_string();
        self.set_compact_text(line);
        let next_target = self
            .compact_incoming
            .as_deref()
            .unwrap_or(&self.compact_text);
        if !self.expanded && !self.hud_active && !self.news_active && next_target != prev_target {
            self.update_targets(factories);
        }
    }
}
