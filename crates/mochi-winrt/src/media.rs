use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::json;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

#[derive(Debug, Clone)]
pub struct MediaState {
    pub title: String,
    pub artist: String,
    pub source_app: String,
    pub is_playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub position_updated_at: Instant,
    pub playback_rate: f64,
    pub thumbnail_bytes: Option<Vec<u8>>,
    pub thumbnail_id: u64,
}

impl MediaState {
    pub fn current_position_secs(&self) -> f64 {
        let mut pos = self.position_secs;
        if self.is_playing {
            pos += self.position_updated_at.elapsed().as_secs_f64() * self.playback_rate;
        }
        if pos < 0.0 {
            pos = 0.0;
        }
        if self.duration_secs > 0.0 && pos > self.duration_secs {
            self.duration_secs
        } else {
            pos
        }
    }
}

#[derive(Default)]
struct PlayerChoice {
    playing: HashMap<String, bool>,
    removed: HashSet<String>,
    chosen: Option<String>,
}

impl PlayerChoice {
    fn is_removed(&self, id: &str) -> bool {
        self.removed.contains(id)
    }

    fn seen(&mut self, id: &str, playing: bool) -> bool {
        let started = playing && self.playing.get(id).copied() == Some(false);
        self.playing.insert(id.to_string(), playing);
        if !started {
            return false;
        }
        let mut changed = self.removed.remove(id);
        if self.chosen.as_deref().is_some_and(|c| c != id) {
            self.chosen = None;
            changed = true;
        }
        changed
    }

    fn choose(&mut self, id: &str) {
        self.chosen = Some(id.to_string());
        self.removed.remove(id);
    }

    fn remove(&mut self, id: &str) {
        self.removed.insert(id.to_string());
        if self.chosen.as_deref() == Some(id) {
            self.chosen = None;
        }
    }

    fn keep(&mut self, open: &HashSet<String>) {
        self.playing.retain(|k, _| open.contains(k));
        self.removed.retain(|k| open.contains(k));
        if self.chosen.as_deref().is_some_and(|c| !open.contains(c)) {
            self.chosen = None;
        }
    }
}

#[derive(Debug, Clone)]
pub enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
    Seek(f64),
    SwitchToNext,
    RemoveCurrent,
    Refresh,
}

#[derive(Debug, Clone)]
pub struct MediaSnapshot {
    pub current: Option<MediaState>,
    pub next_source: Option<String>,
}

pub struct MediaService {
    snapshot: Arc<Mutex<MediaSnapshot>>,
    simulated: Arc<Mutex<bool>>,
    cmd_tx: std::sync::mpsc::Sender<MediaCommand>,
}

impl MediaService {
    pub fn start(on_change: Arc<dyn Fn() + Send + Sync>) -> Self {
        let snapshot = Arc::new(Mutex::new(MediaSnapshot {
            current: None,
            next_source: None,
        }));
        let simulated = Arc::new(Mutex::new(false));
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<MediaCommand>();

        let snap_bg = Arc::clone(&snapshot);
        let sim_bg = Arc::clone(&simulated);
        let tx_bg = cmd_tx.clone();
        std::thread::Builder::new()
            .name("mochi-media".to_string())
            .stack_size(256 * 1024)
            .spawn(move || {
                // SAFETY: Initializes COM MTA on this dedicated worker thread.
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                run_media_loop(snap_bg, sim_bg, tx_bg, cmd_rx, on_change);
            })
            .ok();

        Self {
            snapshot,
            simulated,
            cmd_tx,
        }
    }

    pub fn snapshot(&self) -> MediaSnapshot {
        self.snapshot
            .lock()
            .map(|g| g.clone())
            .unwrap_or(MediaSnapshot {
                current: None,
                next_source: None,
            })
    }

    pub fn toggle_play_pause(&self) {
        let _ = self.cmd_tx.send(MediaCommand::TogglePlayPause);
    }

    pub fn next(&self) {
        let _ = self.cmd_tx.send(MediaCommand::Next);
    }

    pub fn previous(&self) {
        let _ = self.cmd_tx.send(MediaCommand::Previous);
    }

