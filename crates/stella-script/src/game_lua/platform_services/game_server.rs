//! Native game-server transport plus the offline Frenemies completion facade.

use crate::*;
use aes::{
    Aes128,
    cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7},
};
use std::{collections::VecDeque, io::Read, time::Duration};

const DEFAULT_BASE_URL: &str = "https://stella-stage.appspot.com/api/v1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const ENCRYPTION_KEY_SUFFIX: &[u8; 8] = b"RAOzTXzh";
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const ZERO_IV: [u8; 16] = [0; 16];

type Aes128CbcEncryptor = cbc::Encryptor<Aes128>;

#[derive(Clone, Debug)]
struct RequestCompletion {
    message_id: i32,
    status: i32,
    body: Vec<u8>,
}

/// Cross-thread state retained by Purple's native GameServerConnection owner.
#[derive(Clone, Debug)]
pub(crate) struct GameServerRuntime {
    base_url: Arc<Mutex<String>>,
    completions: Arc<Mutex<VecDeque<RequestCompletion>>>,
}

impl Default for GameServerRuntime {
    fn default() -> Self {
        Self {
            base_url: Arc::new(Mutex::new(DEFAULT_BASE_URL.to_owned())),
            completions: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl GameServerRuntime {
    fn base_url(&self) -> String {
        self.base_url
            .lock()
            .expect("game-server base URL lock poisoned")
            .clone()
    }

    fn push_completion(&self, completion: RequestCompletion) {
        self.completions
            .lock()
            .expect("game-server completion queue lock poisoned")
            .push_back(completion);
    }

    fn pop_completion(&self) -> Option<RequestCompletion> {
        self.completions
            .lock()
            .expect("game-server completion queue lock poisoned")
            .pop_front()
    }

    #[cfg(test)]
    pub(crate) fn set_base_url_for_test(&self, base_url: impl Into<String>) {
        *self
            .base_url
            .lock()
            .expect("game-server base URL lock poisoned") = base_url.into();
    }
}

enum RequestBody {
    Get,
    Post(Vec<u8>),
}

fn request_message_id(args: &MultiValue, function: &str) -> LuaResult<i32> {
    // Generated adapters 0x1000D8310/0x1000D7F1C read lua_Number as float,
    // then use ARM FCVTZS to form the captured signed request id.
    Ok(native_fcvtzs_f32(
        native_required_number(args, 0, function)? as f32,
    ))
}

fn current_locale(lua: &Lua) -> String {
    game_environment(lua)
        .and_then(|environment| environment.get::<String>("g_currentLocale"))
        .unwrap_or_default()
}

fn serialize_payload(lua: &Lua, payload: mlua::Table) -> Option<Vec<u8>> {
    // sub_1000E2F6C converts LuaTable through util::JSON. The original catches
    // conversion exceptions in lua_postAsync, logs them, and does not enqueue
    // an HTTP task, so an unsupported/cyclic table is intentionally a no-op.
    let document = lua
        .from_value::<serde_json::Value>(Value::Table(payload))
        .ok()?;
    serde_json::to_vec(&document).ok()
}

fn base32_without_padding(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut bits = 0_u64;
    let mut bit_count = 0_u32;
    for &byte in bytes {
        bits = (bits << 8) | u64::from(byte);
        bit_count += 8;
        while bit_count >= 5 {
            bit_count -= 5;
            output.push(BASE32_ALPHABET[((bits >> bit_count) & 0x1f) as usize] as char);
        }
        bits &= (1_u64 << bit_count).wrapping_sub(1);
    }
    if bit_count != 0 {
        output.push(BASE32_ALPHABET[((bits << (5 - bit_count)) & 0x1f) as usize] as char);
    }
    output
}

fn encrypt_payload(seed: &str, plaintext: &[u8]) -> Option<Vec<u8>> {
    // lua_postAsync takes at most the first eight seed bytes, appends the
    // recovered literal suffix, and asks util::AES for inferred 128-bit key,
    // pad mode zero. The implementation is AES-CBC with a zero IV and PKCS#7,
    // followed by unpadded RFC-4648 base32 in a one-field JSON document.
    let seed = seed.as_bytes();
    let mut key = Vec::with_capacity(16);
    key.extend_from_slice(&seed[..seed.len().min(8)]);
    key.extend_from_slice(ENCRYPTION_KEY_SUFFIX);
    let key: [u8; 16] = key.try_into().ok()?;
    let encrypted = Aes128CbcEncryptor::new((&key).into(), (&ZERO_IV).into())
        .encrypt_padded_vec::<Pkcs7>(plaintext);
    serde_json::to_vec(&serde_json::json!({
        "data": base32_without_padding(&encrypted)
    }))
    .ok()
}

fn read_response(mut response: ureq::http::Response<ureq::Body>) -> Option<(i32, Vec<u8>)> {
    let status = i32::from(response.status().as_u16());
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .ok()?;
    Some((status, body))
}

fn perform_request(url: &str, locale: &str, body: RequestBody) -> Option<(i32, Vec<u8>)> {
    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .new_agent();
    let response = match body {
        RequestBody::Get => agent.get(url).header("Accept-Language", locale).call(),
        RequestBody::Post(body) => agent
            .post(url)
            .header("Accept-Language", locale)
            .header("Content-Type", "application/json")
            .send(body),
    }
    .ok()?;
    read_response(response)
}

fn enqueue_request(
    runtime: &GameServerRuntime,
    message_id: i32,
    route: String,
    locale: String,
    body: RequestBody,
) -> LuaResult<()> {
    let url = format!("{}{}", runtime.base_url(), route);
    let worker_runtime = runtime.clone();
    std::thread::Builder::new()
        .name("stella-game-server".to_owned())
        .spawn(move || {
            let (status, body) =
                perform_request(&url, &locale, body).unwrap_or_else(|| (-1, Vec::new()));
            worker_runtime.push_completion(RequestCompletion {
                message_id,
                status,
                body,
            });
        })
        .map_err(|_| runtime_error("Creating thread failed"))?;
    Ok(())
}

/// Publish the exact two native members registered by sub_1000D25D8.
pub(crate) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<GameServerRuntime> {
    let runtime = GameServerRuntime::default();
    let connection = lua.create_table()?;

    let get_runtime = runtime.clone();
    connection.set(
        "native_getAsync",
        lua.create_function(move |lua, args: MultiValue| {
            let message_id = request_message_id(&args, "GameServerConnection.native_getAsync")?;
            let route = native_required_string(&args, 1, "GameServerConnection.native_getAsync")?;
            enqueue_request(
                &get_runtime,
                message_id,
                route,
                current_locale(lua),
                RequestBody::Get,
            )
        })?,
    )?;

    let post_runtime = runtime.clone();
    connection.set(
        "native_postAsync",
        lua.create_function(move |lua, args: MultiValue| {
            let function = "GameServerConnection.native_postAsync";
            let message_id = request_message_id(&args, function)?;
            let route = native_required_string(&args, 1, function)?;
            let encrypt = native_required_boolean(&args, 2, function)?;
            let seed = native_required_string(&args, 3, function)?;
            let payload = native_required_table(&args, 4, function)?;
            let Some(mut body) = serialize_payload(lua, payload) else {
                return Ok(());
            };
            if encrypt {
                let Some(encrypted) = encrypt_payload(&seed, &body) else {
                    return Ok(());
                };
                body = encrypted;
            }
            enqueue_request(
                &post_runtime,
                message_id,
                route,
                current_locale(lua),
                RequestBody::Post(body),
            )
        })?,
    )?;

    globals.set("GameServerConnection", connection)?;
    Ok(runtime)
}

/// Load the constructor-owned common facade once the Rust host has created
/// its retained GameLua environment. Synthetic `/tmp` test hosts omit data;
/// real distributions execute the exact shipped persistent bytecode here.
pub(crate) fn load_shipped_facade(lua: &Lua, data_root: &Path) -> LuaResult<bool> {
    const SCRIPT: &str = "scripts_common/network/GameServerConnection.lua";
    if !data_root.join(SCRIPT).is_file() {
        return Ok(false);
    }
    execute_script_in(lua, data_root, SCRIPT, game_environment(lua)?)?;
    Ok(true)
}

fn response_table(lua: &Lua, status: i32, body: &[u8]) -> LuaResult<mlua::Table> {
    if status == 200
        && let Ok(document) = serde_json::from_slice::<serde_json::Value>(body)
        && let Ok(Value::Table(table)) = lua.to_value(&document)
    {
        return Ok(table);
    }
    // luaRequestDoneHandler catches JSON parse/conversion exceptions and
    // still calls completion with the initially constructed empty LuaTable.
    lua.create_table()
}

/// Deliver completed HttpRequestTask callbacks on the application thread.
pub(crate) fn dispatch_completions(lua: &Lua, runtime: &GameServerRuntime) -> LuaResult<()> {
    while let Some(completion) = runtime.pop_completion() {
        // The shipped chunk creates its public high-level facade in the
        // retained GameLua environment, but attaches these two native bridge
        // callbacks to `_G.GameServerConnection`. Purple's LuaObject retains
        // that original constructor table even after the local shadow exists.
        let connection: mlua::Table = lua.globals().raw_get("GameServerConnection")?;
        if completion.status == -1 {
            let callback: mlua::Function = connection.get("onAsyncRequestTimedOut")?;
            callback.call::<()>((completion.message_id, completion.status))?;
        } else {
            let callback: mlua::Function = connection.get("onAsyncRequestCompleted")?;
            let response = response_table(lua, completion.status, &completion.body)?;
            callback.call::<()>((completion.message_id, completion.status, response))?;
        }
    }
    Ok(())
}

/// Publish the callback surface consumed by the shipped challenge menus.
///
/// Purple contains a native `GameServerConnection` owner and loads
/// `scripts_common/network/GameServerConnection.lua`, but this exact 1.1.6
/// chunk terminates every request with `assert(false,
/// "GAMESERVER-DISABLED")`. A disconnected cross-platform rehost cannot
/// obtain the former service's replay response, so retain the script callback
/// contract and synthesize its level/session payload from the active local
/// challenge. Callbacks are deferred one update just like native HTTP
/// completion and all original menu/event routing remains in charge.
pub(crate) fn install_offline_facade(lua: &Lua) -> LuaResult<()> {
    lua.load(
        r##"
        do
            local connection = GameServerConnection or {}
            GameServerConnection = connection
            GameServerErrorCodes = GameServerErrorCodes or {
                NOT_ENOUGH_TOKENS = 409
            }

            if not rawget(connection, "__stellaOfflineFacade") then
                local offlineSession = 0

                local function defer(callback, ...)
                    if type(callback) ~= "function" then
                        return
                    end
                    local values = {...}
                    if type(callDelayed) == "function" then
                        callDelayed(0, function()
                            callback(unpack(values))
                        end)
                    else
                        callback(unpack(values))
                    end
                end

                local function currentChallenge()
                    if PlayerState and PlayerState.getCurrentChallenge then
                        return PlayerState:getCurrentChallenge()
                    end
                end

                local function copyTable(source)
                    local result = {}
                    if type(source) == "table" then
                        for key, value in pairs(source) do
                            result[key] = value
                        end
                    end
                    return result
                end

                local function currentLevelSeed(challenge)
                    local seed = challenge and (challenge.levelId or challenge.seed)
                    if seed == nil and type(getLevelMetadata) == "function" and levelName then
                        local metadata = getLevelMetadata(levelName)
                        local event = metadata and metadata.islandEvent
                        seed = event and event.seed
                        if seed == nil and event and event.levels and event.levels[1] then
                            seed = event.levels[1].variantSeed
                        end
                    end
                    if seed == nil and g_variantRandom then
                        seed = g_variantRandom.seedToSetOnLevelLoad or g_variantRandom.seed
                    end
                    return seed or "1"
                end

                local function replayPayload(competitionId)
                    local challenge = currentChallenge() or {
                        competitionId = competitionId,
                        players = {}
                    }
                    challenge.competitionId = challenge.competitionId or competitionId
                    local players = {}
                    local foundCurrentPlayer = false
                    for index, player in ipairs(challenge.players or {}) do
                        local copy = copyTable(player)
                        if copy.currentPlayer then
                            foundCurrentPlayer = true
                            offlineSession = offlineSession + 1
                            copy.gameSessionId = copy.gameSessionId or
                                (PlayerState and PlayerState:getGameSessionToken()) or
                                ("offline-session-" .. offlineSession)
                        end
                        players[index] = copy
                    end
                    if not foundCurrentPlayer then
                        offlineSession = offlineSession + 1
                        players[#players + 1] = {
                            currentPlayer = true,
                            nickname = "Me",
                            score = 0,
                            stars = 0,
                            gameSessionId = "offline-session-" .. offlineSession
                        }
                    end
                    local payload = {
                        competitionId = challenge.competitionId,
                        levelId = currentLevelSeed(challenge),
                        players = players
                    }
                    challenge.levelId = payload.levelId
                    challenge.players = players
                    if PlayerState and PlayerState.setCurrentChallenge then
                        PlayerState:setCurrentChallenge(challenge)
                    end
                    return payload
                end

                function connection.replayCompetitionLevel(competitionId, success)
                    defer(success, replayPayload(competitionId))
                end

                function connection.startCompetitionLevel(competitionId, success)
                    defer(success, replayPayload(competitionId))
                end

                function connection.completeCompetitionLevel(
                    sessionToken, competitionId, result, success
                )
                    local challenge = currentChallenge()
                    if challenge then
                        for _, player in ipairs(challenge.players or {}) do
                            if player.currentPlayer then
                                player.score = result and result.score or player.score or 0
                                player.stars = result and result.stars or player.stars or 0
                                player.gameSessionId = player.gameSessionId or sessionToken
                            end
                        end
                        if PlayerState and PlayerState.setCurrentChallenge then
                            PlayerState:setCurrentChallenge(challenge)
                        end
                    end
                    defer(success, challenge or {})
                end

                function connection.getPlayerCompetitions(success)
                    local challenge = currentChallenge()
                    defer(success, {
                        competitions = challenge and {challenge} or {}
                    })
                end

                function connection.joinCompetition(playerLevel, success)
                    local challenge = currentChallenge() or {
                        competitionId = "offline-competition",
                        players = {}
                    }
                    if PlayerState and PlayerState.setCurrentChallenge then
                        PlayerState:setCurrentChallenge(challenge)
                    end
                    defer(success, challenge)
                end

                function connection.closeCompetition(competitionId, success)
                    defer(success, { rewardsWon = {} })
                end

                for _, method in ipairs({
                    "getPlayerStatus", "refreshFeathers", "startLevel",
                    "completeLevel", "respinLevel", "closeLevel"
                }) do
                    connection[method] = function(...)
                        local arguments = {...}
                        for index = #arguments, 1, -1 do
                            if type(arguments[index]) == "function" then
                                defer(arguments[index], {})
                                return
                            end
                        end
                    end
                end

                connection.__stellaOfflineFacade = true
            end
        end
        "##,
    )
    .set_name("[stella-offline-game-server]")
    .set_environment(game_environment(lua)?)
    .exec()
}

#[cfg(test)]
mod tests {
    use super::{base32_without_padding, encrypt_payload};

    #[test]
    fn encryption_matches_recovered_aes_and_base32_pipeline() {
        assert_eq!(base32_without_padding(b"foo"), "MZXW6");
        assert_eq!(
            encrypt_payload("12345678tail", br#"{"a":"x","z":2}"#).unwrap(),
            br#"{"data":"ATS5BS2VEJJELN5ULPWIRRDPC4"}"#
        );
    }
}
