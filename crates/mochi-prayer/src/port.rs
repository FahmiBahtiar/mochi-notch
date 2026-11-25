//! Pure astronomical prayer time calculation and Hijri date formatting.

#![forbid(unsafe_code)]

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrayerMethod {
    Kemenag,
    Mwl,
    Isna,
    UmmAlQura,
    Egyptian,
}

impl PrayerMethod {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "mwl" => Self::Mwl,
            "isna" => Self::Isna,
            "ummalqura" | "umm al-qura" | "makkah" => Self::UmmAlQura,
            "egyptian" | "egypt" => Self::Egyptian,
            _ => Self::Kemenag,
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Self::Kemenag => "Kemenag",
            Self::Mwl => "MWL",
            Self::Isna => "ISNA",
            Self::UmmAlQura => "UmmAlQura",
            Self::Egyptian => "Egyptian",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Kemenag => "Kemenag RI",
            Self::Mwl => "MWL",
            Self::Isna => "ISNA",
            Self::UmmAlQura => "Umm al-Qura",
            Self::Egyptian => "Egyptian",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Kemenag => Self::Mwl,
            Self::Mwl => Self::Isna,
            Self::Isna => Self::UmmAlQura,
            Self::UmmAlQura => Self::Egyptian,
            Self::Egyptian => Self::Kemenag,
        }
    }

    fn angles(self) -> (f64, Option<f64>) {
        match self {
            Self::Kemenag => (20.0, Some(18.0)),
            Self::Mwl => (18.0, Some(17.0)),
            Self::Isna => (15.0, Some(15.0)),
            Self::UmmAlQura => (18.5, None),
            Self::Egyptian => (19.5, Some(17.5)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrayerKind {
    Fajr = 0,
    Dhuhr = 1,
    Asr = 2,
    Maghrib = 3,
    Isha = 4,
}

impl PrayerKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Fajr => "Subuh",
            Self::Dhuhr => "Dzuhur",
            Self::Asr => "Ashar",
            Self::Maghrib => "Maghrib",
            Self::Isha => "Isya",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrayerEntry {
    pub kind: PrayerKind,
    pub name: String,
    pub time: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct PrayerSchedule {
    pub date: NaiveDate,
    pub hijri: String,
    pub sunrise: NaiveDateTime,
    pub prayers: [PrayerEntry; 5],
    pub next_idx: usize,
    pub next_time: NaiveDateTime,
    pub prev_time: NaiveDateTime,
}

fn julian_day(date: NaiveDate) -> f64 {
    let mut y = f64::from(date.year());
    let mut m = f64::from(date.month());
    let d = f64::from(date.day());
    if m <= 2.0 {
        y -= 1.0;
        m += 12.0;
    }
    let a = (y / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + d + b - 1524.5
}

fn sun_position(jd: f64) -> (f64, f64) {
    let d = jd - 2451545.0;
    let g = (357.529 + 0.98560028 * d).rem_euclid(360.0).to_radians();
    let q = (280.459 + 0.98564736 * d).rem_euclid(360.0);
    let l = (q + 1.915 * g.sin() + 0.020 * (2.0 * g).sin())
        .rem_euclid(360.0)
        .to_radians();
    let e = (23.439 - 0.00000036 * d).to_radians();
    let ra = (e.cos() * l.sin()).atan2(l.cos()).to_degrees() / 15.0;
    let ra = ra.rem_euclid(24.0);
    let decl = (e.sin() * l.sin()).asin();
    let eqt = q / 15.0 - ra;
    (decl, eqt)
}

fn hour_angle_hours(lat_rad: f64, decl: f64, angle_deg: f64) -> f64 {
    let num = -angle_deg.to_radians().sin() - lat_rad.sin() * decl.sin();
    let den = lat_rad.cos() * decl.cos();
    let cos_t = (num / den).clamp(-1.0, 1.0);
    cos_t.acos().to_degrees() / 15.0
}

fn hours_to_dt(date: NaiveDate, hours: f64) -> NaiveDateTime {
    let total_secs = (hours * 3600.0).round() as i64;
    let base = date.and_hms_opt(0, 0, 0).unwrap();
    base + Duration::seconds(total_secs)
}

fn compute_day_times(
    date: NaiveDate,
    lat: f64,
    lon: f64,
    tz_hours: f64,
    method: PrayerMethod,
) -> (NaiveDateTime, [NaiveDateTime; 5]) {
    let jd = julian_day(date) - lon / 360.0;
    let lat_rad = lat.to_radians();
    let (fajr_ang, isha_ang) = method.angles();

    let (_decl_noon, eqt_noon) = sun_position(jd + 0.5);
    let dhuhr_h = 12.0 + tz_hours - lon / 15.0 - eqt_noon;

    let (decl_fajr, eqt_fajr) = sun_position(jd + 5.0 / 24.0);
    let fajr_h =
        (12.0 + tz_hours - lon / 15.0 - eqt_fajr) - hour_angle_hours(lat_rad, decl_fajr, fajr_ang);

    let (decl_sun, eqt_sun) = sun_position(jd + 6.0 / 24.0);
    let sunrise_h =
        (12.0 + tz_hours - lon / 15.0 - eqt_sun) - hour_angle_hours(lat_rad, decl_sun, 0.833);

    let (decl_asr, eqt_asr) = sun_position(jd + 15.0 / 24.0);
    let asr_alt = (1.0 / (1.0 + (lat_rad - decl_asr).abs().tan())).atan();
    let asr_h = (12.0 + tz_hours - lon / 15.0 - eqt_asr)
        + hour_angle_hours(lat_rad, decl_asr, -asr_alt.to_degrees());

    let (decl_mag, eqt_mag) = sun_position(jd + 18.0 / 24.0);
    let maghrib_h =
        (12.0 + tz_hours - lon / 15.0 - eqt_mag) + hour_angle_hours(lat_rad, decl_mag, 0.833);

    let isha_h = match isha_ang {
        Some(ang) => {
            let (decl_isha, eqt_isha) = sun_position(jd + 19.5 / 24.0);
            (12.0 + tz_hours - lon / 15.0 - eqt_isha) + hour_angle_hours(lat_rad, decl_isha, ang)
        }
        None => maghrib_h + 1.5,
    };

    (
        hours_to_dt(date, sunrise_h),
        [
            hours_to_dt(date, fajr_h),
            hours_to_dt(date, dhuhr_h),
            hours_to_dt(date, asr_h),
            hours_to_dt(date, maghrib_h),
            hours_to_dt(date, isha_h),
        ],
    )
}

fn hijri_date_string(date: NaiveDate) -> String {
    let jd = (julian_day(date) + 0.5).floor() as i64;
    let l = jd - 1948440 + 10632;
    let n = (l - 1) / 10631;
    let l = l - 10631 * n + 354;
    let j = ((10985 - l) / 5316) * ((50 * l) / 17719) + (l / 5670) * ((43 * l) / 15238);
    let l = l - ((30 - j) / 15) * ((17719 * j) / 50) - (j / 16) * ((15238 * j) / 43) + 29;
    let month = (24 * l) / 709;
    let day = l - (709 * month) / 24;
    let year = 30 * n + j - 30;
    const MONTHS: [&str; 12] = [
        "Muharram",
        "Safar",
        "Rabiul Awal",
        "Rabiul Akhir",
        "Jumadil Awal",
        "Jumadil Akhir",
        "Rajab",
        "Syaban",
        "Ramadhan",
        "Syawal",
        "Dzulqaidah",
        "Dzulhijjah",
    ];
    let m_name = MONTHS
        .get((month - 1).clamp(0, 11) as usize)
        .copied()
        .unwrap_or("Muharram");
    format!("{day} {m_name} {year} H")
}

pub fn build_prayer_schedule(
    now: NaiveDateTime,
    lat: f64,
    lon: f64,
    method: PrayerMethod,
) -> PrayerSchedule {
    let tz_hours = f64::from(Local::now().offset().local_minus_utc()) / 3600.0;
    let today = now.date();
    let (sunrise, times) = compute_day_times(today, lat, lon, tz_hours, method);
    let kinds = [
        PrayerKind::Fajr,
        PrayerKind::Dhuhr,
        PrayerKind::Asr,
        PrayerKind::Maghrib,
        PrayerKind::Isha,
    ];
    let prayers: [PrayerEntry; 5] = std::array::from_fn(|i| PrayerEntry {
        kind: kinds[i],
        name: kinds[i].key().to_string(),
        time: times[i],
    });

    let (next_idx, next_time, prev_time) =
        if let Some(idx) = prayers.iter().position(|p| p.time > now) {
            let prev = if idx == 0 {
                let yesterday = today - Duration::days(1);
                let (_, y_times) = compute_day_times(yesterday, lat, lon, tz_hours, method);
                y_times[4]
            } else {
                prayers[idx - 1].time
            };
            (idx, prayers[idx].time, prev)
        } else {
            let tomorrow = today + Duration::days(1);
            let (_, t_times) = compute_day_times(tomorrow, lat, lon, tz_hours, method);
            (0, t_times[0], prayers[4].time)
        };

    PrayerSchedule {
        date: today,
        hijri: hijri_date_string(today),
        sunrise,
        prayers,
        next_idx,
        next_time,
        prev_time,
    }
}

pub fn format_countdown(now: NaiveDateTime, target: NaiveDateTime) -> String {
    let secs = (target - now).num_seconds().max(0);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn format_hm(dt: NaiveDateTime) -> String {
    format!("{:02}:{:02}", dt.hour(), dt.minute())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jakarta_prayer_times_are_ordered_and_realistic() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let (sunrise, times) =
            compute_day_times(date, -6.2088, 106.8456, 7.0, PrayerMethod::Kemenag);
        assert!(times[0] < sunrise);
        assert!(sunrise < times[1]);
        assert!(times[1] < times[2]);
        assert!(times[2] < times[3]);
        assert!(times[3] < times[4]);
        assert!((3..=5).contains(&times[0].hour()));
        assert!((11..=12).contains(&times[1].hour()));
        assert!((14..=16).contains(&times[2].hour()));
        assert!((17..=18).contains(&times[3].hour()));
        assert!((18..=20).contains(&times[4].hour()));
    }

    #[test]
    fn hijri_date_formats_cleanly() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let h = hijri_date_string(date);
        assert!(h.ends_with(" H"));
        assert!(h.contains("1448"));
    }
}
