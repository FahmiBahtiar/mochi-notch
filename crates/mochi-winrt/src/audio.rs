use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, PROPERTYKEY};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, AudioSessionDisconnectReason, AudioSessionStateActive,
    DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IAudioSessionControl,
    IAudioSessionControl2, IAudioSessionEvents, IAudioSessionEvents_Impl, IAudioSessionManager2,
    IAudioSessionNotification, IAudioSessionNotification_Impl, IMMDevice, IMMDeviceEnumerator,
    IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eCapture,
    eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_core::{GUID, Interface, PCWSTR, Ref, implement};

use crate::native::to_wide;
use crate::paths::{Glyphs, HudKind, HudMessage};
use crate::strings::Language;

// PKEY_AudioEndpoint_FormFactor {1DA5D803-D492-4EDD-8C23-E0C0FFEE7F0E}, pid 0
const PKEY_AUDIO_ENDPOINT_FORM_FACTOR: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 0,
};

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Hud(HudMessage),
    MicMuteChanged(bool),
    MicAppsChanged(Vec<String>),
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    on_notify: Arc<dyn Fn(f32, bool) + Send + Sync>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows_core::Result<()> {
        if !pnotify.is_null() {
            // SAFETY: Core Audio passes a valid `AUDIO_VOLUME_NOTIFICATION_DATA` pointer during `OnNotify`.
            let data = unsafe { &*pnotify };
            (self.on_notify)(data.fMasterVolume, data.bMuted.as_bool());
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceNotifyClient {
    on_default_changed: Arc<dyn Fn(EDataFlow, ERole) + Send + Sync>,
    on_topology_changed: Arc<dyn Fn() + Send + Sync>,
}

impl IMMNotificationClient_Impl for DeviceNotifyClient_Impl {
    fn OnDeviceStateChanged(
        &self,
        _pwstrdeviceid: &PCWSTR,
        _dwnewstate: DEVICE_STATE,
    ) -> windows_core::Result<()> {
        (self.on_topology_changed)();
        Ok(())
    }
    fn OnDeviceAdded(&self, _pwstrdeviceid: &PCWSTR) -> windows_core::Result<()> {
        (self.on_topology_changed)();
        Ok(())
    }
    fn OnDeviceRemoved(&self, _pwstrdeviceid: &PCWSTR) -> windows_core::Result<()> {
        (self.on_topology_changed)();
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _pwstrdefaultdeviceid: &PCWSTR,
    ) -> windows_core::Result<()> {
        (self.on_default_changed)(flow, role);
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        _pwstrdeviceid: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows_core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioSessionNotification)]
struct SessionCreatedNotify {
    on_change: Arc<dyn Fn() + Send + Sync>,
}

impl IAudioSessionNotification_Impl for SessionCreatedNotify_Impl {
    fn OnSessionCreated(
        &self,
        _newsession: Ref<'_, IAudioSessionControl>,
    ) -> windows_core::Result<()> {
        (self.on_change)();
        Ok(())
    }
}

#[implement(IAudioSessionEvents)]
struct SessionEventsNotify {
    on_change: Arc<dyn Fn() + Send + Sync>,
}

impl IAudioSessionEvents_Impl for SessionEventsNotify_Impl {
    fn OnDisplayNameChanged(
        &self,
        _newdisplayname: &PCWSTR,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnIconPathChanged(
        &self,
        _newiconpath: &PCWSTR,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnSimpleVolumeChanged(
        &self,
        _newvolume: f32,
        _newmute: windows_core::BOOL,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnChannelVolumeChanged(
        &self,
        _channelcount: u32,
        _newchannelvolumearray: *const f32,
        _changedchannel: u32,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnGroupingParamChanged(
        &self,
        _newgroupingparam: *const GUID,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnStateChanged(
        &self,
        _newstate: windows::Win32::Media::Audio::AudioSessionState,
    ) -> windows_core::Result<()> {
        (self.on_change)();
        Ok(())
    }
    fn OnSessionDisconnected(
        &self,
        _disconnectreason: AudioSessionDisconnectReason,
    ) -> windows_core::Result<()> {
        (self.on_change)();
        Ok(())
    }
}

enum AudioCmd {
    RebindRender(bool),
    RebindMicMute,
    ToggleMicMute,
    RefreshMicApps,
    RebindMicDevices,
}

pub struct AudioService {
    cmd_tx: std::sync::mpsc::Sender<AudioCmd>,
    mic_muted: Arc<AtomicBool>,
    mic_apps: Arc<Mutex<Vec<String>>>,
}

impl AudioService {
    pub fn start(lang: Language, on_event: Arc<dyn Fn(AudioEvent) + Send + Sync>) -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<AudioCmd>();
        let mic_muted = Arc::new(AtomicBool::new(false));
        let mic_apps = Arc::new(Mutex::new(Vec::new()));

        let tx_bg = cmd_tx.clone();
        let muted_bg = Arc::clone(&mic_muted);
        let apps_bg = Arc::clone(&mic_apps);
        std::thread::Builder::new()
            .name("mochi-audio".to_string())
            .stack_size(256 * 1024)
            .spawn(move || {
                // SAFETY: Initializes COM MTA on this dedicated Core Audio worker thread.
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                run_audio_loop(lang, tx_bg, cmd_rx, muted_bg, apps_bg, on_event);
            })
            .ok();

        Self {
            cmd_tx,
            mic_muted,
            mic_apps,
        }
    }

    pub fn is_mic_muted(&self) -> bool {
        self.mic_muted.load(Ordering::Relaxed)
    }

    pub fn mic_apps(&self) -> Vec<String> {
        self.mic_apps.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn toggle_mic_mute(&self) {
        let _ = self.cmd_tx.send(AudioCmd::ToggleMicMute);
    }

    pub fn refresh_mic_apps(&self) {
        let _ = self.cmd_tx.send(AudioCmd::RefreshMicApps);
    }
}

fn run_audio_loop(
    lang: Language,
    cmd_tx: std::sync::mpsc::Sender<AudioCmd>,
    cmd_rx: std::sync::mpsc::Receiver<AudioCmd>,
    mic_muted: Arc<AtomicBool>,
    mic_apps: Arc<Mutex<Vec<String>>>,
    on_event: Arc<dyn Fn(AudioEvent) + Send + Sync>,
) {
    // SAFETY: CoCreateInstance creates the standard Windows MMDeviceEnumerator COM object.
    let Ok(enumerator): windows_core::Result<IMMDeviceEnumerator> =
        (unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) })
    else {
        return;
    };

    let tx_dev = cmd_tx.clone();
    let tx_top = cmd_tx.clone();
    let dev_client: IMMNotificationClient = DeviceNotifyClient {
        on_default_changed: Arc::new(move |flow, role| {
            if flow == eRender && role == eMultimedia {
                let _ = tx_dev.send(AudioCmd::RebindRender(true));
            } else if flow == eCapture && role == eCommunications {
                let _ = tx_dev.send(AudioCmd::RebindMicMute);
            }
        }),
        on_topology_changed: Arc::new(move || {
            let _ = tx_top.send(AudioCmd::RebindMicDevices);
        }),
    }
    .into();
    // SAFETY: `dev_client` is a valid COM object implementing `IMMNotificationClient`.
    let _ = unsafe { enumerator.RegisterEndpointNotificationCallback(&dev_client) };

    let mut render_vol: Option<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)> = None;
    let mut render_id: Option<String> = None;
    let mut mic_vol: Option<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)> = None;
    let mut managers: Vec<(IAudioSessionManager2, IAudioSessionNotification)> = Vec::new();
    let mut tracked_sessions: HashMap<String, (IAudioSessionControl2, IAudioSessionEvents)> =
        HashMap::new();

    let _ = cmd_tx.send(AudioCmd::RebindRender(false));
    let _ = cmd_tx.send(AudioCmd::RebindMicMute);
    let _ = cmd_tx.send(AudioCmd::RebindMicDevices);

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            AudioCmd::RebindRender(announce) => {
                if let Some((v, cb)) = render_vol.take() {
                    // SAFETY: Unregistering previously registered callback.
                    unsafe {
                        let _ = v.UnregisterControlChangeNotify(&cb);
                    }
                }
                // SAFETY: Querying default render endpoint.
                if let Ok(dev) = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }
                {
                    let id = device_id(&dev);
                    // SAFETY: Activating IAudioEndpointVolume on the render device.
                    if let Ok(vol) =
                        unsafe { dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
                    {
                        let ev_cb = Arc::clone(&on_event);
                        let cb: IAudioEndpointVolumeCallback = VolumeCallback {
                            on_notify: Arc::new(move |level, muted| {
                                ev_cb(AudioEvent::Hud(volume_message(level, muted, lang)));
                            }),
                        }
                        .into();
                        // SAFETY: Registering callback on `vol`.
                        if unsafe { vol.RegisterControlChangeNotify(&cb) }.is_ok() {
                            render_vol = Some((vol, cb));
                        }
                    }
                    let changed = render_id.is_some() && render_id != id;
                    render_id = id;
                    if announce && changed {
                        let (name, headphones) = describe_device(&dev, lang);
                        let mut msg = HudMessage::new(
                            if headphones {
                                Glyphs::HEADPHONES
                            } else {
                                Glyphs::SPEAKERS
                            },
                            name,
                        );
                        msg.kind = HudKind::AudioDevice;
                        on_event(AudioEvent::Hud(msg));
                    }
                }
            }
            AudioCmd::RebindMicMute => {
                if let Some((v, cb)) = mic_vol.take() {
                    // SAFETY: Unregistering previously registered callback.
                    unsafe {
                        let _ = v.UnregisterControlChangeNotify(&cb);
                    }
                }
                // SAFETY: Querying default capture communications endpoint.
                if let Ok(dev) =
                    unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) }
                {
                    // SAFETY: Activating IAudioEndpointVolume on the capture device.
                    if let Ok(vol) =
                        unsafe { dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
                    {
                        // SAFETY: Reading current mute state.
                        let muted = unsafe { vol.GetMute() }
                            .map(|b| b.as_bool())
                            .unwrap_or(false);
                        mic_muted.store(muted, Ordering::Relaxed);
                        let muted_ref = Arc::clone(&mic_muted);
                        let ev_cb = Arc::clone(&on_event);
                        let cb: IAudioEndpointVolumeCallback = VolumeCallback {
                            on_notify: Arc::new(move |_level, m| {
                                let prev = muted_ref.swap(m, Ordering::Relaxed);
                                if prev != m {
                                    ev_cb(AudioEvent::MicMuteChanged(m));
                                }
                            }),
                        }
                        .into();
                        // SAFETY: Registering callback on `vol`.
                        if unsafe { vol.RegisterControlChangeNotify(&cb) }.is_ok() {
                            mic_vol = Some((vol, cb));
                        }
                    }
                } else {
                    mic_muted.store(false, Ordering::Relaxed);
                }
            }
            AudioCmd::ToggleMicMute => {
                let target_mute = !mic_muted.load(Ordering::Relaxed);
                for role in [eCommunications, eConsole] {
                    // SAFETY: Querying capture endpoint for role.
                    if let Ok(dev) = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, role) }
                        && let Ok(vol) =
                            unsafe { dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
                    {
                        // SAFETY: Setting mute state with empty event context GUID.
                        unsafe {
                            let _ = vol.SetMute(target_mute, &GUID::zeroed());
                        }
                    }
                }
            }
            AudioCmd::RebindMicDevices => {
                for (ctrl, ev) in tracked_sessions.drain().map(|(_, v)| v) {
                    // SAFETY: Unregistering session events.
                    unsafe {
                        let _ = ctrl.UnregisterAudioSessionNotification(&ev);
                    }
                }
                for (mgr, notif) in managers.drain(..) {
                    // SAFETY: Unregistering session notification.
                    unsafe {
                        let _ = mgr.UnregisterSessionNotification(&notif);
                    }
                }
                // SAFETY: Enumerating active capture endpoints.
                if let Ok(col) =
                    unsafe { enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) }
                {
                    // SAFETY: Reading device count.
                    let count = unsafe { col.GetCount() }.unwrap_or(0);
                    for i in 0..count {
                        // SAFETY: Accessing device `i` and activating `IAudioSessionManager2`.
                        if let Ok(dev) = unsafe { col.Item(i) }
                            && let Ok(mgr) =
                                unsafe { dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }
                        {
                            // SAFETY: Enumerating once activates session notifications.
                            let _ = unsafe { mgr.GetSessionEnumerator() };
                            let tx_s = cmd_tx.clone();
                            let notif: IAudioSessionNotification = SessionCreatedNotify {
                                on_change: Arc::new(move || {
                                    let _ = tx_s.send(AudioCmd::RefreshMicApps);
                                }),
                            }
                            .into();
                            // SAFETY: Registering session notification.
                            if unsafe { mgr.RegisterSessionNotification(&notif) }.is_ok() {
                                managers.push((mgr, notif));
                            }
                        }
                    }
                }
                let _ = cmd_tx.send(AudioCmd::RefreshMicApps);
            }
            AudioCmd::RefreshMicApps => {
                let mut seen = std::collections::HashSet::new();
                let mut apps = BTreeSet::new();
                // SAFETY: GetCurrentProcessId has no preconditions.
                let self_pid = unsafe { GetCurrentProcessId() };

                for (mgr, _) in &managers {
                    // SAFETY: Enumerating audio sessions on capture manager.
                    let Ok(enum_s) = (unsafe { mgr.GetSessionEnumerator() }) else {
                        continue;
                    };
                    // SAFETY: Reading session count.
                    let count = unsafe { enum_s.GetCount() }.unwrap_or(0);
                    for i in 0..count {
                        // SAFETY: Getting session `i` and casting to `IAudioSessionControl2`.
                        let Ok(ctrl1) = (unsafe { enum_s.GetSession(i) }) else {
                            continue;
                        };
                        let Ok(ctrl2) = ctrl1.cast::<IAudioSessionControl2>() else {
                            continue;
                        };
                        // SAFETY: Reading session instance identifier.
                        let Ok(id_pw) = (unsafe { ctrl2.GetSessionInstanceIdentifier() }) else {
                            continue;
                        };
                        // SAFETY: `id_pw` is a COM-allocated PWSTR.
                        let id = unsafe { id_pw.to_string() }.unwrap_or_default();
                        // SAFETY: `id_pw` was allocated by COM task allocator.
                        unsafe { CoTaskMemFree(Some(id_pw.0.cast())) };
                        if id.is_empty() {
                            continue;
                        }
                        seen.insert(id.clone());
                        if !tracked_sessions.contains_key(&id) {
                            let tx_e = cmd_tx.clone();
                            let ev: IAudioSessionEvents = SessionEventsNotify {
                                on_change: Arc::new(move || {
                                    let _ = tx_e.send(AudioCmd::RefreshMicApps);
                                }),
                            }
                            .into();
                            // SAFETY: Registering session events.
                            if unsafe { ctrl2.RegisterAudioSessionNotification(&ev) }.is_ok() {
                                tracked_sessions.insert(id.clone(), (ctrl2.clone(), ev));
                            }
                        }
                        // SAFETY: Reading session state and process ID.
                        let state = unsafe { ctrl2.GetState() };
                        // SAFETY: Reading session process ID.
                        let pid = unsafe { ctrl2.GetProcessId() }.unwrap_or(0);
                        if state == Ok(AudioSessionStateActive) && pid != 0 && pid != self_pid {
                            apps.insert(process_display_name(pid));
                        }
                    }
                }

                tracked_sessions.retain(|id, (ctrl, ev)| {
                    if seen.contains(id) {
                        true
                    } else {
                        // SAFETY: Unregistering stale session events.
                        unsafe {
                            let _ = ctrl.UnregisterAudioSessionNotification(&*ev);
                        }
                        false
                    }
                });

                let next_list: Vec<String> = apps.into_iter().collect();
                let changed = if let Ok(mut guard) = mic_apps.lock() {
                    if *guard != next_list {
                        *guard = next_list.clone();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if changed {
                    on_event(AudioEvent::MicAppsChanged(next_list));
                }
            }
        }
    }
}

