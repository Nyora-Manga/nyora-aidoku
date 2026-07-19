//! Chapter date parsing.
//!
//! Two paths, matching MadaraEngine.kt:
//!   absolute ("March 3, 2024")  → host DateFormatter via parse_date_with_options
//!   relative ("2 hours ago")    → done here, because there is no host equivalent
//!
//! The relative forms are multilingual: Madara ships localised themes and the
//! word lists below come straight from the Kotlin engine's parseRelativeDate.

extern crate alloc;
use alloc::string::String;
use aidoku::imports::std::{current_date, parse_date_with_options};

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;

/// Returns a unix timestamp, or None if nothing could be read. Callers treat
/// None as "unknown", which Aidoku renders as no date rather than 1970.
pub fn parse(raw: &str, format: &str, locale: &str) -> Option<i64> {
    if raw.is_empty() {
        return None;
    }
    let lower = raw.to_lowercase();

    if let Some(ts) = relative(&lower) {
        return Some(ts);
    }
    // Absolute: hand to the host, which has real month-name tables per locale.
    // Returns None (not 0) on failure so a bad pattern shows as "no date"
    // instead of Jan 1 1970.
    if let Some(t) = parse_date_with_options(raw, format, locale, "").filter(|t| *t > 0) {
        return Some(t);
    }
    // Some hosts back parse_date with a date+TIME parser (the Aidoku test
    // runner uses chrono's NaiveDateTime), which can never match a date-only
    // string like "16.01.2026". Retry with a zero time appended. On hosts whose
    // parser already handles date-only this branch is never reached.
    if !format.contains('H') && !format.contains('h') {
        let padded_raw = alloc::format!("{raw} 00:00");
        let padded_fmt = alloc::format!("{format} HH:mm");
        if let Some(t) =
            parse_date_with_options(&padded_raw, &padded_fmt, locale, "").filter(|t| *t > 0)
        {
            return Some(t);
        }
    }
    // Last resort: the extracted config is incomplete for some sources (a row
    // with no datePattern gets a default that may not match the site at all),
    // so try the handful of formats these themes actually use before giving up
    // and showing no date.
    const COMMON: &[&str] = &[
        "MMMM d, yyyy", "MMM d, yyyy", "d MMMM yyyy", "dd/MM/yyyy",
        "MM/dd/yyyy", "yyyy-MM-dd", "dd.MM.yyyy", "yyyy/MM/dd",
    ];
    for f in COMMON {
        if *f == format {
            continue;
        }
        let padded_raw = alloc::format!("{raw} 00:00");
        let padded_fmt = alloc::format!("{f} HH:mm");
        if let Some(t) = parse_date_with_options(raw, f, locale, "")
            .or_else(|| parse_date_with_options(&padded_raw, &padded_fmt, locale, ""))
            .filter(|t| *t > 0)
        {
            return Some(t);
        }
    }
    None
}

fn relative(s: &str) -> Option<i64> {
    let now = current_date();

    // Whole-day words first — they carry no number.
    if s.contains("yesterday") || s.contains("يوم واحد") || s.contains("ontem") || s.contains("hier")
    {
        return Some(now - DAY);
    }
    if s.contains("today") || s.contains("hoje") || s.contains("aujourd") || s.contains("اليوم") {
        return Some(now);
    }
    if s.contains("يومين") {
        return Some(now - 2 * DAY);
    }

    // Everything else needs "<n> <unit> <ago-marker>".
    if !is_relative(s) {
        return None;
    }
    let n: i64 = first_number(s)?;
    let unit = unit_seconds(s)?;
    Some(now - n * unit)
}

/// Suffix/prefix markers that identify a relative date across the languages
/// Madara themes ship in.
fn is_relative(s: &str) -> bool {
    const MARKERS: &[&str] = &[
        " ago", "atrás", "hace", "publicado", "назад", "önce", "trước", "مضت", "منذ", "há ",
        "il y a", " dernier",
    ];
    MARKERS.iter().any(|m| s.contains(m))
}

fn unit_seconds(s: &str) -> Option<i64> {
    const SEC: &[&str] = &["second", "segundo", "saniye", "giây", "секунд", "ثانية"];
    const MIN: &[&str] = &["minute", "minuto", "min", "dakika", "phút", "минут", "دقيقة"];
    const HR: &[&str] = &["hour", "hora", "heure", "saat", "giờ", "час", "ساعة"];
    const DY: &[&str] = &["day", "día", "dia", "jour", "gün", "ngày", "дн", "день", "يوم"];
    const WK: &[&str] = &["week", "semana", "semaine", "hafta", "tuần", "недел", "أسبوع"];
    const MO: &[&str] = &["month", "mes", "mês", "mois", "ay", "tháng", "месяц", "شهر"];
    const YR: &[&str] = &["year", "año", "ano", "an ", "yıl", "năm", "год", "سنة"];

    // Order matters: "minute" contains "min", and "month" would otherwise be
    // matched by the "mo"-prefixed month list before the minute list is tried.
    if MIN.iter().any(|w| s.contains(w)) {
        return Some(MINUTE);
    }
    if HR.iter().any(|w| s.contains(w)) {
        return Some(HOUR);
    }
    if WK.iter().any(|w| s.contains(w)) {
        return Some(WEEK);
    }
    if DY.iter().any(|w| s.contains(w)) {
        return Some(DAY);
    }
    if MO.iter().any(|w| s.contains(w)) {
        return Some(MONTH);
    }
    if YR.iter().any(|w| s.contains(w)) {
        return Some(YEAR);
    }
    if SEC.iter().any(|w| s.contains(w)) {
        return Some(1);
    }
    None
}

fn first_number(s: &str) -> Option<i64> {
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            break;
        }
    }
    cur.parse().ok()
}
