//! Native access metadata: seconds east of UTC, minus 3600 for nonzero DST.

#[derive(Clone, Copy, Debug)]
pub(super) struct TimezoneError;

pub(super) fn current_offset() -> Result<String, TimezoneError> {
    // A null destination asks time() to return the timestamp directly.
    let seconds = unsafe { libc::time(std::ptr::null_mut()) };
    if seconds == -1 {
        return Err(TimezoneError);
    }
    offset_at(seconds)
}

fn offset_at(seconds: libc::time_t) -> Result<String, TimezoneError> {
    let (local, utc) = calendar_pair(seconds)?;
    // Windows CRT tm has no tm_gmtoff. The two calendars describe the SAME
    // instant, so their civil-second difference recovers the offset exactly,
    // including second/minute offsets and year boundaries, without mktime's
    // DST inference or mutating process-global TZ. Unix uses the same path.
    Ok(adjusted_offset(civil_seconds(&local) - civil_seconds(&utc), local.tm_isdst).to_string())
}

fn adjusted_offset(offset: i64, is_dst: i32) -> i64 {
    // 100692140..148 subtracts a fixed hour for ANY nonzero tm_isdst.
    // This intentionally is not the actual DST delta in half-hour DST zones.
    offset - if is_dst != 0 { 3600 } else { 0 }
}

fn civil_seconds(value: &libc::tm) -> i64 {
    let previous_year = i64::from(value.tm_year) + 1899;
    let days = 365 * previous_year + previous_year.div_euclid(4) - previous_year.div_euclid(100)
        + previous_year.div_euclid(400)
        + i64::from(value.tm_yday);
    days * 86400
        + i64::from(value.tm_hour) * 3600
        + i64::from(value.tm_min) * 60
        + i64::from(value.tm_sec)
}

fn calendar_pair(seconds: libc::time_t) -> Result<(libc::tm, libc::tm), TimezoneError> {
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    let mut utc = std::mem::MaybeUninit::<libc::tm>::uninit();
    // Each CRT routine writes caller-owned storage; only successful calls
    // permit assume_init. No shared static localtime/gmtime buffer is used.
    #[cfg(unix)]
    let success = unsafe {
        !libc::localtime_r(&seconds, local.as_mut_ptr()).is_null()
            && !libc::gmtime_r(&seconds, utc.as_mut_ptr()).is_null()
    };
    #[cfg(windows)]
    let success = unsafe {
        libc::localtime_s(local.as_mut_ptr(), &seconds) == 0
            && libc::gmtime_s(utc.as_mut_ptr(), &seconds) == 0
    };
    #[cfg(not(any(unix, windows)))]
    let success = false;
    if !success {
        return Err(TimezoneError);
    }
    Ok(unsafe { (local.assume_init(), utc.assume_init()) })
}

#[cfg(test)]
mod tests;
