//! Native CRT calendar conversion used by the world's second-moon cycle.

#![allow(unsafe_code)]

/// 76C1F0 converts the realm's local calendar midnight through CRT mktime,
/// then truncates epoch seconds / 86400. The computer supplies timezone rules,
/// while the complete date comes from the realm packet and its advancement.
pub(crate) fn realm_calendar_days(
    year: u16,
    month: u8,
    day: u8,
    advanced_days: u32,
) -> Option<i32> {
    let day = i32::from(day).checked_add(i32::try_from(advanced_days).ok()?)?;
    // SAFETY: Every native tm field permits zero; required calendar fields are set below.
    let mut time: libc::tm = unsafe { std::mem::zeroed() };
    time.tm_year = i32::from(year % 100) + 100;
    time.tm_mon = i32::from(month);
    time.tm_mday = day;
    time.tm_isdst = -1;
    // SAFETY: CRT receives a uniquely borrowed, initialized native tm and normalizes it.
    let seconds = unsafe { calendar_seconds(&mut time) };
    (seconds >= 0).then_some((seconds / 86_400) as i32)
}

#[cfg(windows)]
unsafe fn calendar_seconds(time: *mut libc::tm) -> i64 {
    unsafe extern "C" {
        fn _mktime64(time: *mut libc::tm) -> i64;
    }
    // SAFETY: The caller supplies the native Windows nine-int tm ABI.
    unsafe { _mktime64(time) }
}

#[cfg(not(windows))]
unsafe fn calendar_seconds(time: *mut libc::tm) -> i64 {
    // SAFETY: The caller supplies the target platform's initialized native tm ABI.
    unsafe { libc::mktime(time) as i64 }
}
