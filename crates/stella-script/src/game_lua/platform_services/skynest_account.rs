//! Rovio Account/Identity Level 2 boundary for the retired Skynest backend.

use crate::*;
use mlua::{IntoLuaMulti, RegistryKey};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

enum Completion {
    LoginUnavailable,
    ValidateNickname {
        callback: RegistryKey,
        is_valid: bool,
    },
}

/// Identity-provider state and retained application-thread completions.
#[derive(Clone)]
pub(crate) struct SkynestAccountRuntime {
    state: Arc<Mutex<OfflineState>>,
    completions: Rc<RefCell<VecDeque<Completion>>>,
}

impl SkynestAccountRuntime {
    fn new(state: Arc<Mutex<OfflineState>>) -> Self {
        Self {
            state,
            completions: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    fn begin_login_unavailable(&self) -> LuaResult<()> {
        self.state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?
            .login_in_progress = true;
        self.completions
            .borrow_mut()
            .push_back(Completion::LoginUnavailable);
        Ok(())
    }

    fn queue_nickname_validation(&self, callback: RegistryKey, is_valid: bool) {
        self.completions
            .borrow_mut()
            .push_back(Completion::ValidateNickname { callback, is_valid });
    }

    fn take_pending(&self) -> VecDeque<Completion> {
        std::mem::take(&mut *self.completions.borrow_mut())
    }
}

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
) -> LuaResult<SkynestAccountRuntime> {
    let runtime = SkynestAccountRuntime::new(Arc::clone(&state));
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
    let login_runtime = runtime.clone();
    account.set(
        "native_login",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_1000A8C1C consumes three strict
            // booleans and ignores the tail.
            native_required_boolean(&args, 0, "SkynestAccount.native_login")?;
            native_required_boolean(&args, 1, "SkynestAccount.native_login")?;
            native_required_boolean(&args, 2, "SkynestAccount.native_login")?;
            login_runtime.begin_login_unavailable()
        })?,
    )?;
    account.set(
        "native_logout",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    let social_login_runtime = runtime.clone();
    account.set(
        "native_loginWithSocialNetwork",
        lua.create_function(move |_, _: MultiValue| {
            social_login_runtime.begin_login_unavailable()
        })?,
    )?;
    account.set(
        "native_unRegister",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;

    account.set(
        "native_hasNickname",
        // Despite its exported name, sub_1000A3D68 returns the identity
        // provider's `profileNickname.empty()`. A signed-out provider has an
        // empty profile regardless of similarly named Skynest Storage keys.
        lua.create_function(|_, _: MultiValue| Ok(true))?,
    )?;
    let validation_runtime = runtime.clone();
    account.set(
        "native_validateNickname",
        lua.create_function(move |lua, args: MultiValue| {
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
            validation_runtime
                .queue_nickname_validation(lua.create_registry_value(callback)?, is_valid);
            Ok(())
        })?,
    )?;

    globals.set("SkynestAccount", account)?;
    Ok(runtime)
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

/// Deliver retained identity-provider completions at the application frame head.
pub(crate) fn dispatch_completions(lua: &Lua, runtime: &SkynestAccountRuntime) -> LuaResult<()> {
    // A callback may submit another provider request. Keep it outside this
    // snapshot so it cannot complete recursively on the same stack.
    for completion in runtime.take_pending() {
        match completion {
            Completion::LoginUnavailable => notify_login_unavailable(lua, &runtime.state)?,
            Completion::ValidateNickname { callback, is_valid } => {
                call_retained(lua, callback, (true, is_valid))?;
            }
        }
    }
    Ok(())
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

fn call_retained(lua: &Lua, callback: RegistryKey, args: impl IntoLuaMulti) -> LuaResult<()> {
    let function = lua.registry_value::<mlua::Function>(&callback)?;
    let result = function.call::<()>(args);
    lua.remove_registry_value(callback)?;
    result
}