fn device_id(dev: &IMMDevice) -> Option<String> {
    // SAFETY: `dev` is a valid IMMDevice.
    let pw = unsafe { dev.GetId() }.ok()?;
    // SAFETY: `pw` is a valid COM-allocated PWSTR.
    let s = unsafe { pw.to_string() }.ok();
    // SAFETY: `pw` was allocated by COM task allocator.
    unsafe { CoTaskMemFree(Some(pw.0.cast())) };
    s
}

fn describe_device(dev: &IMMDevice, lang: Language) -> (String, bool) {
    let mut name = lang.strings().hud_audio_output.to_string();
    let mut headphones = false;
    // SAFETY: Opening read-only property store on `dev`.
    if let Ok(store) = unsafe { dev.OpenPropertyStore(STGM_READ) } {
        // SAFETY: Reading PKEY_Device_FriendlyName.
        if let Ok(mut pv) = unsafe { store.GetValue(&PKEY_Device_FriendlyName) } {
            let s = pv.to_string();
            if !s.is_empty() {
                name = s;
            }
            // SAFETY: Clearing PROPVARIANT.
            unsafe {
                let _ = PropVariantClear(&raw mut pv);
            }
        }
        // SAFETY: Reading PKEY_AudioEndpoint_FormFactor.
        if let Ok(mut pv) = unsafe { store.GetValue(&PKEY_AUDIO_ENDPOINT_FORM_FACTOR) } {
            if let Ok(ff) = u32::try_from(&pv) {
                headphones = ff == 3 || ff == 5;
            }
            // SAFETY: Clearing PROPVARIANT.
            unsafe {
                let _ = PropVariantClear(&raw mut pv);
            }
        }
    }
    (name, headphones)
}

