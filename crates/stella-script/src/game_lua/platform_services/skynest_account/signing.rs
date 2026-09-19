//! Native 100685E38 access signature/salt pair, with explicit host key input.
//! See docs/native-account-signature-generation.md. Never derive Debug here.

use crate::game_lua::platform::{generate_uuid_v4, sha1_digest, upper_hex};
use std::{fmt, sync::Arc};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum ClientSigning {
    Literal { signature: String, salt: String },
    Generated { key: Arc<[u8]> },
}

impl Default for ClientSigning {
    fn default() -> Self {
        Self::literal(String::new(), String::new())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SignatureError;

impl fmt::Display for SignatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("identity client salt generation failed")
    }
}

pub(super) struct SignedAccess {
    pub(super) signature: String,
    pub(super) salt: String,
}

impl ClientSigning {
    pub(super) fn literal(signature: String, salt: String) -> Self {
        Self::Literal { signature, salt }
    }

    pub(super) fn generated(key: impl AsRef<[u8]>) -> Self {
        Self::Generated {
            key: key.as_ref().into(),
        }
    }

    pub(super) fn credentials(&self, client_id: &str) -> Result<SignedAccess, SignatureError> {
        self.credentials_with_uuid(client_id, || generate_uuid_v4().map_err(|_| SignatureError))
    }

    fn credentials_with_uuid(
        &self,
        client_id: &str,
        uuid: impl FnOnce() -> Result<String, SignatureError>,
    ) -> Result<SignedAccess, SignatureError> {
        match self {
            Self::Literal { signature, salt } => Ok(SignedAccess {
                signature: signature.clone(),
                salt: salt.clone(),
            }),
            Self::Generated { key } => {
                // 100686148 hashes the complete uppercase, hyphenated UUID
                // text. 10068623C decodes that hex salt back into these bytes.
                let salt = sha1_digest(uuid()?.as_bytes());
                let derived = derive_key(key, &salt);
                Ok(SignedAccess {
                    signature: upper_hex(&hmac_sha1(&derived, client_id.as_bytes())),
                    salt: upper_hex(&salt),
                })
            }
        }
    }
}

fn derive_key(key: &[u8], salt: &[u8; 20]) -> [u8; 20] {
    let mut input = Vec::with_capacity(key.len() + salt.len());
    input.extend_from_slice(key);
    input.extend_from_slice(salt);
    let mut digest = sha1_digest(&input);
    for _ in 1..32 {
        digest = sha1_digest(&digest);
    }
    digest
}

fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    let mut key_block = [0; 64];
    if key.len() > key_block.len() {
        key_block[..20].copy_from_slice(&sha1_digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + message.len());
    inner.extend(key_block.iter().map(|byte| byte ^ 0x36));
    inner.extend_from_slice(message);
    let inner_digest = sha1_digest(&inner);
    let mut outer = [0; 84];
    for (slot, byte) in outer[..64].iter_mut().zip(key_block) {
        *slot = byte ^ 0x5c;
    }
    outer[64..].copy_from_slice(&inner_digest);
    sha1_digest(&outer)
}

#[cfg(test)]
mod tests;
