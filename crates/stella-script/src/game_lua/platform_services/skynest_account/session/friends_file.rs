//! The independent skynest_friends_store_<account> encrypted string file.
//! Native 1006FBC28/1006FBEB0/1006FC240; AppDataOutputStream 100505080.

use super::registry_codec::{decode_with_key, encode_with_key};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

// Fixed native file-format constant at 1006FBC98, unrelated to credentials.
// Deliberately distinct from fusion.registry's fixed key.
const FRIENDS_KEY: [u8; 32] = [
    0x34, 0x34, 0x69, 0x55, 0x59, 0x35, 0x61, 0x54, 0x72, 0x6c, 0x61, 0x59, 0x6f, 0x65, 0x74, 0x39,
    0x6c, 0x61, 0x70, 0x52, 0x6c, 0x61, 0x4b, 0x31, 0x45, 0x68, 0x6c, 0x65, 0x63, 0x35, 0x69, 0x30,
];
const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub(in super::super) struct FriendsFile {
    path: PathBuf,
}

impl FriendsFile {
    pub(in super::super) fn for_account(registry: &Path, account: &str) -> Result<Self, String> {
        // Host containment rule: native concatenation trusts the service ID.
        if account.contains(['/', '\\', '\0']) {
            return Err("friends cache account contains a path separator or NUL".to_owned());
        }
        let root = registry.parent().ok_or("friends cache root unavailable")?;
        Ok(Self {
            path: root.join(format!("skynest_friends_store_{account}")),
        })
    }

    pub(in super::super) fn path(&self) -> &Path {
        &self.path
    }

    pub(in super::super) fn read(&self) -> Result<String, String> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
            Err(e) => return Err(format!("friends cache read: {e}")),
        };
        let mut ciphertext = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut ciphertext)
            .map_err(|e| format!("friends cache read: {e}"))?;
        if ciphertext.len() as u64 > MAX_BYTES {
            return Err("friends cache exceeds host byte limit".to_owned());
        }
        // 1006FBEB0 only assigns the text when decrypt succeeds. Corrupt AES
        // length/padding leaves empty text; JSON syntax errors occur later.
        let plaintext = match decode_with_key(&ciphertext, &FRIENDS_KEY) {
            Ok(text) => text,
            Err(_) => return Ok(String::new()),
        };
        String::from_utf8(plaintext).map_err(|_| "friends cache is not UTF-8".to_owned())
    }

    pub(in super::super) fn write(&self, text: &str) -> Result<(), String> {
        if text.len() as u64 + 16 > MAX_BYTES {
            return Err("friends cache exceeds host byte limit".to_owned());
        }
        let ciphertext =
            encode_with_key(text.as_bytes(), &FRIENDS_KEY).map_err(|e| e.to_string())?;
        let root = self.path.parent().ok_or("friends cache root unavailable")?;
        fs::create_dir_all(root).map_err(|e| format!("friends cache directory: {e}"))?;
        let mut temporary = self.path.as_os_str().to_os_string();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        let mut file =
            fs::File::create(&temporary).map_err(|e| format!("friends cache open: {e}"))?;
        file.write_all(&ciphertext)
            .map_err(|e| format!("friends cache write: {e}"))?;
        file.flush()
            .map_err(|e| format!("friends cache flush: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("friends cache sync: {e}"))?;
        drop(file);
        // Native 1005057D0 ignores rename's return. Surface failure as a host
        // adaptation instead of publishing an unpersisted success.
        fs::rename(temporary, &self.path).map_err(|e| format!("friends cache replace: {e}"))
    }
}

#[cfg(test)]
mod tests;
