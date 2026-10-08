# Desktop window closure and final persistence

The first beta5 desktop packages could stay open after the user pressed the
window close button. The host queued `CloseRequested` behind the frame-latched
Lua `g_safeToQuit` byte. The original iOS game can leave that byte false,
including on the main menu, so the host could wait indefinitely. Focus loss
also stopped the display-link updates that might otherwise refresh the byte.

Official IDA MCP evidence is recorded in
`../.ida-mcp/desktop-close-termination.json`. The same published macOS beta5
executable reproduced the failure in an isolated QA application: its close
button was pressed through CUA and the window and process remained alive.

`-[AppController applicationWillTerminate:]` at `0x1004047A8` clears
`m_allowUpdate` and calls `stopUpdate` at `0x1004047F0` on the normal target
branch. It does not call the safe-to-quit getter. `stopUpdate`, starting at
`0x100404E24`, clears platform touches, invalidates the display link, delivers
the inactive callback through App virtual slot 19, then stops audio through
slot 22. The existing GameLua inactive callback invokes the shipped
`gamePaused`, including its final script-owned save pass.

The getter `sub_100062518`, reached through `sub_100028FB0`, still exposes
GameLua `+0x6AC`. Its existence does not prove that the iOS termination callback
waits for it. The earlier desktop pending-close interpretation was incorrect
and its IDA annotation has been corrected. The separate script exit member
`sub_1004016E4` sets the native exit flag and exit code; that path is unchanged.

The corrected desktop host calls winit's `exit` on `CloseRequested` and lets
the normal `exiting` callback perform final persistence and audio shutdown.
The callback body is shared with focused tests as `terminate_window`; errors
remain in `WindowErrors` and reach the launcher after the event loop returns.
There is no force-kill path in the application. The Lua truth-value latch and
its original native-behavior regressions are retained.

Three tests of the incorrect deferred-close policy were replaced by shipped
script regressions for active closure, closure after focus loss and closure
with an account overlay. They use isolated AppData and confirm that pending
rewards are cleared in the final on-disk save without double-granting coins.
A further regression confirms that a final `gamePaused` failure is returned
to the launcher instead of silently reporting success.

Local validation on the corrected source:

| Check | Result |
| --- | --- |
| Shipped-script lifecycle regressions | 9 passed |
| Full workspace, all targets/features | 1976 passed; the same 2 tests ignored |
| Release desktop tests | 345 passed |
| Debug workspace and release app strict Clippy | Passed with `-D warnings` |
| External-resource packaging tests | 11 passed |
| Formatting and whitespace | Passed |
| Actual macOS release close button, fresh isolated saves | Normal exit code 0; settings/highscores/BI saved |
| Actual macOS release close button after loading existing isolated saves | Normal exit code 0; saves retained |
| Normal player AppData | All 14 existing file hashes unchanged |

The corrected-source manifest SHA-256 is
`060c902d0cba0e939081f7c9d81aa5add816ed72e71658f0dfebea8c32ea8b64`.
Detailed logs, the old published binary reproduction and the republished
commit/package verification are recorded in
`target/audits/close-window-beta5-20261008`. Screenshot observation may relaunch
a closed private QA app; only the monitored process exit status is used to
judge closure. No normal player directory is used by these probes.
