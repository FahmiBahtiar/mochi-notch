use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use windows::Win32::Globalization::GetUserDefaultUILanguage;

include!(concat!(env!("OUT_DIR"), "/strings_gen.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Id,
}

impl Language {
    pub fn detect_windows() -> Self {
        // SAFETY: GetUserDefaultUILanguage takes no pointers and returns a LANGID.
        let lang_id = unsafe { GetUserDefaultUILanguage() };
        let primary = lang_id & 0x03FF;
        if primary == 0x21 { Self::Id } else { Self::En }
    }

    pub fn resolve(choice: Option<&str>) -> Self {
        match choice {
            Some("id") | Some("id-ID") => Self::Id,
            Some("en") => Self::En,
            _ => Self::detect_windows(),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Id => "id",
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Self::En => &EN,
            Self::Id => &ID,
        }
    }

    pub fn weekday_short(self, date: NaiveDate) -> &'static str {
        let idx = date.weekday().num_days_from_sunday() as usize;
        match self {
            Self::En => ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][idx],
            Self::Id => ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"][idx],
        }
    }

    pub fn weekday_initial(self, date: NaiveDate) -> &'static str {
        let idx = date.weekday().num_days_from_sunday() as usize;
        match self {
            Self::En => ["S", "M", "T", "W", "T", "F", "S"][idx],
            Self::Id => ["M", "S", "S", "R", "K", "J", "S"][idx],
        }
    }

    pub fn weekday_long(self, date: NaiveDate) -> &'static str {
        let idx = date.weekday().num_days_from_sunday() as usize;
        match self {
            Self::En => [
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
            ][idx],
            Self::Id => [
                "Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu",
            ][idx],
        }
    }

    pub fn month_long(self, month: u32) -> &'static str {
        let idx = month.saturating_sub(1).min(11) as usize;
        match self {
            Self::En => [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ][idx],
            Self::Id => [
                "Januari",
                "Februari",
                "Maret",
                "April",
                "Mei",
                "Juni",
                "Juli",
                "Agustus",
                "September",
                "Oktober",
                "November",
                "Desember",
            ][idx],
        }
    }

    pub fn month_short(self, month: u32) -> &'static str {
        let idx = month.saturating_sub(1).min(11) as usize;
        match self {
            Self::En => [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ][idx],
            Self::Id => [
                "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
            ][idx],
        }
    }

    pub fn format_long_date(self, date: NaiveDate) -> String {
        match self {
            Self::En => format!(
                "{}, {} {}",
                self.weekday_long(date),
                self.month_long(date.month()),
                date.day()
            ),
            Self::Id => format!(
                "{}, {} {}",
                self.weekday_long(date),
                date.day(),
                self.month_long(date.month())
            ),
        }
    }

    pub fn format_day_month(self, date: NaiveDate) -> String {
        match self {
            Self::En => format!("{:02}/{:02}", date.month(), date.day()),
            Self::Id => format!("{:02}/{:02}", date.day(), date.month()),
        }
    }

    pub fn format_day_month_time(self, dt: NaiveDateTime) -> String {
        format!(
            "{} {:02}:{:02}",
            self.format_day_month(dt.date()),
            dt.hour(),
            dt.minute()
        )
    }

    pub fn format_weekday_date(self, date: NaiveDate) -> String {
        match self {
            Self::En => format!(
                "{}, {} {}",
                self.weekday_short(date),
                self.month_short(date.month()),
                date.day()
            ),
            Self::Id => format!(
                "{}, {} {}",
                self.weekday_short(date),
                date.day(),
                self.month_short(date.month())
            ),
        }
    }

    pub fn format_int(self, n: i64) -> String {
        let sep = match self {
            Self::En => ',',
            Self::Id => '.',
        };
        let neg = n < 0;
        let s = n.abs().to_string();
        let mut out = String::with_capacity(s.len() + 4);
        for (i, ch) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                out.push(sep);
            }
            out.push(ch);
        }
        if neg {
            out.push('-');
        }
        out.chars().rev().collect()
    }

    pub fn format_short_time(self, dt: NaiveDateTime) -> String {
        match self {
            Self::En => {
                let h = dt.hour();
                let (h12, ampm) = match h {
                    0 => (12, "AM"),
                    1..=11 => (h, "AM"),
                    12 => (12, "PM"),
                    _ => (h - 12, "PM"),
                };
                format!("{h12}:{:02} {ampm}", dt.minute())
            }
            Self::Id => format!("{:02}:{:02}", dt.hour(), dt.minute()),
        }
    }
}

pub fn fmt1(template: &str, a: &str) -> String {
    template.replace("{0}", a)
}

pub fn fmt2(template: &str, a: &str, b: &str) -> String {
    template
        .replace("{0}", a)
        .replace("{1:00}", b)
        .replace("{1}", b)
}

pub fn fmt3(template: &str, a: &str, b: &str, c: &str) -> String {
    template
        .replace("{0}", a)
        .replace("{1}", b)
        .replace("{2}", c)
}
