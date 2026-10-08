# Purchased Poppy animation and BirdRun regression

2026-10-09. Original executable/resource verification uses only the Official
IDA MCP Server, instance `ad5d1f278855`, against Purple 1.1.6 ARM64. Reused
recovered Lua evidence and compact native/resource captures are recorded in
`.ida-mcp/native-purchased-poppy-animation.json`. The full replica goal remains
active; this audit does not establish complete native equivalence.

## Native behavior retained

The player's final corrected comparison says that Poppy bought from the top
bar in the original BirdRun level also keeps its face during sling pull and
flight. That observation supersedes the earlier answer saying it changed.
No animation-map migration or synthetic fallback was implemented.

Official IDA verification of `native_loadTextFileToLuaTable` at `0x100051810`
and its byte pipeline at `0x1000512D8` establishes the resource parsing path.
The original encrypted `data/config/telepod_configuration.dat` was read and
decoded within the official IDA callback. Its single 55,621-byte JSON entry
is version 35 and exactly matches the extracted resource and the private
copy of the player's character cache. Plaintext SHA-256:
`2b6afc6308d6892173e536491d059fd9edd90b6624fe330d3b50f405905c58ee`.

Poppy level 1 contains the older `flyUp`, `flyLevel`, `flyDown` and backward
bindings, but no `aimSling` or `flying`. The recovered scripts explain the
visible result without an engine defect:

1. `ExtraBirdBar.useExtraBird` consumes the in-game price and calls
   `Telepods.giveBird` with the character configuration.
2. `Telepods.giveBird` creates an extra Poppy and passes the configuration's
   `animation.sprites` to `BirdAnimation.setAnimationSprites`.
3. That setter replaces the entire mapping. It does not merge the base bird
   definition or supply missing fields.
4. Sling aiming and flight request the absent `aimSling` and `flying` values.
   `AnimationPriorityStateMachine.setAnimation(nil)` leaves the existing
   action unchanged. Flight still pauses and explicitly seeks that action
   using the velocity angle.

Ordinary queue birds use the base Poppy definition, which does contain both
bindings. Selecting a costume changes its skin; buying an extra bird follows
the separate mapping replacement above. Existing ordinary/costume regressions
continue to require different sling and flight faces. The purchased-bird
regression requires `Poppy_Idle` in both phases. Manual Poppy skill states use
explicit `Poppy_Dizzy` and `Poppy_Power` actions independently of these missing
fields, and must still animate and activate normally.

There is no recovered live original remote configuration in this audit.
The legacy configuration is preserved as evidence, not silently modernized.

## Integration coverage

The new isolated regression enters `BirdRun_L09` through `BasicLevels.start`,
with event seed `1191650966` and variant seed `2667792133`. Its synthetic
profile unlocks/activates Poppy and the scrapbook before HUD creation. It
opens the native extra-bird bar and clicks its Poppy purchase control rather
than calling `giveBird` directly. It checks a new extra bird, price 40,
purchase count, native configuration version and missing fields, the actual
animation action and phase, and a bound face sprite submitted to rendering.

One fixed shot then reaches only the front drum cover with no post-launch
input. It must reflect upward without acquiring collision/ground/disabled
state or activating a skill. An explicit hold must show the Dizzy face, and
release must reach drill state 2 and create its sensor. The scenario stops
before level completion or failure. The front-cover compatibility exception
and its native-evidence limitation remain as documented in
`native-front-drum-bounce.md`; this test does not promote that exception into
a recovered native TOI rule.

Private copies of `runtime/appdata` also exercise the actual leftmost
BirdRun event, top-bar purchase, fixed shot and manual skill through the
diagnostic app's wgpu/Metal path. Observational captures cover idle, pull,
flight, bounce and drill; additional captures assert purchase identity,
unmodified mapping, cover-only contact, absent contact input, reflection,
no unsolicited skill, and explicit skill actions. Results and snapshots are
kept under `target/audits/native-drum-root-20261009/purchased-poppy`.

Earlier rejected probes are preserved: one clicked the QR control instead of
the bar arrow; an initial synthetic test omitted active-skin setup, so its
Poppy purchase control did not exist. Those failures are fixture/input
errors, not evidence of a production face fix. The earlier window experiment
remains inconclusive; four normal-save files changed during that experiment
and their updates were retained. This purchase audit starts from the updated
save and verifies its own 14-file baseline separately.

## Validation results

The final workspace dev suite passes 1,988 tests across 12 suites with zero
failures and the same two existing ignored long audits. Host macOS ARM64
workspace Clippy passes with all targets/features and `-D warnings`; formatting
and diff checks pass. The earlier six-platform Clippy results apply to the
unchanged production implementation, not a newly rerun cross-platform suite.
The previously documented optimized full-suite signed-zero failure remains
open and is not hidden or counted as a pass.

Eight actual purchased-bird captures succeed, including three with runtime
error predicates for bounce, explicit hold and explicit release. All 14
normal-save file hashes match this purchase audit's baseline. Compilation
inputs (1,002 files) remain unchanged throughout the final checks, aggregate
SHA-256 `7c61aa68aaa37053f9b958ec943414357b3270a5831f724581f85e62b55bf6e4`.
The diagnostic binary is unchanged from the preceding damping repair,
SHA-256 `64a5848762c232e006d2bd22128fa2a580d3436254c1bd1cb0c58366fbb6432a`.
The additions here are test coverage, native evidence and documentation;
there is no production animation or configuration change. Work is committed
only to local Git, without upload, push or release.