    pub fn seek(&self, position_secs: f64) {
        if let Ok(mut snap) = self.snapshot.lock()
            && let Some(cur) = snap.current.as_mut()
        {
            cur.position_secs = position_secs;
            cur.position_updated_at = Instant::now();
        }
        let _ = self.cmd_tx.send(MediaCommand::Seek(position_secs));
    }

    pub fn switch_to_next(&self) {
        let _ = self.cmd_tx.send(MediaCommand::SwitchToNext);
    }

    pub fn remove_current(&self) {
        let _ = self.cmd_tx.send(MediaCommand::RemoveCurrent);
    }

    pub fn simulate(
        &self,
        title: &str,
        artist: &str,
        duration_secs: f64,
        position_secs: f64,
        thumbnail_bytes: Option<Vec<u8>>,
    ) {
        if let Ok(mut sim) = self.simulated.lock() {
            *sim = true;
        }
        if let Ok(mut snap) = self.snapshot.lock() {
            let thumb_id = snap.current.as_ref().map_or(1, |c| c.thumbnail_id + 1);
            snap.current = Some(MediaState {
                title: title.to_string(),
                artist: artist.to_string(),
                source_app: String::new(),
                is_playing: true,
                position_secs,
                duration_secs,
                position_updated_at: Instant::now(),
                playback_rate: 1.0,
                thumbnail_bytes,
                thumbnail_id: thumb_id,
            });
            snap.next_source = None;
        }
    }
}

