//! Offline completion of Stella's disabled Frenemies game-server facade.

use crate::*;

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
