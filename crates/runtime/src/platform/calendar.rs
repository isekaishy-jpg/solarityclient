//! Native CRT calendar conversion used by the world's second-moon cycle.

#![allow(unsafe_code)]

/// 86D490 names screenshots using the computer's local calendar, to the second.
pub(crate) fn screenshot_timestamp() -> Option<String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let seconds = i64::try_from(seconds).ok()?;
    // SAFETY: Zero initializes all fields, which the CRT fills on success.
    let mut time: libc::tm = unsafe { std::mem::zeroed() };
    #[cfg(windows)]
    {
        unsafe extern "C" {
            fn _localtime64_s(result: *mut libc::tm, seconds: *const i64) -> i32;
        }
        // SAFETY: Both pointers reference initialized values with the CRT ABI.
        if unsafe { _localtime64_s(&mut time, &seconds) } != 0 {
            return None;
        }
    }
    #[cfg(not(windows))]
    {
        let seconds = libc::time_t::try_from(seconds).ok()?;
        // SAFETY: The CRT writes only to the uniquely borrowed native tm.
        if unsafe { libc::localtime_r(&seconds, &mut time) }.is_null() {
            return None;
        }
    }
    Some(format!(
        "{:02}{:02}{:02}_{:02}{:02}{:02}",
        time.tm_mon + 1,
        time.tm_mday,
        (time.tm_year + 1900) % 100,
        time.tm_hour,
        time.tm_min,
        time.tm_sec
    ))
}

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