fn volume_message(level: f32, muted: bool, lang: Language) -> HudMessage {
    let s = lang.strings();
    let percent = (level * 100.0).round() as i32;
    let glyph = if muted || percent == 0 {
        Glyphs::MUTE
    } else if percent < 34 {
        Glyphs::VOLUME_1
    } else if percent < 67 {
        Glyphs::VOLUME_2
    } else {
        Glyphs::VOLUME_3
    };
    let mut msg = if muted {
        let mut m = HudMessage::new(glyph, s.hud_muted);
        m.level = Some(0.0);
        m
    } else {
        let mut m = HudMessage::new(glyph, s.hud_volume);
        m.value = Some(percent.to_string());
        m.level = Some(f64::from(level));
        m
    };
    msg.kind = HudKind::Volume;
    msg
}

pub fn process_display_name(pid: u32) -> String {
    // SAFETY: OpenProcess returns Result; handle is closed before returning.
    let Ok(proc) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return format!("PID {pid}");
    };
    let mut buf = vec![0u16; 1024];
    let mut size = buf.len() as u32;
    // SAFETY: `proc` is valid and `buf` has `size` UTF-16 code units.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_WIN32,
            windows_core::PWSTR(buf.as_mut_ptr()),
            &raw mut size,
        )
    };
    // SAFETY: `proc` is valid.
    unsafe {
        let _ = CloseHandle(proc);
    }
    if ok.is_err() {
        return format!("PID {pid}");
    }
    let path = String::from_utf16_lossy(&buf[..size as usize]);
    if let Some(desc) = file_description(&path) {
        return desc;
    }
    Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("PID {pid}"))
}

