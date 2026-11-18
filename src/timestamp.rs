//! Date-time coordinate columns.
//!
//! Generic tables may carry their time axis as text (`2026-11-17T09:30:00Z`,
//! `17.11.2026 09:30:00`, `09:30:00`) or as spreadsheet date cells. Such a
//! column becomes a coordinate in seconds elapsed since its first valid row,
//! so analyses and event times keep working in relative time, and the absolute
//! start is kept in [`TimeOrigin`]. Differences are computed in integer
//! nanoseconds before conversion, so they do not lose precision to the size of
//! a calendar timestamp.
//!
//! A numeric `a/b/yyyy` date is read only when the column settles its order or
//! the caller states it. It is never guessed.

use crate::formats::{DetectedLayout, explicit_comment_row, numeric, trim_cell};
use crate::{ReadOptions, Severity};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use std::collections::{BTreeSet, HashMap};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

const NANOS_PER_SECOND: i128 = 1_000_000_000;
const NANOS_PER_DAY: i128 = 86_400 * NANOS_PER_SECOND;
/// A clock-only column that falls back by more than this has passed midnight.
const ROLLOVER_DROP_NANOS: i128 = NANOS_PER_DAY / 2;
/// Data rows inspected when deciding whether a column holds date-times.
const SAMPLE_ROWS: usize = 5;

/// Field order of a numeric `a/b/yyyy` or `a-b-yyyy` date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DateOrder {
    /// `dd/mm/yyyy`.
    DayFirst,
    /// `mm/dd/yyyy`.
    MonthFirst,
}

/// How a date-time column was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TimestampFormat {
    /// `yyyy-mm-dd`, `yyyy/mm/dd` or `yyyy.mm.dd`, with an optional time and,
    /// for these forms only, an optional UTC offset.
    YearFirst,
    /// `dd.mm.yyyy`, `dd-Mon-yyyy` or a numeric date established as day-first.
    DayFirst,
    /// A numeric date established as month-first.
    MonthFirst,
    /// One numeric date whose order is unknown. Elapsed time is unaffected
    /// because every row has that same date, but the calendar date is not known.
    UnresolvedDateOrder,
    /// Time of day without a date; midnight rollovers are inferred.
    ClockTime,
    /// A duration such as `36:00:01` or a spreadsheet `[h]:mm:ss` cell.
    ElapsedDuration,
}

/// The source timestamp at coordinate zero of a date-time column.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TimeOrigin {
    /// The timestamp as written in the source.
    pub source_text: String,
    /// One-based source row.
    pub source_row: usize,
    /// Interpretation applied to the column.
    pub format: TimestampFormat,
    /// Date and time as written. `None` when the source gives no certain date.
    pub date_time: Option<NaiveDateTime>,
    /// UTC offset as written. `None` when the source states none.
    pub utc_offset_seconds: Option<i32>,
}

impl TimeOrigin {
    /// ISO-8601 text of the origin, when its calendar date is known.
    pub fn iso8601(&self) -> Option<String> {
        let date_time = self.date_time?;
        let mut text = date_time.format("%Y-%m-%dT%H:%M:%S%.f").to_string();
        if let Some(offset) = self.utc_offset_seconds {
            let sign = if offset < 0 { '-' } else { '+' };
            let minutes = offset.abs() / 60;
            text.push_str(&format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60));
        }
        Some(text)
    }
}

/// Columns of a table that hold date-times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimestampColumns {
    /// Column that becomes the coordinate. `None` when a numeric time column
    /// already provides it.
    pub time: Option<usize>,
    /// Date-time text consumed by `time`, or standing beside a numeric time
    /// column. It holds no measurement.
    pub absorbed: Option<usize>,
}

impl TimestampColumns {
    pub(crate) fn contains(&self, column: usize) -> bool {
        self.time == Some(column) || self.absorbed == Some(column)
    }
}

/// True for a heading that may name a date-time column. Such a column is used
/// only if its values are date-times, so numeric columns keep their meaning.
pub(crate) fn date_time_header(name: &str) -> bool {
    let compact: String = crate::detection::normalize_for_match(name)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    matches!(
        compact.split('(').next().unwrap_or(&compact),
        "datetime"
            | "date_time"
            | "date-time"
            | "date/time"
            | "date&time"
            | "timestamp"
            | "time_stamp"
            | "time-stamp"
    )
}

