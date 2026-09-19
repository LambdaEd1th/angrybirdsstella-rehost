//! Runtime OS version for access metadata (native UIDevice.systemVersion).
//!
//! Desktop adaptation: Apple product version, other Unix kernel release, and
//! Windows NT major.minor.build. Never substitute the build host's version.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OsVersionError;

#[cfg(target_vendor = "apple")]
pub(super) fn current_version() -> Result<String, OsVersionError> {
    query_version_string(|buffer| {
        let mut length = buffer.as_ref().map_or(0, |bytes| bytes.len());
        let output = buffer.map_or(std::ptr::null_mut(), |bytes| bytes.as_mut_ptr().cast());
        // SAFETY: static C key, writable length and optional output storage
        // meet sysctlbyname's two-stage query contract. No value is written.
        let status = unsafe {
            libc::sysctlbyname(
                c"kern.osproductversion".as_ptr(),
                output,
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        (status == 0).then_some(length)
    })
}

#[cfg(all(unix, not(target_vendor = "apple")))]
pub(super) fn current_version() -> Result<String, OsVersionError> {
    let mut value = std::mem::MaybeUninit::<libc::utsname>::uninit();
    // SAFETY: uname initializes the complete caller-owned structure on success.
    if unsafe { libc::uname(value.as_mut_ptr()) } != 0 {
        return Err(OsVersionError);
    }
    let value = unsafe { value.assume_init() };
    // SAFETY: POSIX uname guarantees a NUL-terminated release field.
    let release = unsafe { std::ffi::CStr::from_ptr(value.release.as_ptr()) };
    version_text(release.to_bytes())
}

#[cfg(windows)]
pub(super) fn current_version() -> Result<String, OsVersionError> {
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut WindowsVersion) -> i32;
    }
    query_windows_version(|info| {
        // SAFETY: repr(C) RTL_OSVERSIONINFOW with initialized size and writable
        // storage; ntdll uses the system ABI and returns an NTSTATUS.
        unsafe { RtlGetVersion(info) }
    })
}

#[cfg(not(any(unix, windows)))]
pub(super) fn current_version() -> Result<String, OsVersionError> {
    Err(OsVersionError)
}

#[cfg(any(unix, test))]
fn version_text(bytes: &[u8]) -> Result<String, OsVersionError> {
    let text = std::str::from_utf8(bytes).map_err(|_| OsVersionError)?;
    if text.is_empty() {
        return Err(OsVersionError);
    }
    Ok(text.to_owned())
}

#[cfg(any(target_vendor = "apple", test))]
fn query_version_string(
    mut query: impl FnMut(Option<&mut [u8]>) -> Option<usize>,
) -> Result<String, OsVersionError> {
    let length = query(None)
        .filter(|size| *size != 0)
        .ok_or(OsVersionError)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| OsVersionError)?;
    bytes.resize(length, 0);
    let written = query(Some(&mut bytes)).ok_or(OsVersionError)?;
    if written == 0 || written > bytes.len() {
        return Err(OsVersionError);
    }
    let text =
        std::ffi::CStr::from_bytes_until_nul(&bytes[..written]).map_err(|_| OsVersionError)?;
    version_text(text.to_bytes())
}

#[cfg(any(windows, test))]
#[repr(C)]
struct WindowsVersion {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}

#[cfg(any(windows, test))]
fn query_windows_version(
    mut query: impl FnMut(&mut WindowsVersion) -> i32,
) -> Result<String, OsVersionError> {
    let mut info = WindowsVersion {
        size: std::mem::size_of::<WindowsVersion>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    if query(&mut info) != 0 {
        return Err(OsVersionError);
    }
    Ok(format!("{}.{}.{}", info.major, info.minor, info.build))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_version_string_query_checks_both_calls_and_exact_bytes() {
        let mut calls = 0;
        assert_eq!(
            query_version_string(|buffer| {
                calls += 1;
                if let Some(buffer) = buffer {
                    buffer.copy_from_slice(b"15.6.1\0");
                }
                Some(7)
            })
            .unwrap(),
            "15.6.1"
        );
        assert_eq!(calls, 2);
        for size in [None, Some(0), Some(usize::MAX)] {
            let mut calls = 0;
            assert_eq!(
                query_version_string(|_| {
                    calls += 1;
                    size
                }),
                Err(OsVersionError)
            );
            assert_eq!(calls, 1);
        }
        for result in [None, Some(0), Some(9), Some(4)] {
            let mut calls = 0;
            assert_eq!(
                query_version_string(|buffer| {
                    calls += 1;
                    if let Some(buffer) = buffer {
                        buffer.fill(b'x'); // No NUL: do not publish truncated data.
                        result
                    } else {
                        Some(4)
                    }
                }),
                Err(OsVersionError)
            );
            assert_eq!(calls, 2);
        }
        assert_eq!(version_text(b""), Err(OsVersionError));
        assert_eq!(version_text(b"\xff"), Err(OsVersionError));
        assert_eq!(
            version_text(b"6.12.0-custom+build").unwrap(),
            "6.12.0-custom+build"
        );
    }

    #[test]
    fn os_version_windows_abi_and_status_are_preserved() {
        assert_eq!(std::mem::size_of::<WindowsVersion>(), 276);
        assert_eq!(std::mem::offset_of!(WindowsVersion, service_pack), 20);
        assert_eq!(
            query_windows_version(|info| {
                assert_eq!(info.size, 276);
                info.major = 10;
                info.minor = 0;
                info.build = 26100;
                0
            })
            .unwrap(),
            "10.0.26100"
        );
        for status in [1, -1, i32::MIN] {
            assert_eq!(query_windows_version(|_| status), Err(OsVersionError));
        }
    }

    #[test]
    fn os_version_failure_is_a_sanitized_session_error() {
        let error = super::super::session::SessionError::from(OsVersionError);
        assert_eq!(error.status, -1);
        assert_eq!(error.to_string(), "identity OS version query failed");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn os_version_live_matches_independent_product_version_tool() {
        let output = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            current_version().unwrap(),
            String::from_utf8(output.stdout).unwrap().trim()
        );
    }
}
