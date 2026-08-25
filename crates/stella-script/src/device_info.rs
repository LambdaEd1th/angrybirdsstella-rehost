//! `pf::DeviceInfo` platform model queried before GameLua loads gamelogic.

/// `pf::DeviceInfo::DeviceInfoImpl::getModel` at `sub_10053B404` performs a
/// two-stage `sysctlbyname("hw.machine")` query and returns an empty string
/// when either stage fails or reports a zero length.
pub(crate) fn native_device_info_model() -> String {
    platform_device_info_model()
}

#[cfg(target_vendor = "apple")]
fn platform_device_info_model() -> String {
    query_native_machine(|buffer| {
        let mut length = buffer.as_ref().map_or(0, |bytes| bytes.len());
        let output = buffer.map_or(std::ptr::null_mut(), |bytes| bytes.as_mut_ptr().cast());
        // SAFETY: `hw.machine` is a static NUL-terminated key. The optional
        // output buffer and its mutable length follow sysctlbyname's contract.
        let result = unsafe {
            libc::sysctlbyname(
                c"hw.machine".as_ptr(),
                output,
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        (result != -1).then_some(length)
    })
}

#[cfg(all(unix, not(target_vendor = "apple")))]
fn platform_device_info_model() -> String {
    let mut value = std::mem::MaybeUninit::<libc::utsname>::uninit();
    // SAFETY: uname initializes the complete utsname object on success.
    if unsafe { libc::uname(value.as_mut_ptr()) } == -1 {
        return String::new();
    }
    // SAFETY: the successful call initialized `value`, and POSIX guarantees a
    // NUL-terminated machine field.
    let value = unsafe { value.assume_init() };
    // SAFETY: `machine` is NUL-terminated by uname.
    unsafe { std::ffi::CStr::from_ptr(value.machine.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(not(unix))]
fn platform_device_info_model() -> String {
    // Purple's iOS implementation has no corresponding Win32 branch. Retain
    // the same machine-identifier role with Rust's target architecture rather
    // than publishing a rehost branding string into the original scripts.
    match std::env::consts::ARCH {
        "aarch64" => "arm64".to_owned(),
        architecture => architecture.to_owned(),
    }
}

#[cfg(any(target_vendor = "apple", test))]
fn query_native_machine(mut query: impl FnMut(Option<&mut [u8]>) -> Option<usize>) -> String {
    let Some(length) = query(None).filter(|length| *length != 0) else {
        return String::new();
    };
    let mut bytes = vec![0_u8; length];
    let Some(length) = query(Some(&mut bytes)).filter(|length| *length != 0) else {
        return String::new();
    };
    let available = length.min(bytes.len());
    let end = bytes[..available]
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(available);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_stage_machine_query_matches_native_success_and_failure_boundaries() {
        let mut calls = 0;
        let model = query_native_machine(|buffer| {
            calls += 1;
            if let Some(buffer) = buffer {
                buffer[..10].copy_from_slice(b"iPhone7,2\0");
            }
            Some(10)
        });
        assert_eq!(model, "iPhone7,2");
        assert_eq!(calls, 2);

        assert_eq!(query_native_machine(|_| None), "");
        assert_eq!(query_native_machine(|_| Some(0)), "");

        let mut second_stage = false;
        assert_eq!(
            query_native_machine(|buffer| {
                if buffer.is_some() {
                    second_stage = true;
                    None
                } else {
                    Some(16)
                }
            }),
            ""
        );
        assert!(second_stage);
    }

    #[test]
    fn live_platform_model_is_not_the_removed_host_branding_literal() {
        assert_ne!(native_device_info_model(), "Stella Rust rehost");
    }
}
