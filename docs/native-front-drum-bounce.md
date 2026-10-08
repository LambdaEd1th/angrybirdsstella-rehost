# Front drum bounce and unsolicited bird abilities

2026-10-09. A disposable copy of the player's `runtime/appdata` reproduced
the leftmost BirdRun event as `BirdRun_L09`, event seed `1191650966`, variant
seed `2667792133`. One fixed drag, `(-240,+24)` over 60 frames, releases the
bird at frame 2160. No subsequent input or automatic level completion is used.

At frame 2246 Dahlia first contacts `BLOCK_POPPYSHOUSE_BIGDRUM_DRUMSKIN_1_3`.
The original Lua reflects velocity from `(4.14449,2.42140)` to
`(3.19573,-4.79217)`. An additional contact with
`BLOCK_POPPYSHOUSE_BIGDRUM_BASE_3` in the same frame sets `hasCollided` and
creates `dahliaAbility`. Pressed, held and released are false at both callbacks.
The user confirmed that the original game does not activate itself in this
situation. That comparison is a user observation, not an original-device
runtime capture made by this audit.

## Official IDA MCP evidence and implementation boundary

New native analysis used only the Official IDA MCP Server with Purple 1.1.6
ARM64; existing native evidence was reused:

- `0x100062520`: BeginContact dispatches the eight-argument bird callback,
  then arms the single-bird collision timer. Reordering or suppressing this
  callback would diverge from the recovered original behavior.
- `0x100062510`: PreSolve returns; no native drum-specific contact immunity
  was identified there.
- `0x10086EA54`: SolveTOI admits ordinary dynamic/dynamic pairs only when
  at least one endpoint is a bullet. Constructor and velocity-setter evidence
  show that the ordinary bird does not acquire a bullet flag.
- `0x100861B54` and `0x10085E624`: the recovered float32 TOI and polygon-circle
  manifold calculations are retained for the earlier front-surface impact.

The change is therefore a narrow **rehost compatibility exception**, not a
claim that native SolveTOI contains this eligibility rule. A controllable
circle approaching the outward front of a box marked by the original Lua as
`isDrum && ignoreCollision` can now enter the existing continuous solver.
The local-space sweep must start outside the cover plus both fixture radii,
move toward its front, and produce an outward front manifold. This stops the
bird at the membrane before the discrete penetration reaches the backing
base. It leaves native bullet flags, ordinary pair eligibility, Lua collision
rules, callback arguments and the original ability handlers intact.

The scope is not blanket immunity around a drum: ordinary base/obstacle
contacts, side/underside approaches, unmarked boxes and non-bird circles keep
their existing contact path. Full physical equivalence for every native drum
trajectory remains an outstanding replication question. This behavioral fix
does not complete the paused long-term goal or establish a performance gain.

## Regressions and Poppy observations

The saved pre-fix diagnostic test executable fails the fixed BirdRun shot for
Dahlia, Willow, Luca and Stella because it reaches both the cover and base.
Poppy already passes that exact shallow trajectory; its different body shape
does not prove that other Poppy trajectories were safe. After the change all
five species contact only the cover and keep their unactivated ability and
upward velocity, with no input at contact. The isolated event fixture uses the
original BasicLevels variant generator and per-turn bird handler. It carries
only event seeds and unlocked bird choices, not the player's private save.

Functional TOI regressions exercise rotated covers and both creation orders,
and reject side/underside, unmarked and non-bird pairs while leaving ordinary
manifolds available. A control shot still contacts the shipped wooden platform
and activates Dahlia's normal collision ability without input.

Poppy's unchanged-expression report has not been reproduced in the private
player-save images: sling idle, sling pull and flight differ. Existing checks
cover six normal/costume selections through sling, flight, drum reflection,
ability hold and drill activation. A new missed-shot/next-bird regression also
retains the normal, angry and happy face transitions. These observations must
not be reported as a newly identified or fixed facial-animation root cause.

An existing optimized sub-epsilon edge test failed in both pre-fix and
post-fix executables on the sign of zero; the unoptimized test passed. A
diagnostic attempt with opaque fixture inputs did not resolve it and was
removed. The test and its strict signed-zero, face-branch and feature-ID
assertions remain unchanged. This pre-existing optimized-mode failure is
recorded separately and is not a full-suite pass or resolved native numerical
equivalence claim.

Detailed local logs, player-save images, native artifact hashes and validation
manifests are under `target/audits/bird-drum-shared-20261008`. Raw private saves
and traces are not committed. Normal player save hashes are checked separately.

Final unoptimized workspace verification passes 1,986 tests; the two existing
ignored long-running tests remain unchanged. Strict Clippy with `-D warnings`
passes macOS ARM64, Linux ARM64/x64, Windows MSVC ARM64/x64 and Emscripten,
including the macOS release configuration. Debug and diagnostic desktop
executables are rebuilt from the final source.

The repeated player-save shot now contacts only the cover at frame 2244;
through frame 2270 it retains upward velocity with no collision/ability flags
or further input. A separate Poppy shot enters ability aiming only after the
explicit hold at frame 2250, then states 1 and 2 after release. The ordinary
Chapter01_L18 window trace also enters `onSlingAiming` during a held mouse
press. The short native UI-automation drag delivered both edges in one frame
and is not counted as a successful visual flight validation. Normal save
verification covers all 14 files with no changed hashes.