/// Finds date-time columns from a header and sample data rows.
pub(crate) fn timestamp_columns(
    header: Option<&[String]>,
    samples: &[&[String]],
) -> Option<TimestampColumns> {
    let every = |column: usize, accepted: &dyn Fn(&str) -> bool| {
        let mut seen = false;
        for row in samples {
            let value = row.get(column).map_or("", |cell| trim_cell(cell));
            if value.is_empty() || missing_token(value) {
                continue;
            }
            if !accepted(value) {
                return false;
            }
            seen = true;
        }
        seen
    };
    // A date-time column may hold bad rows, which the coordinate policy then
    // handles, but it must start with a timestamp and mostly hold them.
    let stamped = |column: usize| {
        let values: Vec<&str> = samples
            .iter()
            .filter_map(|row| row.get(column))
            .map(|cell| trim_cell(cell))
            .filter(|value| !value.is_empty() && !missing_token(value))
            .collect();
        let valid = values
            .iter()
            .filter(|value| parse_token(value).is_some())
            .count();
        values
            .first()
            .is_some_and(|first| parse_token(first).is_some())
            && valid * 2 > values.len()
    };
    let Some(header) = header else {
        return stamped(0).then_some(TimestampColumns {
            time: Some(0),
            absorbed: None,
        });
    };
    let time_columns: Vec<usize> = (0..header.len())
        .filter(|&column| crate::formats::header_role(&header[column]) == crate::ColumnRole::Time)
        .collect();
    let date_column = (0..header.len())
        .find(|&column| crate::detection::normalize_for_match(&header[column]) == "date");
    if let Some(date) = date_column
        && every(date, &|value| {
            parse_token(value).is_some_and(|token| token.is_date_only())
        })
        && let Some(&time) = time_columns.iter().find(|&&column| {
            every(column, &|value| {
                parse_token(value).is_some_and(|token| token.is_clock_only())
            })
        })
    {
        return Some(TimestampColumns {
            time: Some(time),
            absorbed: Some(date),
        });
    }
    let named_columns = (0..header.len()).filter(|&column| date_time_header(&header[column]));
    let stamped_column = time_columns
        .iter()
        .copied()
        .chain(named_columns)
        .chain(date_column)
        .find(|&column| stamped(column))?;
    let numeric_time = time_columns
        .iter()
        .any(|&column| every(column, &|value| numeric(value)));
    Some(if numeric_time {
        TimestampColumns {
            time: None,
            absorbed: Some(stamped_column),
        }
    } else {
        TimestampColumns {
            time: Some(stamped_column),
            absorbed: None,
        }
    })
}

/// Finds date-time columns of a detected table.
pub(crate) fn detected_columns(
    rows: &[Vec<String>],
    layout: &DetectedLayout,
) -> Option<TimestampColumns> {
    let header = layout
        .header_row
        .and_then(|row| rows.get(row))
        .map(Vec::as_slice);
    let samples: Vec<&[String]> = rows
        .iter()
        .skip(layout.data_start_row)
        .filter(|row| row.iter().any(|cell| !cell.trim().is_empty()) && !explicit_comment_row(row))
        .take(SAMPLE_ROWS)
        .map(Vec::as_slice)
        .collect();
    timestamp_columns(header, &samples)
}

