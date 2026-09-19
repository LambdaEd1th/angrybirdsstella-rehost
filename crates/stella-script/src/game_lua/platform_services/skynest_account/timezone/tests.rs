use super::*;

#[test]
fn timezone_adjustment_uses_seconds_and_any_nonzero_dst_flag() {
    for (raw, dst, expected) in [
        (20700, 0, 20700),
        (-12600, 0, -12600),
        (0, 0, 0),
        (-14400, 1, -18000),
        (39600, 1, 36000),
        (0, -1, -3600),
        (1234, 2, -2366),
    ] {
        assert_eq!(adjusted_offset(raw, dst), expected);
    }
}

#[test]
fn timezone_query_failure_is_explicit_and_sanitized() {
    let error = super::super::session::SessionError::from(TimezoneError);
    assert_eq!(error.status, -1);
    assert_eq!(error.to_string(), "identity timezone query failed");
    #[cfg(all(unix, target_pointer_width = "64"))]
    assert!(offset_at(libc::time_t::MAX).is_err());
}

#[test]
fn timezone_child_probe() {
    let Ok(case) = std::env::var("STELLA_TIMEZONE_CASE") else {
        return;
    };
    for pair in case.split(';') {
        let (seconds, expected) = pair.split_once(',').unwrap();
        let seconds = seconds.parse::<libc::time_t>().unwrap();
        assert_eq!(offset_at(seconds).unwrap(), expected);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let (local, utc) = calendar_pair(seconds).unwrap();
            // Independent platform field checks the portable civil-calendar
            // subtraction used for Windows CRT, including year/leap boundaries.
            assert_eq!(civil_seconds(&local) - civil_seconds(&utc), local.tm_gmtoff);
        }
    }
}

#[cfg(unix)]
#[test]
fn timezone_posix_child_processes_cover_dst_fractional_offsets_and_year_boundaries() {
    // TZ is set only on newly spawned test processes, never in the running
    // multithreaded host or system settings. Fixed timestamps avoid clock races.
    let executable = std::env::current_exe().unwrap();
    for (zone, winter, summer) in [
        ("UTC0", 0, 0),
        ("NST3:30", -12600, -12600),
        ("NPT-5:45", 20700, 20700),
        ("SMT-0:19:32", 1172, 1172),
        ("EST5EDT,M3.2.0/2,M11.1.0/2", -18000, -18000),
        ("LHST-10:30LHDT-11,M10.1.0/2,M4.1.0/2", 36000, 37800),
    ] {
        let cases = format!(
            "1704067200,{winter};1719792000,{summer};1709251200,{winter};1735689600,{winter}"
        );
        let output = std::process::Command::new(&executable)
            .args(["--exact", "game_lua::platform_services::skynest_account::timezone::tests::timezone_child_probe", "--nocapture"])
            .env("TZ", zone).env("STELLA_TIMEZONE_CASE", cases).output().unwrap();
        assert!(
            output.status.success(),
            "{zone}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}