fn app_id_of(session: &GlobalSystemMediaTransportControlsSession) -> Option<String> {
    session
        .SourceAppUserModelId()
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn is_playing(session: &GlobalSystemMediaTransportControlsSession) -> bool {
    session
        .GetPlaybackInfo()
        .and_then(|p| p.PlaybackStatus())
        .is_ok_and(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
}

fn same_session(
    a: &GlobalSystemMediaTransportControlsSession,
    b: &GlobalSystemMediaTransportControlsSession,
) -> bool {
    match (app_id_of(a), app_id_of(b)) {
        (Some(ia), Some(ib)) => ia == ib,
        _ => false,
    }
}

fn read_thumbnail_bytes(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Vec<u8>> {
    let thumb = props.Thumbnail().ok()?;
    let stream = thumb.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 16 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    let loaded = reader.LoadAsync(size).ok()?.join().ok()?;
    let mut buf = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(buf)
}

fn run_media_loop(
    snapshot: Arc<Mutex<MediaSnapshot>>,
    simulated: Arc<Mutex<bool>>,
    cmd_tx: std::sync::mpsc::Sender<MediaCommand>,
    cmd_rx: std::sync::mpsc::Receiver<MediaCommand>,
    on_change: Arc<dyn Fn() + Send + Sync>,
) {
    let Ok(op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() else {
        return;
    };
    let Ok(manager) = op.join() else {
        return;
    };

    let tx_cur = cmd_tx.clone();
    let _ = manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx_cur.send(MediaCommand::Refresh);
        Ok(())
    }));
    let tx_ses = cmd_tx.clone();
    let _ = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx_ses.send(MediaCommand::Refresh);
        Ok(())
    }));

    let mut choice = PlayerChoice::default();
    let mut untitled: HashSet<String> = HashSet::new();
    let mut active_session: Option<GlobalSystemMediaTransportControlsSession> = None;
    let mut watched_ids: HashSet<String> = HashSet::new();
    let mut thumb_seq: u64 = 1;
    let mut last_thumb_key: Option<(String, String, String)> = None;
    let mut cached_thumb: Option<Vec<u8>> = None;

    let _ = cmd_tx.send(MediaCommand::Refresh);

    while let Ok(cmd) = cmd_rx.recv() {
        if simulated.lock().is_ok_and(|g| *g) {
            continue;
        }
        match cmd {
            MediaCommand::TogglePlayPause => {
                if let Some(s) = &active_session
                    && let Ok(op) = s.TryTogglePlayPauseAsync()
                {
                    let _ = op.join();
                }
            }
            MediaCommand::Next => {
                if let Some(s) = &active_session
                    && let Ok(op) = s.TrySkipNextAsync()
                {
                    let _ = op.join();
                }
            }
            MediaCommand::Previous => {
                if let Some(s) = &active_session
                    && let Ok(op) = s.TrySkipPreviousAsync()
                {
                    let _ = op.join();
                }
            }
            MediaCommand::Seek(secs) => {
                if let Some(s) = &active_session {
                    let ticks = (secs * 10_000_000.0).round() as i64;
                    if let Ok(op) = s.TryChangePlaybackPositionAsync(ticks) {
                        let _ = op.join();
                    }
                }
            }
            MediaCommand::SwitchToNext => {
                let sessions = list_sessions(&manager);
                let switchable: Vec<_> = sessions
                    .iter()
                    .filter(|s| {
                        app_id_of(s)
                            .is_some_and(|id| !choice.is_removed(&id) && !untitled.contains(&id))
                    })
                    .collect();
                if switchable.len() >= 2 {
                    let idx = active_session
                        .as_ref()
                        .and_then(|cur| switchable.iter().position(|s| same_session(s, cur)))
                        .map_or(0, |i| (i + 1) % switchable.len());
                    if let Some(id) = app_id_of(switchable[idx]) {
                        choice.choose(&id);
                    }
                }
                let _ = cmd_tx.send(MediaCommand::Refresh);
            }
            MediaCommand::RemoveCurrent => {
                if let Some(cur) = &active_session
                    && let Some(id) = app_id_of(cur)
                {
                    choice.seen(&id, is_playing(cur));
                    choice.remove(&id);
                }
                let _ = cmd_tx.send(MediaCommand::Refresh);
            }
            MediaCommand::Refresh => {
                let sessions = list_sessions(&manager);
                for s in &sessions {
                    if let Some(id) = app_id_of(s)
                        && watched_ids.insert(id)
                    {
                        let tx1 = cmd_tx.clone();
                        let _ = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                            let _ = tx1.send(MediaCommand::Refresh);
                            Ok(())
                        }));
                        let tx2 = cmd_tx.clone();
                        let _ = s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                            let _ = tx2.send(MediaCommand::Refresh);
                            Ok(())
                        }));
                        let tx3 = cmd_tx.clone();
                        let _ =
                            s.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
                                let _ = tx3.send(MediaCommand::Refresh);
                                Ok(())
                            }));
                    }
                }

                let open: HashSet<String> = sessions.iter().filter_map(app_id_of).collect();
                untitled.retain(|id| open.contains(id));
                choice.keep(&open);
                watched_ids.retain(|id| open.contains(id));

                let mut players = Vec::new();
                for s in &sessions {
                    let id = app_id_of(s);
                    let playing = is_playing(s);
                    if let Some(ref id_str) = id {
                        choice.seen(id_str, playing);
                    }
                    if id.as_ref().is_none_or(|i| !choice.is_removed(i)) {
                        players.push((s.clone(), id, playing));
                    }
                }

                let chosen = choice.chosen.as_ref().and_then(|cid| {
                    players
                        .iter()
                        .find(|(_, id, _)| id.as_deref() == Some(cid.as_str()))
                        .map(|(s, _, _)| s.clone())
                });

                let picked = if let Some(c) = chosen {
                    Some(c)
                } else {
                    let sys_cur = manager
                        .GetCurrentSession()
                        .ok()
                        .filter(|cur| app_id_of(cur).is_none_or(|id| !choice.is_removed(&id)));
                    let candidates: Vec<_> = players
                        .iter()
                        .map(|(s, id, playing)| {
                            let is_shown = active_session
                                .as_ref()
                                .is_some_and(|cur| same_session(s, cur));
                            let is_sys = sys_cur.as_ref().is_some_and(|cur| same_session(s, cur));
                            let is_unt = id.as_ref().is_some_and(|i| untitled.contains(i));
                            json!({
                                "IsPlaying": *playing,
                                "IsShown": is_shown,
                                "IsSystemCurrent": is_sys,
                                "IsUntitled": is_unt,
                            })
                        })
                        .collect();
                    let idx = mochi_media::port::pick_media_session(Some(candidates)).unwrap_or(-1);
                    if idx >= 0 && (idx as usize) < players.len() {
                        Some(players[idx as usize].0.clone())
                    } else {
                        sys_cur
                    }
                };

                active_session = picked;

                let next_source = {
                    let switchable: Vec<_> = sessions
                        .iter()
                        .filter(|s| {
                            app_id_of(s).is_some_and(|id| {
                                !choice.is_removed(&id) && !untitled.contains(&id)
                            })
                        })
                        .collect();
                    if switchable.len() >= 2 {
                        let idx = active_session
                            .as_ref()
                            .and_then(|cur| switchable.iter().position(|s| same_session(s, cur)))
                            .map_or(0, |i| (i + 1) % switchable.len());
                        app_id_of(switchable[idx])
                    } else {
                        None
                    }
                };

                let new_state = active_session.as_ref().and_then(|s| {
                    let id = app_id_of(s).unwrap_or_default();
                    let props = s.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
                    let title = props.Title().ok()?.to_string();
                    if title.trim().is_empty() {
                        if !id.is_empty() {
                            untitled.insert(id);
                        }
                        return None;
                    }
                    untitled.remove(&id);
                    let artist = props
                        .Artist()
                        .ok()
                        .map(|a| a.to_string())
                        .unwrap_or_default();
                    let key = (id.clone(), title.clone(), artist.clone());
                    if last_thumb_key.as_ref() != Some(&key) {
                        last_thumb_key = Some(key);
                        cached_thumb = read_thumbnail_bytes(&props);
                        thumb_seq += 1;
                    } else if cached_thumb.is_none()
                        && let Some(bytes) = read_thumbnail_bytes(&props)
                    {
                        cached_thumb = Some(bytes);
                        thumb_seq += 1;
                    }
                    let pb = s.GetPlaybackInfo().ok();
                    let playing = pb
                        .as_ref()
                        .and_then(|p| p.PlaybackStatus().ok())
                        .is_some_and(|st| {
                            st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
                        });
                    let rate = pb
                        .as_ref()
                        .and_then(|p| p.PlaybackRate().ok())
                        .and_then(|r| r.Value().ok())
                        .unwrap_or(1.0);
                    let tl = s.GetTimelineProperties().ok();
                    let mut pos_secs = tl
                        .as_ref()
                        .and_then(|t| t.Position().ok())
                        .map_or(0.0, |ts| ts.Duration as f64 / 10_000_000.0);
                    let dur_secs = tl
                        .as_ref()
                        .and_then(|t| t.EndTime().ok())
                        .map_or(0.0, |ts| ts.Duration as f64 / 10_000_000.0);
                    if playing
                        && let Some(ref t) = tl
                        && let Ok(updated) = t.LastUpdatedTime()
                        && updated.UniversalTime > 0
                    {
                        // SAFETY: `GetSystemTimeAsFileTime` returns current UTC FILETIME.
                        let now_ticks = unsafe {
                            let ft = GetSystemTimeAsFileTime();
                            ((i64::from(ft.dwHighDateTime)) << 32) | i64::from(ft.dwLowDateTime)
                        };
                        if now_ticks > updated.UniversalTime {
                            let elapsed = (now_ticks - updated.UniversalTime) as f64 / 10_000_000.0;
                            if elapsed < 86_400.0 {
                                pos_secs += elapsed * rate;
                            }
                        }
                    }

                    Some(MediaState {
                        title,
                        artist,
                        source_app: id,
                        is_playing: playing,
                        position_secs: pos_secs,
                        duration_secs: dur_secs,
                        position_updated_at: Instant::now(),
                        playback_rate: rate,
                        thumbnail_bytes: cached_thumb.clone(),
                        thumbnail_id: thumb_seq,
                    })
                });

                if let Ok(mut snap) = snapshot.lock() {
                    snap.current = new_state;
                    snap.next_source = next_source;
                }
                on_change();
            }
        }
    }
}

fn list_sessions(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
) -> Vec<GlobalSystemMediaTransportControlsSession> {
    let Ok(view) = manager.GetSessions() else {
        return Vec::new();
    };
    let count = view.Size().unwrap_or(0);
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        if let Ok(s) = view.GetAt(i) {
            out.push(s);
        }
    }
    out
}