fn missing_token(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "na" | "n/a" | "nan" | "null" | "missing"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    YearFirst,
    DayFirst,
    NumericPair,
    Clock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatePart {
    None,
    Known(NaiveDate),
    Pair { first: u32, second: u32, year: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Token {
    family: Family,
    date: DatePart,
    has_time: bool,
    clock_nanos: i64,
    offset_seconds: Option<i32>,
    /// Written as a duration rather than a time of day.
    elapsed: bool,
}

impl Token {
    fn is_date_only(&self) -> bool {
        self.family != Family::Clock && !self.has_time
    }

    fn is_clock_only(&self) -> bool {
        self.family == Family::Clock && !self.elapsed
    }
}

fn digits(text: &str, lengths: std::ops::RangeInclusive<usize>) -> Option<u32> {
    (lengths.contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let name = name.to_ascii_lowercase();
    if name.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|month| *month == name || month.get(..3) == Some(name.as_str()))
        .map(|index| index as u32 + 1)
}

fn parse_date(text: &str) -> Option<(Family, DatePart)> {
    let separator = text.chars().find(|c| matches!(c, '-' | '/' | '.'))?;
    let fields: Vec<&str> = text.split(separator).collect();
    let [first, second, third] = fields.as_slice() else {
        return None;
    };
    let known = |year: u32, month: u32, day: u32| {
        NaiveDate::from_ymd_opt(year as i32, month, day).map(DatePart::Known)
    };
    if let Some(year) = digits(first, 4..=4) {
        let date = known(year, digits(second, 1..=2)?, digits(third, 1..=2)?)?;
        return Some((Family::YearFirst, date));
    }
    let leading = digits(first, 1..=2)?;
    let year = digits(third, 4..=4)?;
    if let Some(month) = month_number(second) {
        return Some((Family::DayFirst, known(year, month, leading)?));
    }
    let middle = digits(second, 1..=2)?;
    if separator == '.' {
        return Some((Family::DayFirst, known(year, middle, leading)?));
    }
    let plausible = |value: u32| (1..=31).contains(&value);
    (plausible(leading) && plausible(middle) && (leading <= 12 || middle <= 12)).then_some((
        Family::NumericPair,
        DatePart::Pair {
            first: leading,
            second: middle,
            year: year as i32,
        },
    ))
}

/// `h:mm[:ss[.f]]` as hours and nanoseconds past the hour.
fn parse_clock(text: &str) -> Option<(u32, i64)> {
    let fields: Vec<&str> = text.split(':').collect();
    let (hour, minute, second) = match fields.as_slice() {
        [hour, minute] => (*hour, *minute, None),
        [hour, minute, second] => (*hour, *minute, Some(*second)),
        _ => return None,
    };
    let hour = digits(hour, 1..=6)?;
    let minute = digits(minute, 2..=2).filter(|minute| *minute < 60)?;
    let mut nanos = i64::from(minute) * 60 * 1_000_000_000;
    if let Some(second) = second {
        let (whole, fraction) = match second.find(['.', ',']) {
            Some(position) => (&second[..position], Some(&second[position + 1..])),
            None => (second, None),
        };
        let whole = digits(whole, 2..=2).filter(|whole| *whole < 60)?;
        nanos += i64::from(whole) * 1_000_000_000;
        if let Some(fraction) = fraction {
            if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let kept = &fraction[..fraction.len().min(9)];
            nanos += kept.parse::<i64>().ok()? * 10_i64.pow(9 - kept.len() as u32);
        }
    }
    Some((hour, nanos))
}

fn parse_offset(text: &str) -> Option<i32> {
    let (sign, rest) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    let (hours, minutes) = match rest.split_once(':') {
        Some((hours, minutes)) => (hours, minutes),
        None if rest.len() == 4 => rest.split_at(2),
        None => (rest, "00"),
    };
    let hours = digits(hours, 2..=2).filter(|hours| *hours <= 14)?;
    let minutes = digits(minutes, 2..=2).filter(|minutes| *minutes < 60)?;
    Some(sign * (hours * 3600 + minutes * 60) as i32)
}

/// A time of day, with optional AM/PM and, when allowed, UTC offset.
fn parse_time(text: &str, allow_offset: bool) -> Option<(i64, Option<i32>)> {
    let mut text = text.trim();
    let mut offset = None;
    if allow_offset {
        if let Some(rest) = text.strip_suffix(['Z', 'z']) {
            text = rest.trim_end();
            offset = Some(0);
        } else if let Some(position) = text.rfind(['+', '-']) {
            offset = Some(parse_offset(&text[position..])?);
            text = text[..position].trim_end();
        }
    }
    let lower = text.to_ascii_lowercase();
    let (clock, afternoon) = match (lower.strip_suffix("am"), lower.strip_suffix("pm")) {
        (Some(clock), _) => (clock.trim_end().to_owned(), Some(false)),
        (_, Some(clock)) => (clock.trim_end().to_owned(), Some(true)),
        _ => (lower, None),
    };
    let (hour, nanos) = parse_clock(&clock)?;
    let hour = match afternoon {
        Some(afternoon) if (1..=12).contains(&hour) => hour % 12 + if afternoon { 12 } else { 0 },
        Some(_) => return None,
        None => hour,
    };
    (hour < 24).then_some((i64::from(hour) * 3_600_000_000_000 + nanos, offset))
}

fn clock_token(text: &str) -> Option<Token> {
    let (body, forced_elapsed) = match text.strip_prefix('+') {
        Some(body) => (body, true),
        None => (text, false),
    };
    // Two fields could be h:mm or m:ss, so a clock alone needs its seconds.
    if body.split(':').count() != 3 {
        return None;
    }
    let (clock_nanos, elapsed) = match parse_clock(body) {
        Some((hour, nanos)) if forced_elapsed || hour >= 24 => {
            (i64::from(hour) * 3_600_000_000_000 + nanos, true)
        }
        _ if forced_elapsed => return None,
        _ => (parse_time(body, false)?.0, false),
    };
    Some(Token {
        family: Family::Clock,
        date: DatePart::None,
        has_time: true,
        clock_nanos,
        offset_seconds: None,
        elapsed,
    })
}

/// Splits at the first space, or at a `T` between digits.
fn split_date_time(text: &str) -> (&str, Option<&str>) {
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        let between_digits = matches!(byte, b'T' | b't')
            && index > 0
            && bytes[index - 1].is_ascii_digit()
            && bytes.get(index + 1).is_some_and(u8::is_ascii_digit);
        if byte.is_ascii_whitespace() || between_digits {
            return (
                text[..index].trim_end_matches(','),
                Some(text[index + 1..].trim()),
            );
        }
    }
    (text, None)
}

fn parse_token(text: &str) -> Option<Token> {
    let text = trim_cell(text);
    if text.is_empty() || numeric(text) {
        return None;
    }
    let (date_text, time_text) = split_date_time(text);
    if date_text.contains(':') {
        return clock_token(text);
    }
    let (family, date) = parse_date(date_text)?;
    let (clock_nanos, offset_seconds) = match time_text {
        Some(time) => parse_time(time, family == Family::YearFirst)?,
        None => (0, None),
    };
    Some(Token {
        family,
        date,
        has_time: time_text.is_some(),
        clock_nanos,
        offset_seconds,
        elapsed: false,
    })
}

/// A condition recorded while reading a date-time column.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimestampNote {
    pub severity: Severity,
    pub row_index: usize,
    pub message: String,
}

/// A date-time column that cannot be read without guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DateOrderConflict {
    pub row_index: usize,
    pub column: usize,
    pub message: String,
}

