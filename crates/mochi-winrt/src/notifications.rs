use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::Management::{
    UserNotificationListener, UserNotificationListenerAccessStatus,
};
use windows::UI::Notifications::{
    KnownNotificationBindings, NotificationKinds, UserNotification,
    UserNotificationChangedEventArgs,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAccess {
    Allowed,
    Denied,
    Unspecified,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct SystemNotification {
    pub win_id: u32,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub enum NotificationEvent {
    Access(NotificationAccess),
    Snapshot(Vec<SystemNotification>),
    Added(SystemNotification),
    Removed(u32),
}

fn winrt_datetime_to_local(universal_time: i64) -> NaiveDateTime {
    const TICKS_PER_SEC: i64 = 10_000_000;
    const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;
    let unix_ticks = universal_time.saturating_sub(EPOCH_DIFF_TICKS);
    let secs = unix_ticks.div_euclid(TICKS_PER_SEC);
    let nanos = (unix_ticks.rem_euclid(TICKS_PER_SEC) * 100) as u32;
    DateTime::<Utc>::from_timestamp(secs, nanos)
        .map(|dt| dt.with_timezone(&Local).naive_local())
        .unwrap_or_else(|| Local::now().naive_local())
}

fn extract_notification(un: &UserNotification) -> Option<SystemNotification> {
    let win_id = un.Id().ok()?;
    let app_name = un
        .AppInfo()
        .ok()
        .and_then(|info| info.DisplayInfo().ok())
        .and_then(|disp| disp.DisplayName().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Windows".to_string());

    let mut lines: Vec<String> = Vec::new();
    if let Ok(notif) = un.Notification()
        && let Ok(visual) = notif.Visual()
        && let Ok(toast_key) = KnownNotificationBindings::ToastGeneric()
        && let Ok(binding) = visual.GetBinding(&toast_key)
        && let Ok(elems) = binding.GetTextElements()
    {
        let count = elems.Size().unwrap_or(0);
        for i in 0..count {
            if let Ok(el) = elems.GetAt(i)
                && let Ok(txt) = el.Text()
            {
                let s = txt.to_string().trim().to_string();
                if !s.is_empty() {
                    lines.push(s);
                }
            }
        }
    }

    let title = lines.first().cloned().unwrap_or_else(|| app_name.clone());
    let body = if lines.len() > 1 {
        lines[1..].join(" · ")
    } else {
        String::new()
    };
    let timestamp = un
        .CreationTime()
        .ok()
        .map(|dt| winrt_datetime_to_local(dt.UniversalTime))
        .unwrap_or_else(|| Local::now().naive_local());

    Some(SystemNotification {
        win_id,
        app_name,
        title,
        body,
        timestamp,
    })
}

fn fetch_all_toasts(listener: &UserNotificationListener) -> Option<Vec<SystemNotification>> {
    let op = listener
        .GetNotificationsAsync(NotificationKinds::Toast)
        .ok()?;
    let view = op.join().ok()?;
    let count = view.Size().unwrap_or(0);
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        if let Ok(un) = view.GetAt(i)
            && let Some(item) = extract_notification(&un)
        {
            out.push(item);
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
    Some(out)
}

fn map_access(status: UserNotificationListenerAccessStatus) -> NotificationAccess {
    match status {
        UserNotificationListenerAccessStatus::Allowed => NotificationAccess::Allowed,
        UserNotificationListenerAccessStatus::Denied => NotificationAccess::Denied,
        UserNotificationListenerAccessStatus::Unspecified => NotificationAccess::Unspecified,
        _ => NotificationAccess::Unavailable,
    }
}

pub fn start_notification_watcher(
    stop: Arc<AtomicBool>,
    on_event: Arc<dyn Fn(NotificationEvent) + Send + Sync>,
) {
    let _ = std::thread::Builder::new()
        .name("mochi-notifications".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            // SAFETY: Initializing MTA COM on background WinRT listener thread.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let Ok(listener) = UserNotificationListener::Current() else {
                on_event(NotificationEvent::Access(NotificationAccess::Unavailable));
                return;
            };

            let mut access = listener
                .GetAccessStatus()
                .map(map_access)
                .unwrap_or(NotificationAccess::Unavailable);
            if access == NotificationAccess::Unspecified
                && let Ok(op) = listener.RequestAccessAsync()
                && let Ok(res) = op.join()
            {
                access = map_access(res);
            }
            on_event(NotificationEvent::Access(access));
            if access != NotificationAccess::Allowed {
                return;
            }

            let mut known_ids: HashSet<u32> = HashSet::new();
            if let Some(initial) = fetch_all_toasts(&listener) {
                for item in &initial {
                    known_ids.insert(item.win_id);
                }
                on_event(NotificationEvent::Snapshot(initial));
            }

            let wake_pair = Arc::new((Mutex::new(false), Condvar::new()));
            let wake_cb = Arc::clone(&wake_pair);
            let token = listener
                .NotificationChanged(&TypedEventHandler::<
                    UserNotificationListener,
                    UserNotificationChangedEventArgs,
                >::new(move |_, _| {
                    let (lock, cv) = &*wake_cb;
                    if let Ok(mut dirty) = lock.lock() {
                        *dirty = true;
                        cv.notify_one();
                    }
                    Ok(())
                }))
                .ok();

            let mut last_sync = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let mut woke_by_event = false;
                let (lock, cv) = &*wake_pair;
                if let Ok(guard) = lock.lock() {
                    let (mut g, _) = cv
                        .wait_timeout_while(guard, Duration::from_secs(5), |d| {
                            !*d && !stop.load(Ordering::Relaxed)
                        })
                        .unwrap_or_else(|e| e.into_inner());
                    woke_by_event = *g;
                    *g = false;
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if !woke_by_event
                    && (known_ids.is_empty() || last_sync.elapsed() < Duration::from_secs(30))
                {
                    continue;
                }
                last_sync = Instant::now();
                if let Some(current) = fetch_all_toasts(&listener) {
                    let current_ids: HashSet<u32> = current.iter().map(|n| n.win_id).collect();
                    for item in current.into_iter().rev() {
                        if known_ids.insert(item.win_id) {
                            on_event(NotificationEvent::Added(item));
                        }
                    }
                    let removed: Vec<u32> = known_ids
                        .iter()
                        .copied()
                        .filter(|id| !current_ids.contains(id))
                        .collect();
                    for id in removed {
                        known_ids.remove(&id);
                        on_event(NotificationEvent::Removed(id));
                    }
                }
            }

            if let Some(tok) = token {
                let _ = listener.RemoveNotificationChanged(tok);
            }
        });
}

pub fn remove_windows_notification(win_id: u32) {
    std::thread::spawn(move || {
        // SAFETY: Initializing MTA COM on short-lived worker thread.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        if let Ok(listener) = UserNotificationListener::Current() {
            let _ = listener.RemoveNotification(win_id);
        }
    });
}

pub fn clear_windows_notifications() {
    std::thread::spawn(|| {
        // SAFETY: Initializing MTA COM on short-lived worker thread.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        if let Ok(listener) = UserNotificationListener::Current() {
            let _ = listener.ClearNotifications();
        }
    });
}
