//! Original app-data encryption recovered from the ARM64 executable.

use aes::{
    Aes256,
    cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7},
};

use crate::AssetError;

/// Seven-Zip signature emitted after decrypting a wrapped resource.
pub const SEVEN_Z_SIGNATURE: [u8; 6] = [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c];

/// Resource key copied by `sub_1000E3A14` in Purple 1.1.6.
pub const RESOURCE_KEY: &[u8; 32] = b"4FzZOae60yAmxTClzdgfcr4BAbPIgj7X";

/// Persistent Lua-file key copied by `sub_1000E3B38` in Purple 1.1.6.
pub const PERSISTENT_LUA_KEY: &[u8; 32] = b"Bll3qkcy5fKrNVxZqtkFH19Ojn2sdJFu";

/// Alternate text-file key embedded at `0x1009AEF30` and selected by the
/// third `loadTextFileToString` argument.
pub const TEXT_FILE_KEY: &[u8; 32] = b"0xMizJJUh7BbwmYhqxpJ038x8YGvk6aU";

const ZERO_IV: [u8; 16] = [0; 16];

type ResourceDecryptor = cbc::Decryptor<Aes256>;
type ResourceEncryptor = cbc::Encryptor<Aes256>;

/// Decrypt bytes with Purple's ordinary bundle-resource key.
///
/// Unlike [`decrypt_resource`], this mirrors the lower-level AES helper used
/// by GameLua's text-file APIs and therefore does not require a 7z signature.
pub fn decrypt_resource_data(input: &[u8]) -> Result<Vec<u8>, AssetError> {
    if input.is_empty() || !input.len().is_multiple_of(16) {
        return Err(AssetError::InvalidCiphertextLength(input.len()));
    }

    ResourceDecryptor::new(RESOURCE_KEY.into(), (&ZERO_IV).into())
        .decrypt_padded_vec::<Pkcs7>(input)
        .map_err(|_| AssetError::InvalidPadding)
}

pub fn decrypt_resource(input: &[u8]) -> Result<Vec<u8>, AssetError> {
    let output = decrypt_resource_data(input)?;

    if !output.starts_with(&SEVEN_Z_SIGNATURE) {
        return Err(AssetError::NotSevenZip);
    }
    Ok(output)
}

pub fn encrypt_persistent_lua(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }
    ResourceEncryptor::new(PERSISTENT_LUA_KEY.into(), (&ZERO_IV).into())
        .encrypt_padded_vec::<Pkcs7>(input)
}

pub fn decrypt_persistent_lua(input: &[u8]) -> Result<Vec<u8>, AssetError> {
    if input.is_empty() || !input.len().is_multiple_of(16) {
        return Err(AssetError::InvalidCiphertextLength(input.len()));
    }
    ResourceDecryptor::new(PERSISTENT_LUA_KEY.into(), (&ZERO_IV).into())
        .decrypt_padded_vec::<Pkcs7>(input)
        .map_err(|_| AssetError::InvalidPadding)
}

pub fn decrypt_text_file(input: &[u8]) -> Result<Vec<u8>, AssetError> {
    if input.is_empty() || !input.len().is_multiple_of(16) {
        return Err(AssetError::InvalidCiphertextLength(input.len()));
    }
    ResourceDecryptor::new(TEXT_FILE_KEY.into(), (&ZERO_IV).into())
        .decrypt_padded_vec::<Pkcs7>(input)
        .map_err(|_| AssetError::InvalidPadding)
}

#[cfg(test)]
mod tests {
    use aes::cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};

    use super::*;

    #[test]
    fn decrypts_the_recovered_mode() {
        let mut plaintext = SEVEN_Z_SIGNATURE.to_vec();
        plaintext.extend_from_slice(b"test archive payload");
        let encrypted = cbc::Encryptor::<Aes256>::new(RESOURCE_KEY.into(), (&ZERO_IV).into())
            .encrypt_padded_vec::<Pkcs7>(&plaintext);
        assert_eq!(decrypt_resource(&encrypted).unwrap(), plaintext);
    }

    #[test]
    fn lower_level_resource_decryptor_accepts_non_archive_text() {
        let plaintext = b"return { marker = 116 }";
        let encrypted = cbc::Encryptor::<Aes256>::new(RESOURCE_KEY.into(), (&ZERO_IV).into())
            .encrypt_padded_vec::<Pkcs7>(plaintext);
        assert_eq!(decrypt_resource_data(&encrypted).unwrap(), plaintext);
        assert!(matches!(
            decrypt_resource(&encrypted),
            Err(AssetError::NotSevenZip)
        ));
    }

    #[test]
    fn persistent_lua_container_round_trips_with_recovered_key() {
        let plaintext = b"settings = {\n    sound = true,\n}\n";
        let encrypted = encrypt_persistent_lua(plaintext);
        assert!(!encrypted.starts_with(plaintext));
        assert_eq!(decrypt_persistent_lua(&encrypted).unwrap(), plaintext);
    }

    #[test]
    fn decrypts_the_recovered_alternate_text_file_key() {
        let plaintext = b"plain text or an optional 7z payload";
        let encrypted = cbc::Encryptor::<Aes256>::new(TEXT_FILE_KEY.into(), (&ZERO_IV).into())
            .encrypt_padded_vec::<Pkcs7>(plaintext);
        assert_eq!(decrypt_text_file(&encrypted).unwrap(), plaintext);
    }
}