/// Elapsed seconds per zero-based source row, with their origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedTimestamps {
    pub columns: TimestampColumns,
    pub seconds: HashMap<usize, f64>,
    pub origin: Option<TimeOrigin>,
    pub notes: Vec<TimestampNote>,
}

struct Stamp {
    row_index: usize,
    text: String,
    token: Token,
}

fn resolve_date_order(
    stamps: &[Stamp],
    column: usize,
    requested: Option<DateOrder>,
) -> Result<Option<DateOrder>, DateOrderConflict> {
    let pair = |stamp: &Stamp| match stamp.token.date {
        DatePart::Pair {
            first,
            second,
            year,
        } => Some((first, second, year)),
        _ => None,
    };
    let row_where = |test: &dyn Fn(u32, u32) -> bool| {
        stamps
            .iter()
            .find(|stamp| pair(stamp).is_some_and(|(first, second, _)| test(first, second)))
    };
    let day_first = row_where(&|first, _| first > 12);
    let month_first = row_where(&|_, second| second > 12);
    let conflict = |stamp: &Stamp, message: String| DateOrderConflict {
        row_index: stamp.row_index,
        column,
        message,
    };
    match (day_first, month_first, requested) {
        (Some(day), Some(month), _) => Err(conflict(
            month,
            format!(
                "'{}' is only valid day-first but '{}' is only valid month-first",
                day.text, month.text
            ),
        )),
        (Some(day), None, Some(DateOrder::MonthFirst)) => Err(conflict(
            day,
            format!(
                "month-first dates were requested but '{}' is only valid day-first",
                day.text
            ),
        )),
        (None, Some(month), Some(DateOrder::DayFirst)) => Err(conflict(
            month,
            format!(
                "day-first dates were requested but '{}' is only valid month-first",
                month.text
            ),
        )),
        (Some(_), None, _) => Ok(Some(DateOrder::DayFirst)),
        (None, Some(_), _) => Ok(Some(DateOrder::MonthFirst)),
        (None, None, Some(order)) => Ok(Some(order)),
        (None, None, None) => {
            let dates: BTreeSet<_> = stamps.iter().filter_map(pair).collect();
            if dates.len() > 1 {
                Err(conflict(
                    &stamps[0],
                    format!(
                        "{} dates such as '{}' fit both day-first and month-first order; state the date order",
                        dates.len(),
                        stamps[0].text
                    ),
                ))
            } else {
                Ok(None)
            }
        }
    }
}

