//! Stable desktop DeviceID source and Purple's SHA-1 `uniqueDeviceId` projection.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::Path,
};

use mlua::{Result as LuaResult, Table};

use crate::app_data_path;

const DEVICE_ID_FILE: &str = "stella-device-id";
const UNAVAILABLE: &str = "unavailable";

pub(super) fn install(globals: &Table, data_root: &Path) -> LuaResult<()> {
    // 10002C9C0 calls the same SHA-1 wrapper (100539D84) used by access
    // persistentGuid. Keep the existing persisted source bytes unchanged.
    globals.set(
        "uniqueDeviceId",
        native_projection(&load_or_create(data_root)),
    )
}

fn native_projection(source: &str) -> String {
    let digest = crate::game_lua::platform::sha1_digest(source.as_bytes());
    crate::game_lua::platform::upper_hex(&digest)
}

fn load_or_create(data_root: &Path) -> String {
    load_or_create_with(data_root, generate_uuid_v4).unwrap_or_else(|| UNAVAILABLE.to_owned())
}

fn load_or_create_with(
    data_root: &Path,
    generate: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let path = app_data_path(data_root, DEVICE_ID_FILE).ok()?;
    if let Ok(value) = fs::read_to_string(&path) {
        return normalized_uuid(&value);
    }

    let value = normalized_uuid(&generate()?)?;
    fs::create_dir_all(path.parent()?).ok()?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(value.as_bytes()).ok()?;
            file.sync_all().ok()?;
            Some(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            normalized_uuid(&fs::read_to_string(path).ok()?)
        }
        Err(_) => None,
    }
}

fn generate_uuid_v4() -> Option<String> {
    crate::game_lua::platform::generate_uuid_v4().ok()
}

fn normalized_uuid(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() != 36
        || value.bytes().enumerate().any(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte != b'-'
            } else {
                !byte.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(value.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
    const FIRST: &str = "00112233-4455-4677-8899-AABBCCDDEEFF";
    const SECOND: &str = "FFEEDDCC-BBAA-4988-8776-554433221100";

    #[test]
    fn native_device_projection_hashes_source_bytes_including_fallback() {
        assert_eq!(
            native_projection("abc"),
            "A9993E364706816ABA3E25717850C26C9CD0D89D"
        );
        assert_eq!(
            native_projection(""),
            "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709"
        );
        assert_eq!(
            native_projection(UNAVAILABLE),
            "1D5EE313D50A7F8AEDF787B5A6C029EF09906C69"
        );
    }

    fn root(label: &str) -> std::path::PathBuf {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "stella-device-id-{label}-{}-{sequence}",
            std::process::id()
        ))
    }

    #[test]
    fn installation_uuid_is_reused_and_scoped_to_app_data() {
        let first_root = root("stable");
        let first_data = first_root.join("data");
        fs::create_dir_all(&first_data).unwrap();
        assert_eq!(
            load_or_create_with(&first_data, || Some(FIRST.to_owned())).as_deref(),
            Some(FIRST)
        );
        assert_eq!(
            load_or_create_with(&first_data, || Some(SECOND.to_owned())).as_deref(),
            Some(FIRST)
        );

        let second_root = root("separate");
        let second_data = second_root.join("data");
        fs::create_dir_all(&second_data).unwrap();
        assert_eq!(
            load_or_create_with(&second_data, || Some(SECOND.to_owned())).as_deref(),
            Some(SECOND)
        );

        fs::remove_dir_all(first_root).unwrap();
        fs::remove_dir_all(second_root).unwrap();
    }

    #[test]
    fn invalid_or_unwritable_state_uses_the_native_fallback() {
        let invalid_root = root("invalid");
        let invalid_data = invalid_root.join("data");
        fs::create_dir_all(invalid_root.join("appdata")).unwrap();
        fs::create_dir_all(&invalid_data).unwrap();
        fs::write(invalid_root.join("appdata").join(DEVICE_ID_FILE), "broken").unwrap();
        assert_eq!(load_or_create(&invalid_data), UNAVAILABLE);
        let lua = mlua::Lua::new();
        install(&lua.globals(), &invalid_data).unwrap();
        assert_eq!(
            lua.globals().get::<String>("uniqueDeviceId").unwrap(),
            native_projection(UNAVAILABLE)
        );
        assert_eq!(
            fs::read_to_string(invalid_root.join("appdata").join(DEVICE_ID_FILE)).unwrap(),
            "broken"
        );

        let blocked_root = root("blocked");
        let blocked_data = blocked_root.join("data");
        fs::create_dir_all(&blocked_data).unwrap();
        fs::write(blocked_root.join("appdata"), b"not a directory").unwrap();
        assert_eq!(load_or_create(&blocked_data), UNAVAILABLE);

        fs::remove_dir_all(invalid_root).unwrap();
        fs::remove_dir_all(blocked_root).unwrap();
    }

    #[test]
    fn generated_identity_has_uuid_v4_bits() {
        let value = generate_uuid_v4().unwrap();
        assert_eq!(value.len(), 36);
        assert_eq!(value.as_bytes()[14], b'4');
        assert!(matches!(value.as_bytes()[19], b'8' | b'9' | b'A' | b'B'));
        assert_eq!(normalized_uuid(&value).as_deref(), Some(value.as_str()));
    }
}
