//! Dates as spreadsheets count them: serial numbers, days since 1899-12-30.
//!
//! They carry the 1900 leap-year quirk of the original spreadsheets (serial 60 is the day that never was, 1900-02-29). Serial 0 is the "zeroth" of January 1900.

#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

/// 9999-12-31.
pub const MAX_SERIAL: i64 = 2_958_465;

/// Days since 1970-01-01 of a proleptic Gregorian date.
#[must_use]
pub const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day count since 1970-01-01.
#[must_use]
pub const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

const BASE: i64 = days_from_civil(1899, 12, 30);

/// The serial number of a calendar date.
#[must_use]
pub const fn to_serial(y: i64, m: i64, d: i64) -> i64 {
    let s = days_from_civil(y, m, d) - BASE;
    if s < 61 { s - 1 } else { s }
}

/// The calendar date of a serial number (the fraction is ignored). Serial 0 is `(1900, 1, 0)`. `None` outside
/// `0..=MAX_SERIAL`.
#[must_use]
pub fn from_serial(serial: f64) -> Option<(i64, i64, i64)> {
    let s = serial.floor();
    if !(0.0..=MAX_SERIAL as f64).contains(&s) {
        return None;
    }
    let s = s as i64;
    Some(match s {
        0 => (1900, 1, 0),
        60 => (1900, 2, 29),
        1..=59 => civil_from_days(BASE + s + 1),
        _ => civil_from_days(BASE + s),
    })
}

/// The calendar date of a whole serial number 1 or more; `None` outside the calendar.
#[must_use]
pub fn from_serial_day(serial: f64) -> Option<(i64, i64, i64)> {
    if serial < 1.0 {
        return None;
    }
    from_serial(serial)
}

/// Days in a month of a year.
#[must_use]
pub const fn month_len(y: i64, m: i64) -> i64 {
    match m {
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 31,
    }
}

/// 0 for Sunday to 6 for Saturday (serial 1, 1900-01-01, counts as a Sunday, as spreadsheets do).
#[must_use]
pub const fn weekday_from_sunday(serial: f64) -> i64 {
    ((serial.floor() as i64) - 1).rem_euclid(7)
}
