//! Turns stretches of time into days, hours and top lists.
//!
//! A study day runs from 04:00 to 04:00 local time, so a session from 23:00 to 01:30
//! belongs to the evening it started in.

use crate::store::{Kind, Listen, Ms, Study};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Timelike};
use std::collections::{BTreeMap, HashMap};

const DAY_STARTS_AT: u32 = 4;

pub fn local(ms: Ms) -> DateTime<Local> {
    Local.timestamp_millis_opt(ms).earliest().unwrap_or_else(Local::now)
}

/// The study day `ms` belongs to.
pub fn study_day(ms: Ms) -> NaiveDate {
    (local(ms) - Duration::hours(DAY_STARTS_AT as i64)).date_naive()
}

/// When `day` begins (04:00 local), in ms.
fn day_start(day: NaiveDate) -> Ms {
    let naive = day.and_hms_opt(DAY_STARTS_AT, 0, 0).unwrap_or_default();
    Local.from_local_datetime(&naive).earliest().map_or(0, |t| t.timestamp_millis())
}

pub fn monday(day: NaiveDate) -> NaiveDate {
    day - Duration::days(day.weekday().num_days_from_monday() as i64)
}

/// Milliseconds studied on each day, with stretches split at 04:00.
pub fn per_day(study: &[Study], open: Option<(Ms, Ms)>) -> BTreeMap<NaiveDate, Ms> {
    let mut days = BTreeMap::new();
    for (mut start, end) in spans(study, open) {
        while start < end {
            let day = study_day(start);
            let split = day_start(day + Duration::days(1)).min(end).max(start + 1);
            *days.entry(day).or_insert(0) += split - start;
            start = split;
        }
    }
    days
}

/// Milliseconds studied in each hour of the clock (0 to 23) on days in `from..to`.
pub fn per_hour(study: &[Study], open: Option<(Ms, Ms)>, from: NaiveDate, to: NaiveDate) -> [Ms; 24] {
    let mut hours = [0; 24];
    let (from, to) = (day_start(from), day_start(to));
    for (start, end) in spans(study, open) {
        let (mut start, end) = (start.max(from), end.min(to));
        while start < end {
            let t = local(start);
            let next = (t.with_minute(0).and_then(|t| t.with_second(0)).and_then(|t| t.with_nanosecond(0)).unwrap_or(t)
                + Duration::hours(1))
            .timestamp_millis()
            .min(end)
            .max(start + 1);
            hours[t.hour() as usize] += next - start;
            start = next;
        }
    }
    hours
}

/// One line of a top list.
pub struct Entry {
    pub title: String,
    /// The artist under a song's title; empty for artists and podcasts.
    pub subtitle: String,
    /// The artist or show and album, which name the cover art.
    pub author: String,
    pub album: String,
    pub ms: Ms,
}

pub struct Top {
    pub songs: Vec<Entry>,
    pub artists: Vec<Entry>,
    pub podcasts: Vec<Entry>,
}

/// The songs, artists and podcasts heard most on days in `from..to`, `n` of each.
pub fn top(listening: &[Listen], from: NaiveDate, to: NaiveDate, n: usize) -> Top {
    let (from, to) = (day_start(from), day_start(to));
    let mut songs: HashMap<(&str, &str), Entry> = HashMap::new();
    let mut artists: HashMap<&str, Entry> = HashMap::new();
    let mut podcasts: HashMap<&str, Entry> = HashMap::new();
    let entry = |title: &str, subtitle: &str, l: &Listen| Entry {
        title: title.to_owned(),
        subtitle: subtitle.to_owned(),
        author: l.artist.clone(),
        album: l.album.clone(),
        ms: 0,
    };
    for l in listening {
        let ms = l.end.min(to) - l.start.max(from);
        if ms <= 0 {
            continue;
        }
        match l.kind {
            Kind::Song => {
                songs.entry((&l.title, &l.artist)).or_insert_with(|| entry(&l.title, &l.artist, l)).ms += ms;
                artists.entry(&l.artist).or_insert_with(|| entry(&l.artist, "", l)).ms += ms;
            }
            Kind::Podcast => podcasts.entry(&l.artist).or_insert_with(|| entry(&l.artist, "", l)).ms += ms,
        }
    }
    let sorted = |entries: Vec<Entry>| {
        let mut v = entries;
        v.sort_by_key(|e| std::cmp::Reverse(e.ms));
        v.truncate(n);
        v
    };
    Top {
        songs: sorted(songs.into_values().collect()),
        artists: sorted(artists.into_values().collect()),
        podcasts: sorted(podcasts.into_values().collect()),
    }
}

fn spans(study: &[Study], open: Option<(Ms, Ms)>) -> impl Iterator<Item = (Ms, Ms)> + '_ {
    study.iter().map(|s| (s.start, s.end)).chain(open)
}

/// "3 Std. 12 Min.", "45 Min.", "0 Min."
pub fn hours_minutes(ms: Ms) -> String {
    let minutes = ms.max(0) / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} Min."),
        (h, 0) => format!("{h} Std."),
        (h, m) => format!("{h} Std. {m} Min."),
    }
}

/// "1:02:03" or "2:03" for the running timer.
pub fn clock(ms: Ms) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn first_of_month(day: NaiveDate) -> NaiveDate {
    day.with_day(1).unwrap_or(day)
}

pub fn next_month(day: NaiveDate) -> NaiveDate {
    let (y, m) = if day.month() == 12 { (day.year() + 1, 1) } else { (day.year(), day.month() + 1) };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> Ms {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).earliest().unwrap().timestamp_millis()
    }

    #[test]
    fn late_night_counts_for_the_evening() {
        let s = Study { start: at(2026, 11, 16, 23, 0), end: at(2026, 11, 17, 1, 30), imported: false };
        let days = per_day(&[s], None);
        assert_eq!(days.len(), 1);
        assert_eq!(days[&NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()], 150 * 60_000);
    }

    #[test]
    fn splits_at_four_in_the_morning() {
        let s = Study { start: at(2026, 11, 17, 3, 0), end: at(2026, 11, 17, 5, 0), imported: false };
        let days = per_day(&[s], None);
        assert_eq!(days[&NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()], 60 * 60_000);
        assert_eq!(days[&NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()], 60 * 60_000);
    }

    #[test]
    fn hours_of_the_clock() {
        let s = Study { start: at(2026, 11, 17, 20, 30), end: at(2026, 11, 17, 22, 0), imported: false };
        let d = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        let h = per_hour(&[s], None, d, d + Duration::days(1));
        assert_eq!((h[20], h[21], h[22]), (30 * 60_000, 60 * 60_000, 0));
    }

    #[test]
    fn formats() {
        assert_eq!(hours_minutes(192 * 60_000), "3 Std. 12 Min.");
        assert_eq!(hours_minutes(45 * 60_000 + 59_000), "45 Min.");
        assert_eq!(clock(3_723_000), "1:02:03");
        assert_eq!(clock(123_000), "2:03");
    }
}