fn stamp_date(token: &Token, order: Option<DateOrder>) -> Option<NaiveDate> {
    match token.date {
        DatePart::Known(date) => Some(date),
        DatePart::Pair {
            first,
            second,
            year,
        } => match order? {
            DateOrder::DayFirst => NaiveDate::from_ymd_opt(year, second, first),
            DateOrder::MonthFirst => NaiveDate::from_ymd_opt(year, first, second),
        },
        DatePart::None => None,
    }
}

/// Converts a detected date-time column to elapsed seconds. Rows that cannot be
/// read in the column's format stay absent, so the normal coordinate policy
/// applies to them.
pub(crate) fn resolve(
    rows: &[Vec<String>],
    columns: TimestampColumns,
    layout: &DetectedLayout,
    options: &ReadOptions,
) -> Result<ResolvedTimestamps, DateOrderConflict> {
    let mut resolved = ResolvedTimestamps {
        columns,
        seconds: HashMap::new(),
        origin: None,
        notes: Vec::new(),
    };
    let Some(column) = columns.time else {
        return Ok(resolved);
    };
    let date_column = columns.absorbed;
    let mut stamps: Vec<Stamp> = Vec::new();
    for (row_index, row) in rows.iter().enumerate().skip(layout.data_start_row) {
        let Some(time) = row.get(column).map(|cell| trim_cell(cell)) else {
            continue;
        };
        let text = match date_column {
            Some(date) => format!("{} {time}", row.get(date).map_or("", |c| trim_cell(c))),
            None => time.to_owned(),
        };
        let Some(token) = parse_token(text.trim()) else {
            continue;
        };
        if stamps.first().is_none_or(|first| {
            first.token.family == token.family
                && first.token.offset_seconds.is_some() == token.offset_seconds.is_some()
        }) {
            stamps.push(Stamp {
                row_index,
                text: text.trim().to_owned(),
                token,
            });
        }
    }
    let Some(first) = stamps.first() else {
        return Ok(resolved);
    };
    let family = first.token.family;
    let first_row = first.row_index;
    let order = if family == Family::NumericPair {
        resolve_date_order(&stamps, column, options.date_order)?
    } else {
        None
    };
    let unresolved = family == Family::NumericPair && order.is_none();
    let elapsed_column = stamps.iter().any(|stamp| stamp.token.elapsed);
    let mut instants: Vec<(usize, i128)> = Vec::with_capacity(stamps.len());
    let mut retained: Vec<&Stamp> = Vec::with_capacity(stamps.len());
    let mut day = 0_i128;
    let mut previous_clock = None;
    let mut rollovers = 0_usize;
    for stamp in &stamps {
        let clock = i128::from(stamp.token.clock_nanos);
        let instant = if family == Family::Clock {
            if !elapsed_column {
                if previous_clock.is_some_and(|before| clock < before - ROLLOVER_DROP_NANOS) {
                    day += 1;
                    rollovers += 1;
                }
                previous_clock = Some(clock);
            }
            day * NANOS_PER_DAY + clock
        } else if unresolved {
            clock
        } else {
            let Some(date) = stamp_date(&stamp.token, order) else {
                continue;
            };
            i128::from(date.num_days_from_ce()) * NANOS_PER_DAY + clock
                - i128::from(stamp.token.offset_seconds.unwrap_or(0)) * NANOS_PER_SECOND
        };
        instants.push((stamp.row_index, instant));
        retained.push(stamp);
    }
    let (Some(&(_, origin_instant)), Some(origin)) = (instants.first(), retained.first()) else {
        return Ok(resolved);
    };
    resolved.seconds = instants
        .iter()
        .map(|&(row, instant)| {
            (
                row,
                (instant - origin_instant) as f64 / NANOS_PER_SECOND as f64,
            )
        })
        .collect();
    let format = match (family, order) {
        (Family::YearFirst, _) => TimestampFormat::YearFirst,
        (Family::DayFirst, _) | (Family::NumericPair, Some(DateOrder::DayFirst)) => {
            TimestampFormat::DayFirst
        }
        (Family::NumericPair, Some(DateOrder::MonthFirst)) => TimestampFormat::MonthFirst,
        (Family::NumericPair, None) => TimestampFormat::UnresolvedDateOrder,
        (Family::Clock, _) if elapsed_column => TimestampFormat::ElapsedDuration,
        (Family::Clock, _) => TimestampFormat::ClockTime,
    };
    let date_time = stamp_date(&origin.token, order).and_then(|date| {
        let seconds = (origin.token.clock_nanos / 1_000_000_000) as u32;
        let nanos = (origin.token.clock_nanos % 1_000_000_000) as u32;
        NaiveTime::from_num_seconds_from_midnight_opt(seconds, nanos)
            .map(|time| date.and_time(time))
    });
    resolved.notes.push(TimestampNote {
        severity: Severity::Info,
        row_index: origin.row_index,
        message: format!(
            "date-time coordinate converted to seconds elapsed since '{}' ({format:?})",
            origin.text
        ),
    });
    if unresolved {
        resolved.notes.push(TimestampNote {
            severity: Severity::Warning,
            row_index: first_row,
            message: format!(
                "the date in '{}' fits both day-first and month-first order; elapsed time is unaffected, but the calendar date is not recorded",
                origin.text
            ),
        });
    }
    if rollovers > 0 {
        resolved.notes.push(TimestampNote {
            severity: Severity::Warning,
            row_index: first_row,
            message: format!(
                "{rollovers} midnight rollover(s) inferred in a time-of-day column; a gap of 24 h or more cannot be detected without dates"
            ),
        });
    }
    resolved.origin = Some(TimeOrigin {
        source_text: origin.text.clone(),
        source_row: origin.row_index + 1,
        format,
        date_time,
        utc_offset_seconds: origin.token.offset_seconds,
    });
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_timestamp(text: &str) -> bool {
        parse_token(text).is_some()
    }

    #[test]
    fn recognizes_supported_forms_and_rejects_lookalikes() {
        for text in [
            "2026-11-17",
            "2026-11-17T09:30:00Z",
            "2026-11-17 09:30:00.125+01:00",
            "2026/11/17 09:30",
            "17.11.2026 09:30:00",
            "17-Nov-2026 09:30:00",
            "17-OCT-2026 09:30:00",
            "11/17/2026 9:30:00 AM",
            "09:30:00",
            "36:00:01",
            "+0:00:01.500",
        ] {
            assert!(is_timestamp(text), "{text}");
        }
        for text in [
            "",
            "12.5",
            "1e5",
            "20261117",
            "12:30",
            "2026-13-01",
            "2026-02-30",
            "17/11/26 09:30:00",
            "13/13/2026",
            "09:61:00",
            "09:30:60",
            "13:00:00 PM",
            "17.11.2026 09:30:00+01:00",
            "voltage",
        ] {
            assert!(!is_timestamp(text), "{text}");
        }
    }

    #[test]
    fn origin_text_is_iso8601() {
        let origin = TimeOrigin {
            source_text: String::new(),
            source_row: 2,
            format: TimestampFormat::YearFirst,
            date_time: NaiveDate::from_ymd_opt(2026, 11, 17)
                .unwrap()
                .and_hms_milli_opt(9, 30, 0, 250),
            utc_offset_seconds: Some(-5400),
        };
        assert_eq!(origin.iso8601().unwrap(), "2026-11-17T09:30:00.250-01:30");
    }

    #[test]
    fn year_is_read_back_from_dates() {
        let (_, date) = parse_date("2026-11-17").unwrap();
        let DatePart::Known(date) = date else {
            panic!("expected a known date");
        };
        assert_eq!(date.year(), 2026);
    }
}
