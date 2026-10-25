// SPDX-License-Identifier: Apache-2.0
//! Validate Gregorian UTC calendar fields and convert firmware RTC samples to Unix seconds.
pub fn unix_seconds(
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
) -> Option<u64> {
    if !(2020..=2099).contains(&year)
        || !(1..=12).contains(&month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let leap = |y: u16| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let days = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || day > days[month as usize - 1] {
        return None;
    }
    let mut total = 0;
    for y in 1970..year {
        total += if leap(y) { 366 } else { 365 };
    }
    for &n in &days[..month as usize - 1] {
        total += u64::from(n);
    }
    total += u64::from(day - 1);
    Some(total * 86400 + u64::from(hour) * 3600 + u64::from(minute) * 60 + u64::from(second))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_dates_and_leap_boundaries() {
        assert_eq!(unix_seconds(2024, 1, 1, 0, 0, 0), Some(1704067200));
        assert_eq!(unix_seconds(2024, 2, 29, 0, 0, 0), Some(1709164800));
        assert!(unix_seconds(2025, 2, 29, 0, 0, 0).is_none());
        assert!(unix_seconds(2026, 0, 1, 0, 0, 0).is_none());
        assert!(unix_seconds(2026, 1, 1, 24, 0, 0).is_none());
        assert!(unix_seconds(2019, 1, 1, 0, 0, 0).is_none());
    }
}
