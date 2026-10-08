# Bird rotation and follow-up drum/face audit

2026-10-09. New executable analysis uses only the Official IDA MCP Server,
instance `ad5d1f278855`, against Purple 1.1.6 ARM64. The compact instruction
evidence and original executable hash are in
`.ida-mcp/native-bird-angular-damping.json`. The full replication goal is
active. This requested bug investigation is one verified step within it and
does not complete that goal.

## Confirmed discrepancy and repair

Every physics constructor initializes `b2BodyDef::angularDamping` to `1.0f`.
That intermediate value is not the final value for controllable objects.
After body/fixture creation, all four constructors conditionally overwrite
`b2Body+0xAC` with `0x40000000`, or `2.0f`:

| Constructor | Conditional branch | Final write |
| --- | --- | --- |
| `0x100034740` box | `0x100034CF8`, TBZ controllable bit zero | `0x100034D04` |
| `0x100034FB0` circle | `0x100035508`, CBZ controllable | `0x100035514` |
| `0x1000357A4` polygon | `0x100035D18`, CBZ controllable | `0x100035D24` |
| `0x1000364E0` line | `0x100036A54`, CBZ controllable | `0x100036A60` |

The Rust constructor previously retained `1.0` for all physics objects. Its
shared initialization now retains the native distinction: controllable `2.0`,
other physics bodies `1.0`, and non-physics records `0.0`. It preserves body
activation, mass, fixtures, bullet flags and Lua fields. No shipped asset or
script is edited. Ordinary objects keep the vehicle-settling behavior of the
earlier unit-damping correction.

The new functional regression creates a normal bird circle and a comparable
uncontrolled circle, activates the bird, assigns both angular velocity 3,
and executes a complete native fixed physics step. Before the repair it fails:
the bird velocity is `2.8999998569488525` instead of
`2.799999952316284`. After the repair, both angular velocity and the resulting
angle match the separate float32 native damping factors. The existing object
constructor contract also checks the controllable circle's final value.
The regression exercises activation and integration, not just a stored field.

## Drum and face evidence boundaries

Full pseudocode/instruction captures and geometry observations remain local
under `target/audits/native-drum-root-20261009`. The additional checks cover:

- `0x10086F3FC`, `0x10086E634`, `0x10086CE84`, `0x10086EA54`: discrete contact
  update, island solving and continuous candidate eligibility/order.
- `0x10086BAB0`, `0x10086373C`, `0x100065488`: live contact update, touching
  transition and the original filter. No drum-specific exclusion was found.
- `0x100062520`, `0x100062510`, `0x100040FA4`, `0x10003F930`, `0x10003F9CC`:
  BeginContact callback/timer order, no-op PreSolve and direct float32
  velocity/impulse/force writes. Launch does not mark ordinary birds as bullets.
- `0x10005E898`: scaled frame accumulation, `1/30f` physics updates and
  `World::Step(dt,10,10)`. This does not justify replacing the step with `1/60`.
- `0x10001396C`, `0x10040E798`, `0x1004111A4`, `0x100013A98`, `0x100013B9C`,
  `0x100012F18`, `0x100410A18`, `0x100411230`, `0x100095A4C`, `0x10006794C`:
  exact-time seek, forced target application, pause/resume, control start,
  update, sprite submission and flash-animation placement.

The reproduced drum cover and base are dynamic, non-bullet bodies at density
1. Their cover height is `0.16`, base height `0.49`, with about `0.038` front
protrusion. Ordinary native TOI eligibility skips this dynamic/dynamic pair.
The front-drum compatibility exception remains an exception; this audit does
not turn it into a recovered original eligibility rule. It must not be
silently removed while the fixed player reproduction still requires it.

The verified damping discrepancy is not established as the cause of either
reported symptom. Sling launch resets angular velocity to zero. Poppy's
existing checks and saved images already showed distinct sling, pull and
flight faces before this repair. No new expression-freeze root cause was
identified. These observations must not be reported as a newly proved facial
repair, complete drum equivalence, or complete original-game parity.

## Validation

Frozen compilation inputs: 1,002 files, aggregate SHA-256
`4aff467b27708d769a1086510cc945c347beab205247cd433e0962e77e102ce7`.
The workspace all-target/all-feature dev suite passes 1,987 tests with zero
failures and the same two existing ignored long-running audits. It includes
all five no-input BirdRun drum shots, the ordinary-obstacle ability control,
six Poppy normal/costume face/ability scenarios and missed-shot next-Poppy
progression. Strict Clippy passes macOS ARM64, Linux ARM64/x64, Windows MSVC
ARM64/x64 and Emscripten, plus the macOS release configuration. Source,
before/after logs and final check manifests remain in the local audit directory.
The optimized full-suite signed-zero limitation recorded in
`native-front-drum-bounce.md` remains unresolved and is not hidden or counted
as a complete optimized-suite pass.

The rebuilt diagnostic app also executes four fixed input scenarios through
the actual wgpu/Metal screenshot path, using private copies of the player's
save: Chapter01_L18 Poppy idle, sling pull and flight, then the leftmost
BirdRun_L09 Dahlia drum bounce. All four exit successfully after explicit Lua
error predicates validate the scene, animation action or contact state, and
require the level to remain incomplete. The drum case observes cover-only
contacts with no post-launch input, unsolicited ability or collision state.
The Poppy images show different pull and flight faces; they are local runtime
regression evidence, not a comparison against original native rendering.
All 14 normal-save file hashes remain unchanged. Debug, diagnostic and release
app builds succeed; no build or evidence is uploaded or published.

Two initial verification observers incorrectly read the cleared
`currentBirdName` after launch and the transient `currentState` after animation
update. The corrected observers use `flyingBird` and the retained
`currentStateName`; the rejected runs are preserved separately. No production
behavior, Lua predicate, or expected state was weakened to make those runs pass.
