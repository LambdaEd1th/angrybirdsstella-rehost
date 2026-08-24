//! Legacy device-registration state and unlock checksum member.

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use crate::{native_fcvtzs_f32, native_required_number, native_required_string, runtime_error};

use super::sha1::sha1_upper_hex;

pub(super) fn install_identity(lua: &Lua, globals: &Table) -> LuaResult<()> {
    globals.set(
        "getDeviceID",
        lua.create_function(|_, _: MultiValue| Ok(""))?,
    )?;
    globals.set(
        "verifyDeviceID",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "verifyDeviceID")?;
            Ok(false)
        })?,
    )?;
    globals.set(
        "checkRegistrationResult",
        lua.create_function(|_, _: MultiValue| {
            // sub_100031104 initializes bytes +0x98/+0x99 to {0, 1};
            // sub_10003112C pushes +0x99 first and +0x98 second.
            Ok((true, false))
        })?,
    )?;
    Ok(())
}

pub(super) fn install_checksum(lua: &Lua, globals: &Table) -> LuaResult<()> {
    globals.set(
        "getUnlockRequestChecksum",
        lua.create_function(|_, args: MultiValue| {
            // sub_10005966C reads -2, -3 and -1 from the current Lua top:
            // second string, first string, then a float32 salt selector.
            let count = args.len();
            let first =
                native_required_string(&args, count.saturating_sub(3), "getUnlockRequestChecksum")?;
            let second =
                native_required_string(&args, count.saturating_sub(2), "getUnlockRequestChecksum")?;
            let selector = native_fcvtzs_f32(native_required_number(
                &args,
                count.saturating_sub(1),
                "getUnlockRequestChecksum",
            )? as f32);
            let salt = ["ThinkOfTheChildren", "ThinkOfTheChildren2"]
                .get(selector as usize)
                .ok_or_else(|| {
                    runtime_error(format!(
                        "getUnlockRequestChecksum salt selector {selector} out of bounds"
                    ))
                })?;
            let mut input = String::with_capacity(first.len() + second.len() + salt.len());
            input.push_str(&second);
            input.push_str(salt);
            input.push_str(&first);
            Ok(sha1_upper_hex(input.as_bytes()))
        })?,
    )?;
    Ok(())
}