pub fn file_description(path: &str) -> Option<String> {
    let wide = to_wide(path);
    let mut dummy = 0u32;
    // SAFETY: `wide` is NUL-terminated UTF-16.
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), Some(&raw mut dummy)) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` has `size` bytes.
    if unsafe {
        GetFileVersionInfoW(
            PCWSTR(wide.as_ptr()),
            Some(0),
            size,
            data.as_mut_ptr().cast(),
        )
    }
    .is_err()
    {
        return None;
    }
    let mut trans_ptr = std::ptr::null_mut();
    let mut trans_len = 0u32;
    let trans_q = to_wide("\\VarFileInfo\\Translation");
    // SAFETY: `data` holds valid version info.
    let has_trans = unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(trans_q.as_ptr()),
            &raw mut trans_ptr,
            &raw mut trans_len,
        )
    };
    let (lang, cp) = if has_trans.as_bool() && trans_len >= 4 && !trans_ptr.is_null() {
        // SAFETY: `trans_ptr` points to at least 2 u16 values inside `data`.
        let pair = unsafe { std::slice::from_raw_parts(trans_ptr.cast::<u16>(), 2) };
        (pair[0], pair[1])
    } else {
        (0x0409, 0x04B0)
    };
    let sub = to_wide(&format!(
        "\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"
    ));
    let mut val_ptr = std::ptr::null_mut();
    let mut val_len = 0u32;
    // SAFETY: `data` holds valid version info and `sub` is NUL-terminated UTF-16.
    let ok = unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(sub.as_ptr()),
            &raw mut val_ptr,
            &raw mut val_len,
        )
    };
    if ok.as_bool() && val_len > 0 && !val_ptr.is_null() {
        // SAFETY: `val_ptr` points to `val_len` UTF-16 code units inside `data`.
        let slice = unsafe { std::slice::from_raw_parts(val_ptr.cast::<u16>(), val_len as usize) };
        let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        let text = String::from_utf16_lossy(&slice[..end]).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}
