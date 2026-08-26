//! Rovio Account/Identity Level 2 boundary for the retired Skynest backend.

use crate::*;

pub(super) struct OfflineState {
    pub(super) keys: BTreeMap<String, String>,
    login_in_progress: bool,
}

impl Default for OfflineState {
    fn default() -> Self {
        Self {
            keys: BTreeMap::new(),
            // SkynestAccountService's constructor finishes by calling
            // sub_1000A4AB0, which sets byte +0x41 before starting the
            // provider's automatic login. The Lua facade is installed later,
            // so the offline completion is delivered after service
            // announcement instead of being lost during construction.
            login_in_progress: true,
        }
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    state: Arc<Mutex<OfflineState>>,
) -> LuaResult<()> {
    let account = lua.create_table()?;
    account.set(
        "native_getServiceName",
        // ICloudService vtable slot +0x18 at sub_1000A7D28.
        lua.create_function(|_, _: MultiValue| Ok("identityLevel2"))?,
    )?;
    account.set(
        "native_isLoggedIn",
        // IdentityLevel2 state 2 is the signed-out state. The desktop host
        // has no provider capable of leaving it.
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;
    let progress_state = Arc::clone(&state);
    account.set(
        "native_isLoginInProgress",
        lua.create_function(move |_, _: MultiValue| {
            Ok(progress_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .login_in_progress)
        })?,
    )?;
    account.set(
        "native_getAccountDetailsUrl",
        lua.create_function(|_, _: MultiValue| Ok("https://account.rovio.com"))?,
    )?;
    let login_state = Arc::clone(&state);
    account.set(
        "native_login",
        lua.create_function(move |lua, args: MultiValue| {
            // Generated adapter sub_1000A8C1C consumes three strict
            // booleans and ignores the tail.
            native_required_boolean(&args, 0, "SkynestAccount.native_login")?;
            native_required_boolean(&args, 1, "SkynestAccount.native_login")?;
            native_required_boolean(&args, 2, "SkynestAccount.native_login")?;
            begin_and_complete_login_unavailable(lua, &login_state)
        })?,
    )?;
    account.set(
        "native_logout",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    let social_login_state = Arc::clone(&state);
    account.set(
        "native_loginWithSocialNetwork",
        lua.create_function(move |lua, _: MultiValue| {
            begin_and_complete_login_unavailable(lua, &social_login_state)
        })?,
    )?;
    account.set(
        "native_unRegister",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;

    let nickname_state = Arc::clone(&state);
    account.set(
        "native_hasNickname",
        lua.create_function(move |_, _: MultiValue| {
            // Despite its exported name, sub_1000A3D68 returns
            // `profileNickname.empty()`: true before a nickname exists and
            // false after one is stored. Preserve that observable inversion.
            Ok(!nickname_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .keys
                .contains_key("nickname"))
        })?,
    )?;
    account.set(
        "native_validateNickname",
        lua.create_function(|_, args: MultiValue| {
            // Adapter sub_1000A8998 reads a string and LuaFunction. Purple's
            // success completion calls callback(true, isValid); its transport
            // failure completion calls callback(false). Preserve the success
            // shape while providing a deterministic local validator now that
            // the remote identity service is gone.
            let nickname =
                native_required_string(&args, 0, "SkynestAccount.native_validateNickname")?;
            let callback =
                native_required_function(&args, 1, "SkynestAccount.native_validateNickname")?;
            let trimmed = nickname.trim();
            let is_valid = !trimmed.is_empty() && trimmed.chars().count() <= 32;
            callback.call::<()>((true, is_valid))
        })?,
    )?;

    globals.set("SkynestAccount", account)?;
    Ok(())
}

fn native_required_function(
    args: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<mlua::Function> {
    match args.iter().nth(index) {
        Some(Value::Function(callback)) => Ok(callback.clone()),
        _ => Err(runtime_error(format!(
            "bad argument #{} to '{function}' (function expected)",
            index + 1
        ))),
    }
}

pub(super) fn complete_initial_login(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if let Value::Table(facade) = environment.get::<Value>("SkynestAccount")? {
        let completed = facade.get::<Value>("initialAutologinDone")?;
        if !matches!(completed, Value::Nil | Value::Boolean(false)) {
            return Ok(());
        }
    }
    let Value::Table(account) = lua.globals().get::<Value>("SkynestAccount")? else {
        return Ok(());
    };
    let Value::Function(login) = account.get::<Value>("native_login")? else {
        return Ok(());
    };
    // sub_1000A3E04 routes (false, false, false) to sub_1000A4AB0, the same
    // automatic-login branch called by the native service constructor. At
    // this point SkynestAccount.lua has installed onLoginFailure, so the
    // retired backend can complete without leaving initialLoadingScreen
    // waiting forever.
    login.call::<()>((false, false, false))
}

fn begin_and_complete_login_unavailable(
    lua: &Lua,
    state: &Arc<Mutex<OfflineState>>,
) -> LuaResult<()> {
    {
        let mut state = state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
        state.login_in_progress = true;
    }
    notify_login_unavailable(lua, state)
}

fn notify_login_unavailable(lua: &Lua, state: &Arc<Mutex<OfflineState>>) -> LuaResult<()> {
    // Both native success and failure completions clear byte +0x41 before
    // entering the corresponding Lua callback (sub_1000A3F64/sub_1000A4578).
    state
        .lock()
        .map_err(|_| runtime_error("Skynest state lock poisoned"))?
        .login_in_progress = false;
    // SkynestAccount.lua keeps its facade in the game environment but adds
    // the native completions to the original `_G.SkynestAccount` table.
    let account = match lua.globals().get::<Value>("SkynestAccount")? {
        Value::Table(account) => account,
        _ => return Ok(()),
    };
    let Value::Function(on_failure) = account.get::<Value>("onLoginFailure")? else {
        return Ok(());
    };
    // Account manager sub_1000A3BA0 maps backend error 5 to ERROR_OTHER;
    // sub_1000A4578 forwards that code and the provider message verbatim.
    on_failure.call::<()>((
        "ERROR_OTHER",
        "Rovio Account is unavailable on this offline host",
    ))
}
