# IDA + Hopper findings: Purple 1.1.6 ARM64

IDA input database: `Purple.i64`, image base `0x100000000`. The same ARM64
image is open independently as `Purple` in Hopper. The native contracts added
below were checked in both decompilers; addresses use the shared image base.

## Resource loading

`sub_100057E14` is the Lua-facing `GameLua::loadLuaFileFromAppDataToObject`
implementation. Its optional arguments select the app-data file source,
decryption and archive decompression.

The relevant chain is:

```text
sub_100057E14
  -> sub_100061E84              resource decrypt wrapper
     -> sub_1000E3A14           returns the 32-byte resource key
     -> util::AES::Impl         key length inferred from 32 octets
     -> AES block routine       CBC, zero initialization vector
     -> PKCS#7 length removal
  -> sub_100061C10              archive decompression wrapper
     -> KA3D stream/archive classes
```

The key constructor at `sub_1000E3A14` copies the exact 32 bytes used by
`stella-assets::crypto::RESOURCE_KEY`. It is kept in one place in source so it
can later be supplied externally for builds that should not contain it.

`util::AES::Impl::decrypt` is at `0x1005584F0`. With pad mode `0`, it requires
16-byte aligned input and removes the last byte count after decryption. The
block loop at `0x10055541C` decrypts each block and XORs it with the previous
ciphertext block; the first block is XORed with a zero buffer. This establishes
AES-CBC with an all-zero IV.

Decrypted samples start with the standard 7z signature. Archive payloads were
validated as:

- `.lua`: Lua 5.1 bytecode (`1B 4C 75 61 51`).
- animation `.json`: UTF-8 JSON.
- configuration `.dat`: usually an archive containing a `.json` file.

## AppData Lua persistence

The registration thunks and implementations were cross-checked in both
decompilers. `saveLuaFile` (`sub_10004B394`) consumes file name, global table
name and a persistence boolean; `savePersistentLuaFile` (`sub_10004B6D0`)
always uses the persistent path. `loadTableFromFile` (`sub_10004B880`) parses a
saved table and publishes it under the requested global name.

The native text serializer is `sub_10052A8FC`, with scalar/table formatting in
`sub_10052A490` and byte-string escaping in `sub_10052A020`. It emits executable
Lua assignments, uses bare identifiers only when the name is valid and not a
Lua keyword, uses bracketed keys inside nested tables, writes sequential
numeric keys as array entries, and excludes `_G` and `this`. Booleans, numbers,
strings and recursively nested tables are the supported value set. The Rust
serializer now follows that grammar instead of writing JSON under a mismatched
extension; its reader still accepts the earlier JSON saves for migration.

For persistent mode, `sub_1000E3B38` constructs the separate 32-byte Lua-save
key; Hopper independently shows `sub_100557F1C` dispatching to
`util::AES::Impl::encrypt` and `sub_100557F40` dispatching to the matching
decryptor. The same AES-256-CBC, zero-IV and PKCS#7 behavior recovered from the
resource wrapper is now used for persistent Lua files. Loaders detect this
container before parsing, while retaining plain-Lua and legacy-JSON fallback.

`loadLevelFromAppData` (`sub_100047234`) calls the common level loader with the
AppData selector set. `saveLevel` (`sub_10004730C`) and the corresponding load
path append `.lua`. `fileExistsInAppData` (`sub_10005A290`) delegates only to
the platform AppData existence check. The rehost now keeps these roots separate
from bundle data, so bundle-only files no longer produce false AppData hits and
save/load level tables round-trip through the same path and representation.
The common loader reads the two force multipliers with direct
`lua_isnumber`/`lua_tonumber` calls, so both level fields and their
`worldAttributes` defaults accept Lua 5.1 numeric strings, including the
hexadecimal fallback. The shared Rust publication path now preserves that
coercion for both selectors.

The registration table at `sub_10002C274` pairs `saveLuaFile` with adapter
`sub_100084E54`, `savePersistentLuaFile` and `loadTableFromFile` with
`sub_100089B0C`, and `saveLevel` with `sub_100089E6C`. All three adapters
strictly consume their recovered string/boolean slots and return zero Lua
values. These persistence APIs therefore no longer expose host-added success
booleans; malformed argument types raise errors at the same adapter boundary.

Hopper's full 204-block decompilation of `sub_10004730C` also closes the
editor-export filter that the generic serializer alone could not reproduce.
The exported root accepts, in native order, `trainCarts`, `theme`, the three
camera-data tables, `physicsToWorld`, `joints`, `tracks`, `counts`,
`doNotWaitForMovingObjects`, `themeSprites`, both force multipliers,
`worldGravity`, `variantGroups`, `variantProbabilities`, and a rebuilt `world`;
unlisted runtime fields are discarded. World entries whose key is not a
string are skipped, as are the two definitions `BLOCK_SENSOR_PIG_A` and
`BLOCK_SENSOR_PIG_B`. Every retained block writes the mandatory
`angle/x/y/name/definition/z_order` tuple, then the recovered optional fields.
`gravityFilterCategory = "NONE"` is deliberately omitted.

The sensor-specific branch compares `sensorType` against the four literal
values recovered at `0x100049044..0x1000491B8`: `gravitation` writes its force
range and either radius or box dimensions/angle (plus water zero level when
enabled), `stream` writes radius/force/nodes/vertices, `killing` writes width
and height, and `collectible` needs no extra geometry. `sensorType`, `isWater`
and `canBeEdited` are decision inputs rather than serialized fields. A true
`canBeEdited` admits the eight explosion/force/sucker scalars. Finally,
`editableAttributes` is walked as a one-based string list; named nil values
are skipped, numbers/booleans/strings/tables are copied, and every other type
raises Purple's literal “Attribute … can't be saved because it's of an
unsupported type” error. The Rust `saveLevel` now builds this filtered table
before invoking the already recovered text serializer, with a save/reload
regression covering all four sensor routes, both excluded pig sensors,
custom editable fields, discarded runtime fields and the unsupported-type
exception.

## Native formats

Image `.dat` files use a big-endian chunk envelope beginning with either `KA3D`
or `RVIO`. Purple validates the root length only as an upper bound, then scans
the physical remainder block by block; known blocks consume their fields
directly, while unknown blocks alone are skipped by declared length. `SPRT` v1
stores a version, one texture filename and named atlas rectangles with signed
pivots. Multiple SPRT blocks replace the sheet's current texture, and each
constructed sprite retains the texture pointer active for its block. Two
composite layouts occur:

- `KA3D COMP` versions 1 and 2 store the sprite name and signed 16-bit x/y
  offsets. Version 2 additionally ends each composite with a counted metadata
  list whose records are a string and two u16 values; shipped lists are empty;
- `RVIO COMP` stores sprite name, optional string id, signed x/y, float32 x/y
  scale and angle, then two independent one-byte flip flags. The loader forms
  the Entry-map key as `name#id`, converts the angle from degrees to radians
  immediately and converts each nonzero flip byte to a `-1.0` multiplier.

The parser preserves native Entry identity, block overwrite behavior and float fields rather than
misreading the RVIO id length as a z value. `stella-tool inspect` also decodes
`SPRT` rectangles, `COMP` part records, `FONT` headers and `TEXT` counts, which
makes resource comparisons reproducible without manually decoding the
big-endian payload.

The binary composite loader at `sub_100461B98` reads the two flag bytes in
order and converts them to independent `-1` scale multipliers: the first byte
mirrors X and the second mirrors Y. All three combinations occur in the
shipped pig, level-end and theme composites. The renderer now applies both
multipliers. `sub_1004376D4` also composes the complete parent and child
matrices before `sub_100467BE0` transforms the four vertices; the Rust path now
retains the resulting affine shear instead of approximating every nested
transform as an angle plus two scales.

Texture `.pvr` files are PVR v2 (`PVR!` at offset 44). This build contains only
OpenGL RGBA4444 (`0x10`) and RGBA8888 (`0x12`) PVR formats.

Bitmap fonts use the same big-endian envelope with resource type `FONT` and
versions 1 or 2. The header is texture name, signed leading, signed tracking
and glyph count; v1 glyphs use a 16-bit key and v2 glyphs use a 32-bit key,
followed by signed `(x, y, width, height, pivotY)` fields.
The renderer now uses those atlas glyphs and native tracking/baseline metrics
instead of substituting a host font. KA3D localization `TEXT` resources are
read in the executable's two passes: `LDAT` first establishes locale order,
then a reopened stream reads `LIDS` and selects the matching `TXGP` by locale
index while skipping unknown chunks. The older non-KA3D locale-section and
per-language forward-offset layout is supported by the same public parser.

## Physics

`sub_100055438` reads the Lua fields `deterministicPhysics` and
`gameWorldScale`. Their recovered object offsets are `+400` and `+404`. The
engine preserves a fixed-step clock and exposes world/physics coordinate
conversion through a 20:1 adapter.

`sub_10005E898` calls `sub_10086F3FC` as `b2World::Step(dt, 10, 10)`.
IDA and Hopper agree that the island solver at `sub_10086CE84` clamps each
step's translation to `0.16` and rotation to `1.5708`, uses linear and angular
sleep tolerances squared of `0.0025` and `0.00121847`, and sleeps an island
after `0.5` seconds below both thresholds. The rehost uses those exact values,
the recovered rational damping form `clamp(1 - dt*damping, 0, 1)`, and the same
ten velocity/ten position iteration budget. The leading island loop is now
float32 throughout: it forms `gravityScale * gravity + invMass * force` with
the recovered ARM fused multiply-add sequence, integrates torque through
`invI`, and applies the two clamped damping factors before warm start. Forces
are cleared only after every island returns. Position velocity clamps and
their resulting translations are float32 as well. A fixed-rotation flag makes
`invI` zero but does not suppress damping or integration of an explicitly set
angular velocity; this otherwise surprising Box2D branch is preserved.

`b2Body::SetTransform` at `sub_10086B794` and `ResetMassData` at
`sub_10086B1F4` also establish that transform origin and centre of mass are
independent native state. The body stores `localCenter`, `c0/c` and `a0/a` in
its `b2Sweep`; position integration adds directly to `c`, then reconstructs
the transform origin from `c - R*localCenter`. The rehost now persists that
float32 sweep centre instead of deriving it again from the already-rounded
origin after every constraint impulse. SetTransform explicitly resynchronizes
the sweep, while position/contact/joint integration updates it directly.
ResetMassData walks the intrusive fixture list head first (reverse creation
order), computes every shape mass/centroid/inertia and aggregate in float32,
stores mass and inverse mass independently, shifts inertia to the aggregate
centre, and applies the recovered `cross(angularVelocity,
newCenter-oldCenter)` velocity correction. This removes both compound-order
rounding differences and reciprocal reconstruction of the body's stored mass.

World-level island assembly at `sub_10086E634` is now mirrored as well. It
clears the body/contact/joint island flags, seeds only active awake non-static
bodies, and performs the native stack DFS through touching, enabled,
non-sensor contacts and active physical joints. A sleeping non-static endpoint
is woken as soon as the DFS pops it. Static endpoints are included in the
island constraint/body arrays but terminate traversal; their island flag is
cleared after that island is solved, allowing two independent dynamic islands
to share one ground body. Each assembled island now runs its own complete
velocity, integration, position-convergence and sleep sequence, so a difficult
structure cannot force an already-converged island through extra position
passes. The shared-static-platform and retained sleeping-chain regressions
cover both branches.

`b2World::CreateBody` at `sub_10086DF90` independently confirms that new bodies
are inserted at the world-list head (`prev = 0`, `next = oldHead`, then
`head = body`). Body seeds and joint edges therefore use reverse creation
order rather than Rust map/name order. `b2BroadPhase::UpdatePairs` at
`0x10086BDB0` sorts and deduplicates signed `(proxyIdA, proxyIdB)` pairs before
AddPair. The rehost now reproduces the dynamic-tree leaf sequence
`0, 1, 3, 5, ...`, LIFO leaf-id reuse after body destruction/deactivation,
reverse fixture-list activation order, and ContactManager's list-head
insertion. Those recovered ids determine contact edge order inside the DFS and
Gauss-Seidel solver.

The earlier leaf-id model has now been replaced by the actual dynamic tree.
`sub_100860E30` establishes the 16-node initial free list and doubling growth;
`sub_100860F08`/`sub_1008613B8` allocate and free the native node union;
`sub_1008610A8` chooses insertion siblings with the float32 perimeter cost;
and `sub_1008616E0` performs the height-difference rotations. The Rust tree
maintains the same parent/child AABBs, heights, internal-node allocation and
free-list order. IDA and Hopper also agree that
`b2DynamicTree::Query<b2BroadPhase>` pushes child1 and then child2, so the LIFO
stack visits child2 first; the rehost uses that traversal for broad-phase
pairs and public AABB queries. Growth, balance, free-list reuse and query
integrity are covered beyond the initial 16-node capacity.

IDA and Hopper also agree on the complete proxy-movement constants and phase
order. Fixture proxy creation (`sub_10086CA54` through `sub_10085E310` and
`sub_10086100C`) expands each tight fixture AABB by exactly `0.1f` on every
side. Fixture synchronization at `sub_10086CB74` unions the old and current
tight AABBs, then `sub_1008615A0` leaves a contained proxy unbuffered; otherwise
it removes/reinserts the leaf, applies the same `0.1f` margin and extends the
fat AABB by `2 * displacement` in the signed direction. The broad-phase wrapper
at `sub_10085E3E8` enters the proxy in the move buffer only for that reinsert
case. The rehost now mirrors those float32 operations, creates the sorted
fixture-pair contact before narrow phase reports touching, and preserves that
creation/list order through the later `BeginContact` transition. A dedicated
regression holds two boxes inside overlapping fat AABBs but outside narrow
phase, then moves them into contact and verifies that the original contact
creation order is retained.

The contact-manager pass itself is now recovered rather than approximated by
an all-object collision scan. `b2World::Step` at `sub_10086F3FC` services the
new-fixture pair buffer and then invokes `sub_10086BAB0`; both disassemblers
show that `Collide` walks the contact list from its head. If neither endpoint
has the awake flag on a non-static body, it skips filter validation, fat-AABB
testing and `Contact::Update` together, preserving the old touching bit and
manifold verbatim. Otherwise it applies a pending filter, destroys a contact
whose proxies no longer overlap, or updates its narrow-phase manifold and
listener callbacks. Body `ShouldCollide` at `sub_10086B73C` additionally
requires at least one body of dynamic type 2 and rejects a non-colliding joint.
The Rust contact manager now follows that contact-list order, including the
reverse order induced by AddPair's list-head insertion, rejects static/
kinematic-only pairs and freezes sleeping contacts until an endpoint wakes.
`Contact::Update` at `sub_10086373C` further shows that every list node copies
its old manifold, computes the new touching bit, wakes both bodies and clears
their sleep timers whenever that bit changes, and immediately invokes
BeginContact or EndContact before `Collide` advances to the next node. The
rehost now performs this traversal one contact at a time rather than preparing
an entire event batch: Lua changes to velocity, sleep/lifetime state or joints
are therefore visible to the next contact update in the same native walk.

The physics-facing registrations in `sub_10002C274` establish these contracts:

- `setVelocity` (`sub_100040FA4`) writes the Box2D body's linear velocity and
  wakes it for a non-zero vector; `getVelocity` (`sub_1000410D8`) returns the
  scalar magnitude, while `getLinearVelocity` (`sub_10004110C`) returns the
  x/y pair. The exact wake test squares both float32 lanes and reduces them
  before a strict comparison with zero, while the magnitude path rounds
  `y*y` and then performs one `fmadd(x,x,y²)` before `fsqrt`. Consequently a
  NaN velocity is stored without waking a sleeping body. All three getters
  require a string name but return zero for an unknown object or an object
  without a Box2D body because `sub_100061AE4` is a nullable lookup;
- `setAngularVelocity` (`sub_100041064`) and `getAngularVelocity`
  (`sub_1000410B4`) access the body's float field. Its wake test is likewise
  the strict float32 comparison `value*value > 0`, and nullable body lookup
  returns zero;
- `applyImpulse` (`sub_10003F930`) consumes
  `(name, impulseX, impulseY, pointX, pointY)` and changes linear/angular
  velocity immediately; `applyForceNative` (`sub_10003F9CC`) accumulates force
  and torque for the next step. Both paths measure the point lever arm from
  the body's world center of mass—not its transform origin—and perform the
  cross product and accumulation at float32 precision. `ApplyForce` still
  accumulates torque while fixed rotation makes inverse inertia zero; it is
  not allowed to drop that torque before the next step;
- `setPhysicsEnabled` (`sub_100041ABC`) modifies an aggregate physics lock. Its
  optional named locks are reference-counted while the unnamed lock is
  idempotent; `unlockPhysicsLock` (`sub_100042208`) clears every outstanding
  reference for one name, and `isPhysicsEnabled` (`sub_1000421F8`) is true when
  the aggregate count is zero;
- `setWorldGravity` maps to `sub_1000343A8`, and object damping, sensor,
  activity, sleeping, gravity-scale and fixed-rotation flags map directly to
  Box2D body/fixture state.

The same registration block maps `clearParticlesNative` to
`sub_10004C630` (clear all) and `clearParticlesWithTagNative` to
`sub_10004C640`. The tagged entry point recognizes `INGAME_BACKGROUND`,
`INGAME_FOREGROUND`, `MENU`, and `ALL`; the last tag dispatches to the same
full-clear operation.

`createBox` (`sub_100034740`) receives the native ordering
`name, sprite, collisionEnabled, flag, x, y, width, height, density, friction,
restitution, z`. The Lua adapter's observed ordering is
`name, sprite, x, y, width, height, density, friction, restitution,
collisionEnabled, inactive, z`. Density zero and the sentinel density 100
create static bodies; other densities create dynamic bodies.

`setPhysicsScale` is registered to `sub_10004050C` through the
string/float/float adapter at `sub_10008897C`. After the throwing native
lookup, it applies the ordinary visual scale and then rebuilds supported
fixtures. Shape type 2 covers both boxes
and arbitrary polygon fixtures: it multiplies every current fixture vertex by
the signed new/old visual-scale ratios, stores the absolute resized native
width/height, destroys every fixture head-first, and recreates the saved shapes
in the same traversal order. Since `CreateFixture` inserts at the body-list
head, a compound polygon's fixture order reverses after every rebuild. Density,
friction and restitution are snapshotted from the live Lua object before the
synchronous `EndContact` callbacks and applied to every replacement fixture.
A circle remains circular and rebuilds one fixture with
`(abs(min(scaleX,scaleY)/blockTable.blocks[definition].scale)+0.0001) * radius`, taking
`radius` and its coefficients from the live Lua object. The original sensor
flag is restored after replacement, which wakes a previously sleeping body
only when that flag was true. Edge/line shapes throw the native unsupported
type error after visual scale has already changed. The rehost therefore keeps
visual, Lua and native fixture dimensions separate instead of letting every
`setScale` call silently resize collision geometry.

`native_setDensity` is the direct Lua method at `sub_100030D1C`: it writes the
float directly to `b2Body::m_fixtureList`, calls the Box2D mass-data reset, and
mirrors `density` back to `objects.world`. It does not walk the remaining
fixtures. The rehost therefore stores density per fixture and recomputes a
compound body's signed aggregate mass, density-weighted center and inertia;
only the current head fixture changes. A later `setPhysicsScale` rebuild again
gives every replacement fixture the density snapshotted from Lua. The first
shot's single-fixture `native_setDensity("Stella_1", 2)` is no longer swallowed
by a compatibility stub, while compound bodies retain the native head-only
distinction.

The polygon staging vector lives at engine offsets `+688..+704`.
`clearVertices` (`sub_10003F8C8`) resets its logical end pointer and
`addVertex` (`sub_10003F8D4`) appends one exact float x/y pair. The polygon
builder at `sub_100068310` consumes the full staged contour; contours above
the recovered seven-vertex direct-fixture limit, or smaller contours that fail
`sub_100870520`'s convexity check, enter its decomposition path rather than
being replaced with a rectangle. The rehost now stores the original contour
and every resulting convex fixture separately, uses their preserved area for
mass and returns one table per fixture from `getObjectVertices`.

Hopper's assembly for `getObjectVertices` (`sub_10005A7BC`) makes its ordering
and coordinate behavior explicit: it starts at `b2Body::m_fixtureList` at
`+0x70`, advances through `b2Fixture::m_next` at `+0x8`, reads the polygon
vertex count at shape `+0x98`, and emits those vertices in stored order. Since
fixtures are head-inserted, this is the reverse of creation order; after
`setPhysicsScale` destroys and recreates a saved head-to-tail list, the
Lua-visible compound order flips. Each coordinate is a float32 addition of
the body's position at `+0x0C/+0x10` and the already-resized shape vertex. The
routine does not apply the body's rotation. The rehost now preserves all four
details, including signed physics scaling and native float32 rounding.

The separate Lua utility `decomposePolygon` is `sub_100035FFC`. Unlike the
fixture builder it reads its first Lua argument directly as an array of
`{x, y}` tables and feeds the contour to `sub_10087007C`/`sub_100872360`.
IDA and Hopper show that this is not a first-ear `n - 2` triangle API: all
coordinates are first rounded to float32, positive-area input is reversed,
`sub_100871D5C` validates ears, `sub_100871498` chooses the candidate with the
largest minimum normalized edge cross product, and `sub_100871F08` merges
adjacent triangles into convex contours of at most eight points. Its final
cleanup removes same-direction near-collinear vertices with the recovered
`sin(2 degrees)` threshold. `sub_1008710C0` also recognizes non-adjacent
repeated points with per-axis distance below `0.001`, splits the self-touching
contour at that point and recursively decomposes both loops. The Rust utility
and actual polygon fixture builder now share that complete pipeline and
preserve the returned contour ordering.

`getRayCastedObjects` (`sub_10005464C`) reads `x1`, `y1`, `x2`, `y2` from its
argument and returns one flat six-slot record per hit: object name, hit x/y,
normal x/y, and segment fraction. The callback at `sub_10009366C` explicitly
skips sensor fixtures, keeps fixtures whose Lua `collisionEnabled` flag is
false, and emits one record for every hit fixture rather than coalescing by
body. The original Lua helper sorts those records by fraction; the Rust bridge
also returns them in that order. `getIntersectingObjects` remains the
corresponding AABB/name-array query. Its callback at `sub_1000934F4` inserts
Box2D body pointers into a set, so multiple fixture proxies are deduplicated;
inactive bodies are absent from the broad phase, while sensors and static
bodies remain queryable. The Rust query now walks the recovered dynamic tree,
tests the same `0.1f`-expanded fat fixture AABBs and deduplicates by body before
name lookup. This preserves concave gaps whose individual proxy bounds do not
cover the query while also retaining native hits just outside the tight shape
but inside its fat margin. Hopper's `sub_10086DF90` and block allocator at
`sub_100862BE0` additionally show a `0xC0` body request from a 16-KiB chunk:
fresh blocks advance by `0xC0`, while `sub_100862D2C` pushes destroyed blocks
onto the size-class free-list head. The host now keeps that address-slot state
separate from intrusive world-list creation order, so destroying the middle
of three queried bodies and creating a replacement returns
`first,replacement,last` just like the pointer-ordered native `std::set`.

The native BeginContact listener at `sub_100062520` calls Lua
`blockCollision(name1, name2, force, damaged, false, floor(secondDamage),
pointX, pointY, normalX, normalY)`. The last four floats were previously
documented in the wrong order. The callback is emitted only when a contact
begins, not once per solver frame. One- and two-controllable contacts instead
call `birdCollision` with exactly eight values (two names, force, damage,
point x/y, normal x/y), leaving its optional ninth Lua argument nil. Sensor
begins call `enterCollision`. The EndContact listener at `sub_1000653AC`
calls `exitTriggerCollision` for a sensor pair and then always calls
`exitCollision`; non-sensor pairs call only `exitCollision`. The rehost now
publishes this same callback surface and order. Native `removeObject`
(`sub_100089E6C`/`sub_100042260`) reaches `sub_1000674FC` and then
`b2World::DestroyBody` at `sub_10086E02C`; DestroyBody walks attached contacts
and invokes EndContact before releasing fixture/body storage. The rehost now
does the same synchronously, while both `objects.world` records still exist,
then removes the Lua record and drains all cached contact/position state. This
eliminates the delayed `exitCollision` nil-table crash exposed by several
valid Level 01 trajectories. The sensor-side helper `sub_10006525C` also
removes the ending type-2/type-3 sensor pointer from the opposite object's
overlap vector and clears its Lua `insideGravity` field only when that vector
becomes empty. The mirrored BeginContact branch sets the field only for type 2,
after `enterCollision`; EndContact clears it after `exitTriggerCollision` and
before `exitCollision`. Multiple overlapping sensors now preserve that exact
lifetime and callback order. `sub_1000653AC` also wakes both contact bodies and
zeros their sleep timers before those callbacks; ordinary separation and
DestroyBody-triggered exits now mirror that side effect without reintroducing
the incorrect solver-impulse wakeups.

The source boundary now follows these listener members directly. Hopper reports
`sub_100062520` (`BeginContact`) as 8,992 bytes/247 blocks,
`sub_1000653AC` (`EndContact`) as 212 bytes/10 blocks, the strict break-force
helper `sub_10006510C` as 244 bytes/9 blocks, and Lua joint-descriptor erase
`sub_10007BF50` as a separate 100-byte leaf. `physics_world/contacts.rs` is
therefore a 177-line contact-key/impulse/payload model; synchronous event
selection lives in `contacts/prepare.rs`, force/damage/score mutation in
`contacts/damage.rs`, and Lua/sensor/joint cleanup dispatch in
`contacts/dispatch.rs`. The split keeps callback execution outside the bridge
mutex and retains the native contact-list interleaving.

Circle/circle, circle/convex-fixture
and convex-fixture/convex-fixture narrow phases use the actual transformed
fixture geometry; decomposed concave polygons are tested fixture by fixture
instead of colliding their AABBs. Contact point velocity includes `omega x r`,
and normal/friction impulses update both linear velocity and angular velocity
through shape-derived inverse inertia. Circle/polygon manifolds now follow
Box2D's exact first-vertex, second-vertex and face-region branches, including
the `0.002` polygon skin and surface midpoint; a generic SAT extreme vertex
produced a visibly wrong lever arm in the Chapter 01 Level 02 hammock loop.
`sub_100863BC4` supplies the recovered
restitution velocity threshold of `-1.0`; `sub_1008646A4` supplies position
slop `0.001`, Baumgarte factor `0.2`, maximum correction `0.2` and convergence
limit `-0.003`. `sub_100863FAC` warm-starts cached normal/tangent impulses and
`sub_1008640D0` solves tangent impulses before accumulated normal impulses.
For two-point face contacts it executes the four-case 2x2 complementarity
solver; `sub_100863BC4` builds that matrix and uses the recovered
`k11² < 1000 * determinant` guard, otherwise reducing the constraint to one
point. Polygon/polygon and edge/polygon manifolds now retain both clipped face
points, with independent normal/tangent warm-start values, instead of
collapsing the face to its midpoint. The complete velocity path is now
float32: restitution bias, geometric lever arms, effective masses, mixed
friction, cached impulses and all ten Gauss-Seidel writes round at the same
places as the ARM single-precision instructions. The two-point solution also
combines both normal impulses into one body-velocity write, rather than
introducing a non-native intermediate rounding after point one.

`b2PositionSolverManifold::Initialize` at `0x100864CFC` confirms that position
constraints retain the velocity solver's local manifold witnesses rather than
rerunning narrow phase after integration. Type 0 transforms two local circle
centres and uses their midpoint; type 1 transforms face A's local normal/plane
and body B's clip point; type 2 does the symmetric face-B calculation and
negates its output normal. The rehost stores these three manifold types and
their local points explicitly. `sub_1008646A4` calls this initializer inside
the contact-point loop, so point two must rebuild both transforms after point
one has translated or rotated either body. Position contacts and fixture-pair
velocity contacts now use this sequential Gauss-Seidel state instead of one
stale body snapshot. The island's ten-pass position loop also returns early
only when the contact solver and every native/custom joint report convergence,
matching the branch in `sub_10086CE84`. Its transforms, separations, effective
masses, slop/Baumgarte clamp and position writes now remain float32 as well.
IDA and Hopper agree that the world-manifold initializer is 548 bytes/9 basic
blocks (complexity 3); they also isolate normal position solving at
`sub_1008646A4` (792/12) and TOI position solving at `sub_1008649BC`
(832/15), confirming those solvers do not belong in the local-witness module.
The former 244-line Rust file is consequently a seven-line facade over a
32-line model, 110-line witness constructor and 110-line world-manifold
initializer. Both normal and TOI solvers continue consuming the same object
without duplicating the native three-type switch.
For coincident circle centres, Box2D normalization retains a zero normal; it
does not fabricate an arbitrary `(1,0)` direction, so that degenerate branch
is now explicitly covered.

Polygon clipping now preserves Box2D's packed `b2ContactID` semantics:
incident vertices carry reference-face/incident-vertex indices, side-plane
intersections carry reference-vertex/incident-face indices, and flipped
manifolds exchange the A/B feature bytes. Warm-start impulses are transferred
by these IDs rather than array rank. The two-point 1000:1 condition-number
reduction is performed before any tangent or warm-start work, as in the native
constraint initializer.

The complete island call order is also observable at `sub_10086CE84`:
contact constraints initialize and `sub_100863FAC` applies their cached
impulses before each joint's `InitVelocityConstraints` callback runs; the ten
iterations then solve joints before contacts. Reversing the two warm-start
phases made Level 02's closed hammock/contact loop accumulate energy and
destroy its pig without a shot. Solver-internal impulses also do not call
`SetAwake` or reset body sleep time; only external state changes and island
wake propagation do. `InitializeVelocityConstraints` computes restitution
bias for every persistent and newly touching contact from one common
pre-warm island snapshot, even when its cached impulse is zero; only after all
bias values exist does WarmStart mutate body velocities. Each later fixture
constraint reads those live mutated velocities. The rehost now preserves all
of these details. Level 01 completes with the retuned exact-physics shot while
the Level 04 and Level 06 no-input checkpoints remain active. At frame 1,800
both checkpoints have an exact score of zero, and both Level 06 pigs remain in
the live object table.

The contact-manager phase is now separate from those island iterations.
`b2World::Step` runs `ContactManager::Collide` once before gravity/force
integration, freezes the resulting fixture-pair manifolds, and reuses that
constraint array for every Gauss-Seidel velocity pass. The earlier rehost
incorrectly reran broad/narrow phase inside all ten passes, which could
change feature IDs, emit same-step lifetime transitions and omit later
constraints. The Rust world now refreshes Begin/EndContact once, performs the
native touching-transition wake before each Begin/End callback, and solves one frozen
velocity/position manifold set. A regression deliberately moves a body after
the refresh and verifies that the current island still consumes the frozen
manifold until the next contact-manager step.

The listener itself is part of that same pre-island phase. Purple invokes
`sub_100062520`/`sub_1000653AC` synchronously from
`ContactManager::Collide`, before gravity integration, contact warm starting
or any joint constraint. Damage, bounce bookkeeping, breakable-joint removal
and `enterCollision`/`birdCollision`/`blockCollision`/exit callbacks now run
at that point in the Rust step as well, individually interleaved with
`Contact::Update` in contact-list-head order. A callback that calls
`setVelocity` or removes an object therefore affects both the next contact in
the same `Collide` walk and the current island rather than the next fixed step.
`removeBlocks` remains after `b2World::Step`, matching
`sub_10005E898`, so dead-block queue consumption is not pulled into the locked
contact traversal. A dedicated sensor regression changes a body from `0.03`
to `3` units/s inside `enterCollision` and verifies that the body advances
exactly `0.1` unit during that same 1/30-second step. A second regression puts
a newer sensor contact ahead of a solid contact, changes the bird velocity in
the sensor callback, and verifies that the later solid collision force uses
the changed velocity rather than a stale batch snapshot.
The listener vtable at `0x100A90528` also resolves the remaining two Box2D
slots: PreSolve is `nullsub_20` (`sub_100062510`) and PostSolve is
`nullsub_18` (`sub_100062508`). IDA's pointers and Hopper's empty
pseudocode agree, so Purple has no hidden per-persistent-contact or
post-impulse game callback to reproduce beyond the Begin/End paths above.

RTTI and vtable recovery identifies the five standard joint solver triples:
Distance `0x100865270/0x1008655D0/0x1008656D4`, Prismatic
`0x1008674D4/0x1008678E0/0x100867C68`, Revolute
`0x100868B90/0x100868F20/0x1008692B4`, Rope
`0x10086983C/0x100869B30/0x100869C48`, and Weld
`0x100869F54/0x10086A214/0x10086A398` for initialization, velocity solve and
position solve respectively. IDA and Hopper agree that Distance and Rope use
a `0.001f` velocity-axis cutoff while their position normalization uses
`FLT_EPSILON`; Rope additionally clears its accumulated impulse below that
cutoff. Prismatic uses `0.001f` linear slop and a `0.002f` equal-limit test,
rechecks live translation on every position pass, and does not clamp or wrap
its angular error. Weld likewise keeps the unwrapped Box2D sweep-angle error.
Body sweep integration is consequently continuous past `2π`; only the
explicit RenderObject `setAngle` adapter normalizes its input.

The retail-critical Weld and Revolute paths are now instruction-order
float32 implementations rather than double-precision equivalents. Hopper's
`sub_100862E14` and `sub_100862D60` are the shared `b2Mat33::Solve22` and
`Solve33` helpers: both form their cofactors and negated determinant in the
observed ARM order, leave the reciprocal at zero for a singular matrix, and
therefore return a zero vector without a fabricated fallback. Revolute
initialization scales all four cached impulses by the float32 step ratio,
uses the exact constants at `0x100A0CA20..0x100A0CA30`, and applies the warm
impulse after classifying lower/upper/equal limits. Its velocity solver stages
the motor-adjusted angular velocities before forming point `Cdot`, preserves
the one-sided accumulated-limit complementarity branch, and writes velocities
and impulses back as float32. The position solver likewise applies the angular
limit first, rebuilds both rotated anchor arms, and then solves the live 2x2
point constraint. Weld uses the same recovered 3x3 cofactor path, float32
cached accumulation and unwrapped sweep angle. Exact motor/warm-start,
off-centre weld and coupled revolute-limit regressions cover these paths.

The collision factory and retail narrow phase are now matched at their direct
dispatch boundaries. Circle-circle uses vtable `0x100AB15D0`, evaluate
`0x10086348C` and `sub_10085E590`; polygon-circle uses vtable
`0x100AB16C0`, evaluate `0x10086512C` and `sub_10085E624`; polygon-polygon
uses vtable `0x100AB1710`, evaluate `0x1008651E8` and `sub_10085F648`.
Direct edge-circle/edge-polygon evaluate at `0x100864FB4/0x100865070`, while
the chain wrappers at `0x100863204/0x100863350` dispatch to the same
`sub_10085E8AC` and `sub_10085F5F8` (`sub_10085EADC`) implementations.
`sub_100860148` is the shared two-vertex clip helper.

These paths now keep the observed float32 product/FMA order and reject only on
strict `separation > radius`, so exact skin-radius contact is retained.
Polygon-polygon loads `0.001f/0.98f` from
`0x100A0C7CC/0x100A0C7D0`; edge-polygon uses the `0.004f` combined skin at
`0x100A0C7B8` and its matching `0.001f/0.98f` primary-axis bias at
`0x100A0C7C4/0x100A0C7C8`. Circle-circle preserves the native `(1,0)` world
axis below `FLT_EPSILON`, polygon-circle clears `b2ManifoldPoint::id.key` in
every face/vertex branch, polygon/edge clipping preserves A/B feature swaps,
and independent edge fixtures use the native two-sided Voronoi regions. The
factory has no edge-edge contact registration, so crossing line fixtures no
longer receive a fabricated Rust manifold. Boundary, reference-axis, feature
ordering and absent edge-edge regressions cover each distinction.

The source layout now mirrors those dispatch boundaries as well. Hopper sizes
the five core leaves at 148 bytes (`sub_10085E590`), 648 bytes
(`sub_10085E624`), 560 bytes (`sub_10085E8AC`), 2,844 bytes
(`sub_10085EADC`) and 1,340 bytes (`sub_10085F648`); IDA independently agrees
on both edge leaves at `0x230` and `0xB1C`. The direct edge-circle and
edge-polygon evaluate wrappers remain separate 32-byte leaves, and both tools
also agree on the shared 212-byte/8-block clip helper `sub_100860148`.

The polygon call graph is now recovered below the collision entry as well.
IDA and Hopper agree exactly on `b2FindMaxSeparation` `sub_10085FB84` at 496
bytes/11 basic blocks, its `b2EdgeSeparation` leaf `sub_10085FD74` at 228/6,
and `b2ClipSegmentToLine` `sub_100860148` at 212/8. The maximum-separation
member does not scan all polygon faces: it transforms the centroid delta,
chooses the first strictly greatest aligned normal, samples the preceding and
following faces, then climbs strictly in only the better direction. The Rust
implementation now reproduces that search and its tie ordering. A convex
five-edge regression contains two local separation maxima and proves the
observable distinction: a full scan selects edge 2, while Purple's search
selects edge 4 and therefore preserves a different contact feature identity.
The 152-line collision entry, 153-line separation/search unit and 43-line clip
leaf now mirror those native boundaries while remaining shared with the
edge-polygon collider.

The edge-circle face-region audit additionally exposed an algebraically hidden
rounding boundary. At `0x10085E954..0x10085E988`, Purple forms the reciprocal
edge length, rounds `B*v`, folds `A*u` into its negation with `FNMADD`, then
forms `Q - closest` with `FMADD` before the squared-radius comparison. The
former Rust expression materialized `closest` first. For the focused boundary
case this rounded distance squared to `0.0609355718` and fabricated a contact;
the native sequence yields `0.0609355979`, strictly outside squared combined
radius `0.0609355755`. The face branch now also reconstructs its final point
and penetration from the b2WorldManifold plane separation instead of reusing
the GJK-style distance magnitude.

Accordingly the former 988-line aggregate remains an 18-line facade over its
circle, polygon and shared-geometry modules, while the former 404-line mixed
edge module is now a 7-line facade over a 147-line edge-circle leaf and a
260-line edge-polygon leaf. Feature-ID and clipping ownership remain unchanged.
The new fused-boundary regression raises the verified workspace total to 272;
strict Clippy, the release build and the current
`audit-edge-leaves-menu.png`/`audit-edge-leaves-start.png` wgpu flow captures
pass after the split.

The continuous-collision core is now recovered as well. Hopper identifies
`sub_10086EA54` as `b2World::SolveTOI`, reached by `sub_10086F3FC` after the
ordinary island solve when the world's continuous-physics byte is enabled.
It builds each fixture's distance proxy through `sub_1008602D4` and calls
`sub_100861B54`; the latter is Purple's float32 `b2TimeOfImpact`, while
`sub_1008605D4` is its `b2Distance`/GJK query. The Rust port now keeps the
simplex feature cache, all Points/FaceA/FaceB separation-function branches,
sweep-angle normalization, and the observed conservative-advancement limits:
20 outer iterations, eight separation pushes and 50 alternating
bisection/secant root evaluations. Its target is exactly
`max(0.001f, radiusA + radiusB - 3 * 0.001f)` with `0.00025f` tolerance.
Consequently a radius-0.01 circle crossing a radius-0.002 edge at the
translation-clamped 0.16-unit step advances to alpha about 0.44375 (the
native target separation), rather than the earlier sampled first-overlap
alpha 0.425. The world path synchronizes swept proxies, advances the earliest
dynamic/static candidate, invokes BeginContact before the reduced solve,
disables warm starting, performs the recovered ten velocity iterations and
integrates the `(1-alpha)*dt` remainder. High-speed edge crossing and direct
point-separation regressions lock both the externally visible no-tunnelling
behavior and the native target calculation.

`sub_10086D5EC` is the reduced `b2Island::SolveTOI` invoked by that world
loop. Before its velocity constraints it calls `sub_1008649BC` up to 20
times, using TOI Baumgarte `0.75f`, linear slop `0.001f`, maximum correction
`0.2f`, and the `-0.0015f` convergence boundary. The rehost now preserves
that position-before-velocity order, adds every simultaneously touching
static contact from the selected body's contact-edge list to the same reduced
island, and rescans the world after each remainder integration. A two-wall
bounce regression proves that one body can produce two ordered impacts within
one 1/30-second step; a diagonal-corner regression proves that two equal-time
static contacts are solved together. Auxiliary contact edges are advanced one
at a time around the Lua callback boundary, and a regression verifies that a
first BeginContact callback can disable the next candidate before it is added
to the same island. All recovered object constructors write
the five `b2BodyDef` booleans as `{true, true, false, false, true}`
(`allowSleep`, `awake`, `fixedRotation`, `bullet`, `active`), and Purple
contains no bullet setter/string, so the shipped candidate rule is the
implemented non-bullet dynamic/static route rather than a latent
dynamic/dynamic bullet path.
The per-step contact state now also mirrors `e_toiFlag`, cached absolute alpha
and `m_toiCount`: untouched contacts retain their cached fraction across a
rescan, contacts attached to the solved body are invalidated, the selected
contact increments its count, and counts above eight are skipped exactly as
at `0x10086EC60`.

The same listener computes breakable-joint damage before the velocity solver
changes either body. `b2World::Step` calls the contact manager's collide pass
before island gravity/force integration, so a newly begun contact observes the
velocities that entered the step rather than the gravity-updated velocities.
IDA's `0x1000636C8..0x100063724` path and Hopper agree on
the float32 vector formula
`length(forceDamageMultiplier * (mTarget*vTarget - factor*mAttacker*vAttacker)) / 10`,
where `factor` is the attacker's material-specific `damageMultiplier *
powerupDamageMultiplier`. `velocityMultiplier` is loaded alongside these
fields but is used later only by the non-legacy post-destruction velocity path
at `0x100064588..0x1000645C8`. The divisor is the `10.0` stored at GameLua
`+0x524`. `sub_10006510C` removes a breakable joint only for
the strict comparison `collisionForce > breakForce`. The controllable-body
path applies this force only to the target (which must be dynamic or have
`blockCollisionEnabled`); ordinary contacts perform both asymmetric
directions. The rehost now snapshots pre-solve mass/velocity and follows these
same target rules instead of treating post-solve penetration error as damage.

The target-destruction branch is also now preserved rather than folded into
the collision-force calculation. With `useLegacyCollisionPath`, Purple sets
the controllable body's velocity directly to its pre-impact velocity times
`min((remainingStrength / (birdMass * collisionForce)) * 10 * -1.75, 1)`.
The non-legacy path writes the pre-impact velocity times
`min(velocityMultiplier * (collisionForce - previousStrength) /
collisionForce, 1)` to the `std::map<string, b2Vec2>` at GameLua `+0x730`.
IDA's `sub_10005E898` and Hopper independently show the complete lifecycle:
after every Box2D step it calls Lua `removeBlocks`, then resolves each map key
in the live object map, wakes the non-static body for a non-zero vector,
assigns its linear velocity, and finally clears the entire map.
The non-zero test is not a host-language double comparison: Hopper shows a
packed float32 `FMUL` followed by `FADDP` over the two velocity lanes, then a
strict comparison with zero. Thus each square rounds independently before the
float32 addition; a smallest-subnormal velocity is still assigned but its
squared length underflows to zero and does not wake a sleeping body. The Rust
path and its regression now preserve that boundary without using an FMA.
`sub_10005E860` is the common writer, also reached from DirtMechanics'
collision bridge at `sub_100020560`. The Rust fixed step now implements the
same callback/remove/write/apply/clear order at float32 precision.
The same update function calls Lua `clearLuaForceFunctions` after the complete
fixed-step loop, even when the accumulator is still below `1/30`. The rehost
now gives an `applyForce` closure exactly that one-render-frame lifetime and
does not suppress `updatePhysics` merely because every body was asleep before
the closure had a chance to wake one.

The second constructor boolean is persistent `controllable` state at scene
offset `+0x140`, not merely an initial-active argument. A controllable body is
created inactive for the sling but keeps that identity after `setActive(true)`
so collision attribution still identifies the bird. `blockCollisionEnabled`
is the byte at `+0x13f` and every native constructor initializes it to true;
`+0x142` is `sensorDefinition`. Both fields now survive in the Rust scene
model.
The Rust island step now follows the recovered native phase order independently
for every DFS island: velocity integration, warm-start, ten interleaved
joint/contact velocity passes, transform integration with the
`0.16`/`1.5708` clamps, then up to ten contact/joint position passes and
island-local sleep accounting. Its broad phase only visits pairs
involving an awake moving body, which matches the relevant Box2D dynamic-tree
behavior and avoids quadratic scans of settled level art.

`createLineShape` is registered through `sub_100087704` to
`sub_1000364E0`. The common builder at `sub_100068130` does not create a
one-sided chain: it walks every consecutive pair in the staged vertex buffer,
calls the two-vertex `b2EdgeShape::Set` path at `sub_10085D7A0`, and attaches
one independent two-sided edge fixture per pair. Its inlined shape definition
contains radius `0.002` (`0x3B03126F`). The rehost now mirrors those individual
capsule-like edge fixtures, including endpoint rejection, two-sided contacts,
the same polygon/edge skin radius in narrow phase and AABBs, and Box2D's unit
mass fallback for dynamic zero-area edge bodies.

`createJoints` (`sub_10003CC64`) is a batch wrapper that invokes
`createJoint` (`sub_100037374`) for every descriptor in the supplied table.
The complete switch is now recovered rather than inferred from the first
level: script type 1 creates a distance joint, type 2 a weld, and type 3 a
revolute. Type 4 and a type 5 descriptor without `oneWayDestroy` create a
prismatic joint through `sub_1008673AC`; the native prismatic defaults are
limit enabled, lower/upper translation `0/5`, motor enabled, speed `0`, and
maximum force `10000`. A type 5 descriptor containing the boolean
`oneWayDestroy` field creates no Box2D joint—even when the value is false—and
instead stores the destruction-link timer/direction metadata. Type 6 creates
a Box2D rope joint (enum value 10), with local anchors `(-1,0)/(1,0)` and an
optional `maxLength`. Types 7-10 dispatch through `createCustomJoint`; the
shipped retail handlers do not all create constraints. Type 7 calls
`createRopeFromJointDefinition`, which creates invisible rope-link bodies and
revolute joints; only the non-play editor branch temporarily changes it to a
type-2 weld. Type 8 appends end2 to end1's `triggerTargets`, type 9 assigns
end2 as end1's `areaOfEffect`, and type 10 either deletes one variant or swaps
two controllable birds. The Rust entry now releases its scene mutex before
that Lua dispatch, mirrors dynamically created descriptors in
`objects.joints`, and permits the type-7 rope builder to re-enter
`createJoint` without deadlocking.

IDA and Hopper also establish the useful source boundary inside this family.
IDA measures `sub_100037374` as 21,876 bytes with 535 basic blocks and
cyclomatic complexity 181, while its independent `sub_10003CC64` batch wrapper
is only 144 bytes with four blocks. Hopper's less aggressive block coalescing
reports the same single-joint entry as 18,240 bytes/243 blocks and confirms
that the batch wrapper's sole semantic call is `sub_100037374`. The eventual
Box2D insertion is a separate 272-byte, 15-block `b2World::CreateJoint`
function at `sub_10086E470`. The Rust joint-construction source is therefore
split without inventing new public entry points: `construction/lua_bridge.rs`
owns the Lua descriptor mirror and the post-lock `createCustomJoint` callback,
`construction/physics.rs` owns native joint-definition decoding and world
insertion, and the seven-line `construction.rs` facade preserves the original
ordered entry path.

The `coordType` conversion is also exact. The adapter rounds with
`floor(value + 0.5)`: 0 ignores supplied coordinates and uses body-center
anchors; 1 subtracts each body's translation without inverse rotation; 2
uses the supplied body-local values directly; invalid values retain the
zeroed joint-definition defaults. Type 6 has its separately inlined rope
variant of this switch. All shipped typed joint records use `coordType=2` on
their physical path. Weld initialization averages the two transformed anchor
positions, while revolute and prismatic initialization use end1's transformed
anchor as the common world point.

The rehost now contains warm-started distance, coupled 3x3 weld/revolute and
prismatic solvers plus the one-sided rope limit, including motor clamping and
lower/upper/equal complementarity. Every physical descriptor reads
`collideConnected`; the native default is false, so connected fixtures are
filtered unless a joint explicitly opts in. Metadata-only type 5 links do not
filter contacts. IDA `sub_10086E470` and `sub_10086E27C`, independently
confirmed in Hopper, establish the exact deferred topology behavior. Creating
a non-colliding physical joint marks every existing contact between its two
bodies with `b2Contact::e_filterFlag` (`0x8`) but does not wake them or destroy
the contacts immediately. `ContactManager::Collide` tests the awake gate before
that flag, so a contact shared by two sleeping endpoints remains dirty.
Destroying a physical joint wakes both bodies and, when `collideConnected` was
false, flags surviving contacts again; metadata-only links do neither. The
Rust contact manager now preserves this flag and its same-pass callback
visibility instead of consulting the current joint map eagerly in every
solver pass.

A one-process audit booted all 153 extracted levels: 86,450 level objects
referenced 558 of 797 block definitions with zero missing definitions. It
found 3,799 typed joint records: type 2 = 2,843, type 3 = 519, type 5 = 196,
type 7 = 207, and type 8 = 34. Every type 5 record contains
`oneWayDestroy` (25 true, 171 false), so all shipped type 5 records select the
metadata-only branch; types 4 and 6 remain implemented for original API/editor
compatibility although retail levels do not instantiate them. Breakable
records comprise 328 type-2, 12 type-3 and 2 type-7 descriptors. Only five
physical descriptors enable `collideConnected` (one type 2, four type 3).
The type-3 audit also found 13 enabled motors, 14 enabled limits, and
`backAndForth` true on 517 of 519 descriptors.

`createTrack` builds the native table returned to Lua and the corresponding
track constraint rather than returning a compatibility placeholder. Point
conversion is split into `getWorldPoint` (`sub_1000550B8`) and
`getLocalPoint` (`sub_10005521C`); both IDA and Hopper show the standard body
rotation/translation pair and its inverse. `setRevoluteJointSpeed`
(`sub_100044C18`) resolves the named joint, wakes both endpoint bodies and
stores motor speed at joint offset `+0xA8` through `sub_1008696B4`.
`checkJointLimits` and `handleJointLimits` use the same current revolute angle:
the former reverses a motor at the reached boundary and the latter stops it,
and both are void Lua calls. `isJointAttached` does not inspect the body's
joint list; it resolves the named fixture and calls Box2D `TestPoint`.
DirtMechanics' `sub_100020858` first adds the supplied local offset to the
body's position with two float32 `fadd` instructions, then walks every fixture
and invokes the shape's `TestPoint` virtual method. The rehost preserves that
pre-test rounding rather than adding the two Lua numbers in double precision.
These ABI distinctions are covered by regressions.

`getCurrentTrackAngle` resolves the named object in `sub_10003D650`, reads its
track object from body `+0x88`, and calls `sub_10085D364`. That helper expands
the stored chain into child edges, projects the body's float32 position onto
each edge and retains the first strictly smaller squared distance; it never
adds an implicit closing edge. `sub_10085DAA4` then returns float32
`atan2(edge.y2-edge.y1, edge.x2-edge.x1)`, or zero when the object has no
track. The Rust track store now rounds input points to float32 and reproduces
the native projection/FMA, tie order, open-chain children and atan2f result.

The constraint itself is also no longer modeled as a post-step coordinate
snap. `sub_10086DA58` selects and caches the closest child before island
solving, derives the edge angle/basis, and warm-starts the accumulated linear
correction only when that child is unchanged (with the native `0.9f` angular
damping). Every one of the ten velocity iterations then calls
`sub_10086DB8C` between ordinary joints and contacts. It removes `0.1f` of
normal velocity, adds `0.1f` of line-distance drift correction, applies the
full tangential correction outside either endpoint, and moves angular
velocity `0.2f` toward the edge angle when `rotateBlock` is true or toward
zero otherwise. Only the normal component is accumulated for the next warm
start. Track position solving is `sub_10086DCE0`, a literal true return, so
the native body is integrated from its corrected velocity rather than being
teleported onto the polyline. Rust now follows that float32 island lifecycle.

`objectAndTrackOverlap` is the separate `sub_10003D208`, not an AABB helper.
It reads `points` from the second Lua table, rounds every point to float32 and
constructs a temporary radius-`0.002f` Box2D chain. For child edges
`0..pointCount-2`, it calls `sub_10086021C`: that routine builds distance
proxies for only `b2Body::m_fixtureList` (the current head fixture) and the
selected chain edge, runs radius-aware `b2Distance` with the body transform
and an identity track transform, and accepts residual distance below
`FLT_EPSILON`. The Rust binding now follows that exact field contract,
fixture-list selection, signed circle radius, rotation, skin radii and convex
core distance instead of intersecting the whole object's AABB with segment
bounding boxes.

Several small object stores are semantically separate despite their similar
Lua signatures. `setSensorGravityMask` (`sub_1000313B4`) writes the object
integer at `+0x104`, while `setObjectGravityCategory` (`sub_1000313D8`) writes
`+0x124`. `setPivotOffset` (`sub_100040228`) writes floats at `+0xB4/+0xB8`;
the object renderer at `sub_10006D5B4` divides them by object scale and uses
them as the secondary pre-scale translation active around Lua pre/post draw
callbacks. They are not copied into the atlas/rotation pivot pair.
`setDecorationObjects` (`sub_10003FD6C`) follows
`objects.world[name].definition` into
`blocks[definition].decorations.objects`, reads `amount`, `sprite`,
`angleIncrement` and `scale`, then installs the native decoration descriptor
at `+0xF8` and enables it at byte `+0x141`.

## Lua host globals and startup

The platform initializer at `sub_10002A278` assigns the resource roots
`data`, `images`, `fonts`, `audio`, `localization`, `levels`, `scripts`,
`scripts_common`, `shaders`, `config`, and `ios`. `sub_100026D2C` exposes the
corresponding `imagePath`, `fontPath`, `audioPath`, `localizationPath`,
`levelPath`, `scriptPath`, `commonScriptPath`, and `configPath` globals before
loading game logic. It also exposes a `res` resource object and device model
strings.

The recovered native startup callback at `sub_10005D44C` invokes Lua
`createStartUpAssets`. Menu and subsystem initialization then continue through
callbacks defined by `game_init.lua`, including `initializeGame`,
`initializeGameMenus`, `initializeEventSystem`, and
`gameSpecificInitSubsystems`.

`sub_10005761C` implements `loadLuaFileToObject`. Its recovered call contract
is `(path, parentObject, childName, loadFromBundle=true)`. For a non-empty
`childName`, it creates or reuses the named child table, injects the `gamelua`
reference as a real table field, evaluates the decoded chunk in that object's
environment, and then stores the child back on the parent. The empty-child
branch evaluates directly in the supplied parent and does not inject that
field. The Rust host now mirrors both branches, including `rawget` visibility,
rather than evaluating every object script in `_G`.

`loadLuaFile` itself is `sub_100058960`. IDA and Hopper both show that its third
argument does not mean "merge into `gamelua[childName]`": when true, the loaded
environment is routed into the persistent native `blockTable` at GameLua
offset `+1112`. With the fourth argument false, `sub_10007F33C` performs a
direct `blockTable[childName] = environment` replacement. With both booleans
true, `sub_100062028` accepts only string-keyed outer groups and numeric-keyed
definition tables, writes the numeric `index` and string `group` fields onto
the definition table itself, and publishes that same object as
`blockTable.blocks[definition]`. The temporary group arrays are discarded.
The numeric key is first narrowed through the native Lua-number-to-float path,
so large integer keys exhibit float32 rounding. Before evaluating a deep pack,
the loader also injects the existing `inheritsBlock` helper and the literal
`IGNORE_COMPONENTS = true` into its isolated environment. This is required by
the shipped level-goal/randomization packs: derived pigs intentionally inherit
without copying the base `components` table.
The rehost now preserves this raw table shape and replacement behavior instead
of relying on recursive merges and metatable aliases.

The common level implementation `sub_100065D3C` validates the loaded table's
`filename` against the requested final path component before publishing
`loadedObjects`. Its bundle path appends `.lua` unconditionally, so an already
suffixed caller probes `.lua.lua` rather than being silently normalized. It
then copies optional `gravityForceMultiplier` and
`waterForceMultiplier` values into native float fields at `+1328/+1332`; when
absent it reads `worldAttributes.defaultGravityForceMultiplier` and
`defaultWaterForceMultiplier`. Both bundle and AppData level paths now update
the Rust physics bridge at this same point with float32 precision. All 130
Chapter01/Chapter02 level files pass the recovered filename contract, and a
real `LevelLoad.transitionToLevel("Chapter01", 1)` run reaches the playable
L01 scene with the expected `2.5/1.75` multipliers.

The Lua adapter thunks at `sub_100083434`, `sub_100089E6C` and the two object
loader entries return zero Lua results. Accordingly, `loadLuaFile`,
`loadLuaFileToObject`, `loadLuaFileFromAppDataToObject`, `loadLevel`, and
`loadLevelFromAppData` now expose the same zero-result ABI instead of returning
a host-added success boolean; failures still propagate as Lua errors.

The adjacent registration audit also corrected `importJSONToLuaTable`.
`sub_100057450` accepts the JSON document itself plus the name of an existing
Lua table; `sub_100009944` rejects a missing/non-table target, and
`sub_1000E32C0`/`sub_100564BC8` populate that same table. Adapter
`sub_100089B0C` returns zero values. This matches the shipped
`setInstalledApps`, which creates `possibleInstalledApps` and passes the HTTP
response text directly, rather than treating the first argument as a file path
or expecting a returned table.

`loadTextFileToString` uses the one-string/three-boolean adapter
`sub_100083998`. Its member `sub_1000512D8` reads a plain file below the
GameLua AppData root when the first boolean is false. When that boolean is
true it opens the virtual resource path (including downloaded AppData assets),
decrypts with the ordinary resource key or the alternate 32-byte key at `0x1009AEF30`
(`0xMizJJUh7BbwmYhqxpJ038x8YGvk6aU`) when the second boolean is true, then
optionally unwraps the 7z payload when the final boolean is true. The Rust host
now mirrors all four strict slots and returns the resulting byte string without
UTF-8 coercion. Its virtual-resource lookup checks the downloaded AppData
location first and the shipped bundle fallback second. Extracted `config/*.json`
files are already decrypted/decompressed forms of the original `.dat`
containers, so that representation is recognized before applying AES/7z.

The direct Lua C entry `native_loadTextFileToLuaTable` (`sub_100051810`) uses
a related but distinct five-slot contract: required path and encrypted-resource boolean,
then optional `parseJSON`, `decompress`, and `alternateKey` booleans in that
order. Missing optional slots default to false, while an explicitly supplied
non-boolean (including `nil`) is rejected. It reuses `sub_1000512D8` with the
last two flags reordered. Empty input yields one `nil`; JSON mode calls
`sub_1000E32C0`, while ordinary mode reaches `sub_10052AE8C` and
`sub_100529004`, which compile the complete byte span through Lua 5.1's
load-buffer path, execute it, and return its first value. The Rust host now
preserves those two modes instead of permissively treating every file as JSON.
The shared resource-key decryptor was also split at the same native boundary:
the text loader accepts valid non-7z plaintext when decompression is disabled,
whereas the archive decoder still enforces the 7z signature.

The Rust source now follows this recovered ownership boundary as well.
`game_lua/text_files.rs` contains `sub_1000512D8`, `sub_100051810`,
`sub_100057450` and their registration bridges; `game_lua/persistence.rs`
contains the `sub_10004B394`/`sub_10004B6D0`/`sub_10004B880` table-file paths
and `sub_10052A020..sub_10052A8FC` serializer family. The public `lib.rs`
coordinates registration, and its 215 regression tests now live in
`tests.rs`. This first structural pass reduced the production facade from
36,224 to 9,928 lines without changing any Lua global or adapter order. A later
ownership pass completed that migration: `lib.rs` is now a 54-line public
facade, while the aggregate constructor is a 352-line order-preserving
coordinator over subsystem installers.
The adjacent `game_lua/platform.rs` mirrors the small platform registration
group. Its latest IDA audit confirms that `getDirectoryFileList` adapter
`sub_100083004` strictly converts stack slot 1 to a string before
`sub_10005A298` returns an empty Lua table; `checkDirectory` uses the same
strict string conversion and the literal-false member `sub_10004BAA0`.
`GetDate` (`sub_1000313FC`/`sub_1000886D0`) truncates `time_t` to signed int32,
divides by 3600, then pushes a float32 rather than using an unbounded host
integer. Those edge contracts and missing/wrong-type failures are now covered.

The platform group now follows that recovered ownership structurally as well.
`platform.rs` is a 35-line coordinator over `registration`, `time`,
`installed_apps`, `misc`, `sharing` and `sha1` leaves. Their order follows the
relative member publications in `sub_10002C274`: device identity, `GetDate`,
URL dispatch, installed-app checks, epoch conversion, unlock checksum,
filesystem/platform calls and screenshot capture. The native members remain
separate—for example Hopper measures `sub_1000313FC` at 56 bytes,
`sub_10005716C` at 440 bytes/14 blocks and `sub_10005966C` at 804 bytes/43
blocks—so this is an ownership split rather than an arbitrary line-count split.
`getTimeFromEpochSeconds` now also matches the recovered adapter: slot 1 is a
strict string parsed like `operator>>(long)`, while slot 2 selects local time
only when it is the Boolean value `true`; missing and non-Boolean values use
UTC. `getUnlockRequestChecksum` reads the top three stack values (`-3`, `-2`,
`-1`), requires a numeric selector, rounds it through float32 and hashes
`second + salt + first`.

The same ownership pass now places the recovered CMWC/MSVC-LCG pair in
`game_lua/simple_random.rs`. The new `resource_manager/fonts.rs` follows the
native ResourceManager boundary: installed face enumeration, system-font
creation, intermediate float32 plus FCVTZS-style integer conversion, bitmap
font parsing, string metrics, and `clipText` line breaking live together.
The split was checked with all 251 workspace tests, warning-free Clippy, a
release build, and a direct Chapter01/L01 wgpu capture.

IDA's `sub_100446570` and Hopper's cross-references to the adjacent method-name
strings independently identify the complete `game::LuaResources` registration
object. In particular, `drawSprite`, `getSpriteBounds`, `getSpritePivot`,
`getCompoSpriteBounds`, `getCompoSpriteData`, `getCompoSpriteEntry`, and
`setCompoSpriteEntry` are one native ownership group. Their shared KA3D sprite
and composite parsing, recursive bounds, Lua entry conversion, strict draw
overload parsing, and anchor calculations now live in
`resource_manager/geometry.rs`; the public Lua registration order remains in
the coordinator during this migration step. The complete `res` and
`ResourceManager` table construction has now moved as one unit into
`resource_manager/registration.rs`, matching the `sub_100446570` constructor
boundary rather than splitting its closures by arbitrary source length. A
typed `RegistrationContext` keeps the constructor's captured runtimes explicit
while preserving the original global-install position. All 251 tests,
warning-free Clippy, the release build, and a direct Chapter01/L01 capture pass
after this move.

That constructor is now represented as an order-preserving coordinator rather
than one 1,351-line Rust function. Its contiguous native groups are split into
resource lifecycle, locale/font, sprite/composite query, audio setup, audio
playback, resource-backed draw and legacy `ResourceManager` installers. The
typed context and the original `res`/global publication points remain in
`resource_manager/registration.rs`.

Hopper cross-references for `loadFromBundle`, `loadFromAppData`,
`setPlaybackEvent`, `getEntityWorldTransform`, and `getEntityWorldBounds` all
land in the same registration function that IDA identifies as
`sub_10000EC80`; the template symbol names its owner `AnimationWrapper`.
Accordingly, the common playback/data types live in
`animation_wrapper/model.rs`; JSON/skin parsing, timeline events, typed track
sampling and shader-table decoding live below the focused `model/asset.rs`,
`timeline.rs`, `tracks.rs` and `shader.rs` modules. IDA and Hopper further
separate the two asset resolution entries (`sub_1000115A0`, 508 bytes/21 basic
blocks, and `sub_1000118F8`, 332 bytes/11 basic blocks) from their common scene
constructor (`sub_100010340`, 1,580 bytes/62 basic blocks). The nine-line Rust
asset facade therefore delegates JSON/hierarchy recovery to
`asset/loading.rs`, companion skin resolution and parsing to `asset/skins.rs`,
and live-state installation to `asset/runtime.rs`. The adjacent
hierarchy/affine transform, skin attachment, bounds and render-command stage
now lives in `animation_wrapper/transform.rs`. This preserves the native
data-to-draw dependency direction without retaining a 1,118-line aggregate
file. The complete Lua method table is coordinated by
`animation_wrapper/registration.rs`, with a typed context capturing the
animation runtime, asset root, render bridge, sprite geometry and
missing-method audit state. Resource, playback, scene/draw, query and fallback
closures live in the adjacent `registration/` modules. The coordinator invokes
them at the original global installation point, preserving closure ownership
while following the complete `sub_10000EC80` constructor boundary.

Both native load entries compute their companion resource with
`filename.length() - strlen(".anim.json")` and append `.skins.json`; neither
checks that the source filename actually ends in `.anim.json`. The Rust loader
now reproduces that byte-count operation, including the `std::string`
substring behavior that clamps an underflowed count for names shorter than ten
bytes, rather than conditionally replacing the suffix. A focused regression
covers the normal, long non-matching, and short-name cases.

Hopper string xrefs recover the constructor's exact publication sequence at
`0x10000ED94..0x10000F268`: bundle/AppData loads; `close`/`closeAll`; playback
controls; wrapper translation/rotation/scale; `update`; `draw`; playback-event
registration; entity queries; `setSkin`; `getActions`; `setShader`; and finally
cache clear plus both preloads. The Rust coordinator now spells out that order
instead of retaining its earlier development-stage ordering. It is 79 lines;
the resource, scene, query and fallback units are 146, 148, 137 and 46 lines
respectively. Playback is a nine-line facade over 165-line state controls,
132-line frame advancement/event draining and a 19-line callback-registration
unit. The callback table remains shared across playback and resource-closing
installers, matching the native wrapper's scene-owned callback lifetime.

This playback split is backed by both disassemblers rather than source length
alone. IDA and Hopper measure the enclosing `sub_10000EC80` registration
constructor as 1,620 bytes; IDA resolves the `start` adapter
`sub_10001CAF0` as a separate 104-byte member, the speed setter
`sub_100013D08` as 272 bytes, the scene-resolving seek member
`sub_10001396C` as 300 bytes, and the queued six-argument Lua event drain
`sub_100016FE4` as 380 bytes. Hopper reports the same entry points and sizes
(with its seek control-flow view spanning the tiny `sub_10040E798` tail).
Accordingly, `registration/playback/controls.rs`, `update.rs` and
`callback.rs` express member ownership beneath the constructor-order facade;
they do not invent new public runtime layers.

The finer model split follows the concrete runtime members rather than JSON
field names alone. Hopper reports cached-asset population `sub_1000E2660` as
368 bytes/13 blocks, `spineEvent` decoding `sub_1000121F4` as 1,244 bytes/67
blocks, shader-table construction `sub_10006CB08` as 2,136 bytes/60 blocks,
and queued event draining `sub_1000141E4` as 696 bytes/39 blocks; the enclosing
Lua registration member `sub_10000EC80` remains a separate 1,620-byte
constructor. The former 699-line mixed model is consequently a 111-line data
facade plus 263-line asset, 195-line timeline, 92-line track and 56-line shader
units, while the same runtime maps keep load-to-sample-to-draw ordering intact.

The first PhysicsWorld structural split follows the binary's bundled Box2D
narrow-phase cluster. Hopper identifies the nearby exported
`b2Simplex::ReadCache` symbol and the leaf contact routines at `0x10085D848`
and `0x10085E590`; IDA independently reports the latter as a three-block,
148-byte packed-float circle routine and the surrounding polygon routine as a
larger multi-branch leaf. Circle/circle, polygon/polygon, circle/polygon,
edge/circle, edge/polygon, contact-feature clipping, and segment-distance
helpers are exposed through `physics_world/narrow_phase.rs`. The 18-line
facade follows the five native algorithm leaves: `circle.rs` owns
`sub_10085E590`/`sub_10085E624`, `edge.rs` owns
`sub_10085E8AC`/`sub_10085EADC`, `polygon.rs` owns `sub_10085F648`, its
`polygon/separation.rs` child owns `sub_10085FB84`/`sub_10085FD74`, and
`polygon/clipping.rs` owns the shared `sub_100860148` clipping record and leaf.
`geometry.rs` owns reusable float32 predicates and segment distance. World
stepping, contact lifetime and Lua callbacks remain in
their existing native ownership modules.

The adjacent continuous-collision group now lives in
`physics_world/continuous.rs`: simplex solve/search, the cache used by the
exported `b2Simplex::ReadCache` neighborhood, GJK core distance, the three
separation-function modes, and `sub_100861B54` time-of-impact advancement.
The recovered implementation retains its twenty outer iterations, eight
separation pushes, alternating bisection/secant roots and fifty-root cap. TOI,
two-edge rescan and dynamic-tree regressions remained unchanged after the
split.

`physics_world/broad_phase.rs` now owns the preceding embedded
`b2DynamicTree` boundary: float32 AABB helpers, the logical 40-byte node,
block-style free-list growth/reuse, fat-proxy movement, stack query order,
surface-area insertion cost, removal, refitting and balancing rotations. The
tree/proxy, fat-AABB and immediate broad-phase-drain regressions pass without
changing RenderBridge's world/contact orchestration.

Hopper places the `createPolygon` and `decomposePolygon` strings together in
the GameLua registration constructor at `sub_10002C274`; their shared physical
contour implementation is now isolated as
`physics_world/polygon_decomposition.rs`. It preserves the float32 boundary,
clockwise ear scoring, repeated-vertex contour splitting, feature orientation,
convex triangle merge limit and near-collinear simplification. Decomposition,
drawable ordering and seven-vertex fixture-limit regressions all pass.

The GameLua-to-PhysicsWorld interface is now separated at the same adapter
family. Hopper resolves `createJoint`, `createJoints`, `destroyJoint`, and
`getRayCastedObjects` references to `0x10002D0B0`, `0x10002D0D0`,
`0x10002D2F0`, and `0x10002EA68` inside `sub_10002C274`. Their Rust method
installation is coordinated by `physics_world/registration.rs`; the actual
force/time, construction, joint, track, vertex-buffer and query closures now
live in the focused `physics_world/registration/` modules.
The immediately following SceneObject mutation/query adapters live in
`game_lua/object_api.rs`. Both modules receive the same RenderBridge handle and
are invoked from the same aggregate installation phase, while the facade makes
the recovered registration and closure-capture sequence explicit. Joint,
object, ray-cast and physics regression groups all pass after the split.

The finer registration boundary is also directly visible in Hopper's string
references. `createBox`, `createCircle`, `createPolygon`, `createLineShape`
and `createNonPhysicsObject` are installed at `0x10002CFB0..0x10002D090`;
`createJoint`, `createJoints` and `createTrack` follow at
`0x10002D0B0..0x10002D0F0`; `destroyJoint`, `clearVertices` and `addVertex`
appear at `0x10002D2F0..0x10002D370`; and the two native spatial queries are
registered at `0x10002EA48` and `0x10002EA68`. IDA independently reports the
containing `sub_10002C274` as 15,028 bytes with 77 basic blocks. The new
31-line Rust facade preserves this native subsystem sequence while the largest
implementation unit is the 343-line scene constructor rather than the former
712-line mixed registration file. The query module still delegates to the
separately recovered `sub_10005411C` AABB and `sub_10005464C` ray contracts.

The same constructor is a 15,028-byte, 77-basic-block registration dispatcher
in IDA. Hopper places `drawUITextNative`, `drawString3D`, `drawRect`,
`drawTexturedRect`, `drawPolygon`, `setTheme`, `drawLine2D`,
`drawCompoSprite`, and `drawBoxNative` at references between `0x10002CD00` and
`0x10002E9C8` within it. Those bindings now live together in
`game_lua/render_api.rs`; their captured RenderBridge, ResourceRuntime,
LocaleRuntime, sprite geometry and composite-name sets are explicit in one
registration context. Theme, draw, sprite, textured and render regression
groups pass after extraction.

Hopper identifies the later PhysicsWorld extension family independently:
`renderGravityVisualsNative`, `decomposePolygon`, `objectAndTrackOverlap`,
`destroyTrack`, `getCurrentTrackAngle`, `setJointParameters`, `makeRay`,
`getObjectVertices`, `createNativeBlockExtension`, `makeLightBeam`, and
`native_applySensorForces` all reference the same constructor span from
`0x10002CDD0` through `0x10002F334`. Their Rust bindings now live in
`physics_world/extensions.rs` with one shared RenderBridge capture. Track,
joint, sensor, native-block, decomposition, ray and light-beam regressions all
pass after the move.

Theme object state is separated from theme drawing. Hopper maps
`createThemeSprite`, `removeThemeSprite`, `modifyThemeSprite`, and
`rotateThemeSprites` to `0x10002D1A0..0x10002D230`, while
`createThemeAnimation` is registered later at `0x10002EB84`. The former
530-line `game_lua/theme_objects.rs` is therefore an order-only coordinator:
the continuous generated-adapter group now has its own 20-line order facade in
`theme_sprite_registration.rs`, with create/remove/modify/rotate implemented
in four adjacent leaves matching `0x10002D1A0/1D0/200/230`; the direct LuaState member lives in
`theme_animation_registration.rs`, and their shared record layout lives in
`theme_state.rs`. Authored layer construction and theme-specific argument
decoding are independently owned by `theme_layer_parser.rs` and
`theme_arguments.rs`.
The level-file family is similarly explicit: `loadBlocksForEditing`,
`loadLevel`, `loadLevelFromAppData`, and `saveLevel` occupy
`0x10002DBB8..0x10002DC48`, with `onLoadLuaFileFail` at `0x10002EBB4`.
`game_lua/level_files.rs` is now only their order coordinator. The zero-argument
editor member, selector-zero/selector-one common loader wrappers, fixed-schema
save member and late failure callback live in `level_editor_registration.rs`,
`level_load_registration.rs`, `level_save_registration.rs` and
`level_failure_registration.rs`. Theme-object, level-save, block-editor and
AppData round-trip regressions pass after both moves.

Implementation ownership now follows the same native components, not only the
registration surface. `physics_world/joints.rs` exposes joint anchors,
prismatic geometry, packed effective-mass matrices, the `sub_100862E14`
Solve22 and `sub_100862D60` Solve33 float32/cofactor order, Lua descriptor
mirroring, and the `b2World::CreateJoint` construction path through focused
submodules. `game_lua/render_primitives.rs`
owns the CPU-side direct-sprite, rect, line and triangle-fan command builders
shared by the recovered draw adapters. The saved-level whitelist is isolated
in `level_save_schema.rs`; Lua 5.1 number/string coercion, cycle rejection and
recursive table cloning sit in `level_table_clone.rs`. The generic
`loader_registration.rs` no longer owns `loadLevel`. Joint/solver, draw and
level serialization regressions pass after these implementation moves.
Particle query parsing, definition overrides, deterministic native random
consumption, sprite selection and spawn construction now live together in
`game_lua/particles.rs`; all six particle regressions and the Catmull aiming
stream regression remain green.

Contact ownership is now explicit in `physics_world/contacts.rs`: native
collision-factor lookup, cached force multiplier, callback argument ordering,
legacy/non-legacy damage, score propagation, breakable-joint collection and
Lua descriptor compaction move together. The same module now owns collision
enter/exit dispatch, sensor overlap accounting and `insideGravity` mirroring,
so the complete native contact-listener lifecycle has one Rust owner.
`physics_world/dirt.rs` owns the associated Dirt texture lookup and component
reconstruction boundary. The 21 contact, 13 collision, damage,
breakable-joint, bird-collision and six Dirt regressions all pass after this
split.

`game_lua/script_runtime.rs` now follows the original load chain as one unit:
Lua 5.1 source/binary preparation, AppData decryption handoff, named-child
environment injection, fallback metatables, definition-pack merge/indexing,
and safe resolution across scripts, common scripts, levels, configuration and
localization roots. Script-loader, named-object, parent-traversal, SHA-1 and
sensor-state regressions pass after the move.

The small lifecycle boundaries are split as well: `game_lua/input.rs` owns the
native `g_key*` compact event buffers separately from the public key maps;
`game_lua/time.rs` owns current-time table construction and seconds conversion;
and `resource_manager/localization.rs` owns localization discovery and table
loading. Canonical `objects.world` creation and scene-entry lookup now live in
`game_lua/object_api.rs` beside the mutation adapters that consume them. Full
workspace tests and warning-free Clippy pass after these ownership moves.

IDA confirms that the GameLua constructor `sub_10002C274` uses an approximately
`0x1540`-byte stack frame, matching its role as an ordering-sensitive aggregate
registration entry rather than one feature implementation. Hopper places the
trajectory data/update names at `0x10002CBA0..0x10002CF20` and the separate
`native_drawSimulationTrajectory` registration at `0x10002DEE8`. The shared
one-body predictor, world-attribute decoding and Catmull-Rom AimStream sampling
therefore live in `game_lua/trajectory.rs`, while their registration positions
remain unchanged in the aggregate constructor. Generated adapter coercions and
diagnostic formatting now live in `game_lua/arguments.rs`. Theme argument/table
decoding, layer construction and FCVTZS index behavior are split into
`game_lua/theme_arguments.rs` and `game_lua/theme_layer_parser.rs`, next to the
separate ThemeSprite and ThemeAnimation registration modules. The
release L01 audit after these moves preserves background/foreground layering,
slingshot, tutorial hand, pig and HUD placement.

The adapter helper itself now reflects the same distinction visible in the
disassembly. Generated thunks such as one-NUMBER `sub_100088D24`, BOOLEAN
`sub_10008962C` and one-STRING `sub_100089E6C` use exact tagged-type guards;
hand-written members using Lua 5.1 `lua_isnumber`/`lua_tonumber` may instead
accept numeric strings. The former 224-line mixed `game_lua/arguments.rs` is
therefore a small re-export façade over `arguments/strict.rs`, `lua51.rs`,
`value.rs`, `table.rs` and `diagnostics.rs`. Existing callers retain one API,
but the source no longer makes strict generated adapters look interchangeable
with C-API coercion. All 297 tests, strict Clippy and the release build pass
after this ownership-only move.

The desktop façade follows the same ownership rule. The production
`stella-app/main.rs` is now a 50-line module/constant/entry coordinator, while
the software rasterizer and its pixel-level native GL compatibility tests live
under the test-only `reference_renderer` facade. The two helpers consumed by
`assets.rs` remain explicitly re-exported only under `cfg(test)`, so this
layout changes neither the release binary path nor the reference-render checks.

The completed structure pass keeps that aggregate ordering without keeping the
implementation in one source file. IDA and Hopper both report
`sub_10002C274` as 15,028 bytes with 77 basic blocks; Hopper also exposes its
large set of generated method-registration callees. The Rust coordinator now
calls ordered installers for bootstrap/input state, platform services, script
loaders, time, particles, world controls, draw callbacks and trajectory APIs.
The world-control segment is an address-annotated ordered façade rather than a
feature-group approximation. Hopper string xrefs and IDA's
`sub_10002C274` assembly agree on `requestExit` at `0x10002C770`, the water
cluster at `0x10002CAE0..0x10002CB70`, notification/gravity/aiming at
`0x10002CEC0..0x10002CF20`, the physics/origin/max-scale cluster at
`0x10002D7F8..0x10002D8D8`, editing/world scale/mouse wheel at
`0x10002D938..0x10002D998`, GL render state/alpha/clear at
`0x10002DAF8..0x10002DB88`, level/game/camera/locale members at
`0x10002E6E0..0x10002E880`, and the late orientation, parameter-table and OS
members at `0x10002E91C`, `0x10002EB64` and `0x10002F314`. In particular the
native water order is bird drag, object drag, color, then additional gravity;
the Rust facade now preserves it instead of registering object drag first.

Coordinate conversion and GL-context state live under
`world_transform_registration/`; water, lifecycle, world controls and device
queries live under `world_environment_registration/`; physics gates, camera
limits, framebuffer clear and locale refresh live under
`world_physics_camera_registration/`. The former 167-, 208- and 206-line
aggregates are short façades over focused leaves, and `world_registration.rs`
preserves the members' cross-family native relative order. All 297 workspace
tests, strict Clippy, the release build and the 11,000-frame
`audit-world-native-order-split.png` route pass with zero invoked fallbacks.
The scene-object surface follows the same ownership split: visual mutations
and their paired visibility query occupy `object_visual_registration.rs`,
while the RenderObject/nullable-Box2D getter family occupies
`object_query_registration.rs`. The coordinating `object_api.rs` is now 81
lines instead of mixing those queries with Lua-table fallbacks, and the
unrelated feature coordinator remains separate. This boundary follows the
separate GL-context member `sub_100044E30` and named RenderObject members
`sub_100044E60`/`sub_1000592C4`, rather than merely cutting at a line count.
The trajectory installer follows Hopper's contiguous data/update xrefs at
`0x10002CBA0..0x10002CF20` and retains the separately observed draw entry at
`0x10002DEE8` in the same relative registration sequence.

IDA's `sub_1000550B8` (`getWorldPoint`) and `sub_10005521C`
(`getLocalPoint`) are direct LuaState members, not permissive Lua-table
helpers. Both read an exact string, resolve the nullable body with
`sub_100061AE4`, consume two exact numeric slots as float32, and then apply the
body's `b2Transform`. The Rust query module preserves the observed
FMUL/FNMSUB/FMADD/FADD ordering with explicit float32 `mul_add`; a missing or
non-physics body becomes a recoverable Lua runtime error instead of attempting
the native null dereference. This recovered boundary moved the two functions
out of the mixed feature installer and added ABI, invalid-body, and exact
round-trip regressions.

The former 444-line object physics installer is now an order-preserving
15-line coordinator. Its native ownership clusters are
`object_motion_registration.rs` (velocity, force and impulse on `b2Body`),
`object_body_registration.rs` (body/fixture coefficients, flags and contact
invalidations), and `object_material_registration.rs` (RenderObject material,
texture and water fields). The split follows the target data structures and
recovered member families; it does not change global registration order.

The body-registration cluster is now split one level deeper along its own
native entry points. Its 19-line facade installs `density.rs` for the direct
272-byte/6-block `sub_100030D1C`, `scalars.rs` for the fixture/body coefficient
wrappers, `flags.rs` for fixed-rotation, sensor and sleep members, and
`activity.rs` for the independent 52-byte `sub_10004DB2C` SetActive wrapper
plus 172-byte `sub_10004F3D8` collision lifecycle. IDA and Hopper both report
`sub_1000411F0` at 456 bytes/16 blocks and show that it always calls the
456-byte/17-block `b2Body::ResetMassData` after changing fixed rotation. Rust
now does the same, so a prior SetMassData inertia override is discarded even
when fixed rotation is later disabled. The wrappers also retain the native
nullable-body branch: non-physics RenderObjects are not allowed to acquire
body flags, damping, activity changes, or a mirrored collisionEnabled write.
Two regressions cover the custom-inertia reset sequence and all bodyless
no-op paths.

The adjacent 229-line object-query aggregate is likewise a 19-line ordered
facade now. IDA/Hopper component analysis shows no internal calls among its
ten native entries: `appearance.rs` owns `getScale` (180/6), flip (24/1), and
the shared angle/rotation getter (24/1); `motion.rs` owns angular speed (36/4),
speed magnitude (52/3), linear velocity (228/9), and sleeping (48/3);
`world.rs` owns the independent 16-byte physics-lock predicate; and
`points.rs` owns the two 264-byte/6-block direct LuaState transform members.
The latter continue to resolve a nullable body before reading numeric slots,
but—matching instructions at `0x100055108..128` and
`0x10005526C..28C`—do not dereference its transform until after both numeric
reads. A cross-error regression therefore locks the native argument-type
error ahead of the recoverable missing-body error when both conditions occur.

The former 341-line `object_feature_registration.rs` is now an 18-line ordered
coordinator as well. IDA reports `sub_10004EF74` as
`(GameLua*, std::string*, float, float)` and shows its throwing
`sub_10005DAF8` lookup before the 39-case parameter switch; Hopper confirms the
same switch plus the generated strict string/number adapters. That recovered
owner lives in `object_parameter_registration.rs` together with the adjacent
single-store gravity category and sensor-mask members. Pivot/decoration table
traversal lives in `object_decoration_registration.rs`, the independently
scanned revolute motor vector in `object_joint_registration.rs`, and object
destruction plus flash callback installation in
`object_lifecycle_registration.rs`.

This pass also removes the fabricated `nativeParameters`, visibility, sensor
and gravity-field Lua mirrors. Parameter 22 now calls the rehosted equivalent
of `b2Body::SetActive`, destroying proxies and emitting synchronous contact
exits rather than changing `b2Fixture::m_isSensor`; the shipped Lua creation
path uses `setAsSensor` for that separate flag. Sensor modes 5 and 7 retain the
native fixed-rotation and bullet side effects. Parameter 38 now treats its
input as inertia about the body origin and subtracts
`mass * dot(localCenter, localCenter)` before storing COM inertia, matching
`b2Body::SetMassData`. Decoration amount remains the signed `FCVTZS` result,
and pivot, decoration, motor speed and parameter scalar stores all cross their
observed float32 boundary. Dedicated regressions cover strict arity/types,
unknown-object failures, pivot's Lua-write-before-native-lookup order, negative
decoration counts, endpoint wakeup, activation and off-centre mass data.

Below the Lua surface, the former aggregate `RenderBridge` implementation is
split by the Box2D execution stages visible in the disassembly: proxy/broad
phase, contact destruction and refresh, island assembly, velocity and position
constraints, joint solving, position integration and reduced TOI islands.
`SceneObject` is likewise divided into state layout, fixture/mass behavior,
sweep/body motion and collision geometry/query modules. The public facade is
54 lines, the aggregate registration coordinator is 352 lines, and the
ownership split passes all 263 workspace tests, warning-free strict Clippy, a
release build and the `audit-theme-split-start.png` wgpu start-flow
capture.
Joint solving is split once more along Box2D's concrete constraint classes:
island dispatch remains in `joint_solver.rs`, common cached/body impulse writes
live in `joint_solver/impulses.rs`, and distance, prismatic, revolute, rope and
weld equations live in independent class modules.
The public Lua host similarly delegates fixed-step physics to
`game_lua/host_physics.rs`, while rehost scene ownership and the live Lua
collision-material view used by the native contact filter live in
`game_lua/host_scene_sync.rs`.
Its remaining facade is now split along the executable's member boundaries:
two-stage startup is in `host_startup.rs`, `sub_10005E898` update and
`sub_10004BAB4` draw dispatch are in `host_frame.rs`, desktop pointer-to-key
translation is in `host_input.rs`, and bridge queries/drains are in
`host_output.rs`.
The large SceneObject and render adapter sources are now order-preserving
coordinators too. Object registration is divided into transform/shape,
physics/material, parameter/decoration/joint/lifecycle and visual/query groups.
Render registration follows
the Hopper string clusters for theme setup, immediate primitives, UI text,
textured/masked draws and direct sprite/composite draws; `render_api.rs` now
contains only the typed capture context and their original call order.
The later PhysicsWorld extension table is split by the same adjacent string
families: track/joint mutation and queries, scene-object/sensor extensions, and
the native block/Dirt method table. `extensions.rs` retains the original
sequence and the final decomposition/ray/light-beam/gravity-visual group.

The physics implementation now follows the same ownership rule below the Lua
surface. `physics_world/dirt.rs` coordinates the complete native dirt pipeline;
its focused children own float32-to-1000x integer conversion, the recovered
0.785000026f cut octagon, pure-Rust Clipper compatibility, short-path rejection
and reversed quality-ranked ear cutting. `physics_world/sensors.rs` contains
`sub_10005DE90` gravity/water
masking, buoyancy and drag. `physics_world/ray_cast.rs` owns fixture entry-face
tests and proxy AABBs, while `physics_world/tracks.rs` owns strict first-tie
closest-edge projection. Joint state moved beside joint construction/solvers;
contact callback data moved beside damage and dispatch; TOI proxies, sweeps,
transforms and cache state moved beside GJK/continuous advancement. Particle
state and the fixed CMWC stream now live in `game_lua/particles.rs`, and sprite
geometry/placement plus native textured-line and rubber-band construction live
in `resource_manager/geometry.rs`. The public script-to-wgpu command ABI is
isolated in `render_types.rs` while retaining the original crate-root exports.

The direct Lua method `native_startURLThread` (`sub_100032688`) is also no
longer a permissive first-string scan: slot 1 is a strict URL string, slot 2 a
strict Lua function retained as the asynchronous callback, and slot 3 is type
checked as boolean only when the total argument count is exactly three. With
four or more arguments the native flag is forced false and slot 3 is not
examined. It returns zero Lua values; the offline host retains the callback and
request metadata without attempting to resurrect the discontinued service.

The main constructor builds `commonScriptPath + "/gamelogic.lua"`. That shared
chunk itself loads `scriptPath + "/game.lua"`, so the Rust `StellaLua::boot`
path evaluates the common chunk once and lets it perform the game-specific
load in the same `gamelua` environment. The constructor also installs the
persistent `settings`, `highscores`, and `bi_data` tables before this point.

The same constructor, at `0x100027114..0x100027160`, loads
`scriptPath + "/starLimits.lua"` into an object named `starTable` before
`gamelogic.lua`. `level_load.lua` writes the selected level's silver/gold score
limits into this table during its first playable-level setup, so omitting this
otherwise implicit native load fails only after the intro and chapter map.

## Update, draw and native scene bridge

`sub_10005E898` does **not** pass elapsed time to Lua. It saves the incoming
unscaled frame delta as `v230`, multiplies it by the engine time multiplier into
`v238`, then invokes `update(v238, v230)`: scaled delta first and raw delta
second. Reversing the second argument with elapsed time makes
`gameCamera.cameraAnimation` multiply its spring by about 91 seconds after the
first shot, causing the observed alternating `+/-` scale explosion and NaNs.
The Rust host now reproduces the scaled/raw ABI. A later whole-function audit,
recorded below, also establishes that these remain callback parameters: the
native dispatcher does not manufacture `deltaTime` or `currentTimeStep`
globals before the call.

The same native dispatcher accumulates scaled time at engine offset `+1308`.
For every accumulated `0.033333` seconds it calls Lua
`updatePhysics(0.033333)` and advances the native physics world. The rehost now
uses this 30 Hz schedule, applies gravity/force/impulse/damping integration,
shape-derived mass and rotational inertia, the recovered motion clamps and
sleep state machine, then writes x/y, linear velocity, angle, angular velocity
and sleeping state back into `objects.world` before the ordinary Lua update.

`sub_10004BAB4` is the native draw dispatcher. It interleaves C++ scene nodes
with Lua `DrawCalls.draw(z)` callbacks. Registration in `sub_10002C274` maps:

- `native_setZOrderRange` to `sub_10004BAA8`, which stores two 32-bit z bounds
  at engine offsets `+1576` and `+1580`;
- `drawGameNative` to `sub_10004BAB4`;
- `drawBackgroundNative` to `sub_10004C4A4`.

The pre/post scene callback setters store Lua functions at scene-object
offsets `+344` and `+352`. Immediately around the native object draw,
`sub_100528834` retrieves the original Lua object table from its registry
reference; the dispatcher calls the callback with that table and the object
byte at `+0x139` as a boolean. In particular, the first argument is not the
object name string. This distinction is required by IslandMap callbacks that
read `object.x` and `object.y` directly.

The rehost mirrors native objects created by `createNonPhysicsObject`, box,
circle, polygon and line constructors, applies position/scale/angle/alpha/
visibility updates, filters them through the recovered z range, and emits
renderer-neutral commands. The recovered IslandMap camera uses a 20:1
physics-to-world conversion, top-left `(-163.6303, -100.3015)` and scale
`5.61361`; these values are supplied by the original scripts through
`setTopLeft` and `setWorldScale` rather than hardcoded for the level.

World-object sprite scale is not just `setWorldScale`. `setObjectParameter`
dispatches through `sub_10004EF74`: parameter 5 writes the same scalar to
the render/object scale fields, while parameters 17 and 18 update the
corresponding axes independently. These cases do not recreate or resize a
Box2D fixture; only the separate `setPhysicsScale` path above does so for its
supported shapes. Parameter 8 sets the horizontal-flip byte at `+313`. In the object draw helper
`sub_10006D5B4`, the field at `+0x100` must equal 2 before the engine's
`gameWorldScale` is applied. This field is not the Box2D body type: every
constructor initializes it to -1, and parameter 21 in `sub_10004EF74` is its
only writer. The helper then requires object byte `+0x147` to be clear; the
circle constructor sets this byte, while box/polygon/line/non-physics
constructors clear it. The render bridge now preserves both conditions. This
is observable in Chapter 01 Level 01: ordinary wood blocks set parameters
5/6/7 but not 21, so native rendering does not multiply them by
`gameWorldScale`; its foliage uses parameter-5 scale `0.10`, and its medium pig
uses `0.09` plus horizontal flip. The explicit sprite matrix at
`sub_10006C838` keeps the sprite angle and places that flip only in the X
scale; the separate callback render state uses the native negated-angle branch.

The rest of `sub_10004EF74` is now represented as state rather than an opaque
parameter map. The numeric names were recovered from the bundled
`ObjectParameters.lua`: 1 marks a level goal; 2 changes static/dynamic type;
6/7 store bounce multipliers; 9/11/12/15 store the flip, not-collided,
ignore-motion and immovable-collision flags; 16 selects the native gravity
scale table `{1.0, 0.0}`; 20/22 distinguish sensor definition from live
fixture-sensor state; 21 and 24 store sensor type/shape; 25-29 and 35 store
sensor force/size data; 31 stores collision time; 32 inserts the object into
the aiming-aid collision collection; 33/34 control visibility and bubble
state; 36 stores the collision group; 37 selects kinematic versus dynamic;
38 replaces Box2D mass-data inertia; and 39 accepts explicit static,
kinematic or dynamic body type. Cases 3/4/10/13/14/19/23/30 really do fall
through the executable's default case. The Rust solver now keeps kinematic
bodies moving while retaining zero solver inverse mass, and body-type changes
apply Box2D's velocity/force reset and unit-mass fallback behavior. The body
pointer guards are preserved as well: parameters 2/16/21/22/37/38/39 do not
mutate non-physics objects, so a non-physics parameter-21 call cannot
accidentally activate the `+0x100 == 2` render-scale branch.

Parameters 6 and 7 also feed a native collision squash/stretch path rather
than remaining passive metadata. `sub_100062520` converts collision impulse to
`min(impulse * 0.02f, 0.1f)` and starts the animation when this exceeds its
current amplitude. During `sub_10005E898`, elapsed time linearly decays that
seed, parameter 6 multiplies the amplitude, and parameter 7 contributes
`value * 5` to a phase whose other term is `amplitude * 100`. The ordered
object index adds `index * pi/2`; opposing sine values offset X and Y fixture
scales. Animation ends below `gameWorldScale * 0.01`. The rehost now mirrors
this whole float32 path once per rendered frame after all fixed physics steps,
including the executable's
0.02/0.1/4.0/5.0/100.0 constants and its reset fields.
`setGameParameters` also rounds `gameWorldScale` through the native float32
table reader before storing GameLua `+0x194`; retaining a Lua double there
would perturb both the parameter-21 scale and bounce cutoff.

The adjacent object-frame pass also consumes several parameters that were
previously only stored. `P_IGNORE_MOTION` at `+0x13c` excludes an object from
the thresholded `hasMovingObjects` and zero-tolerance
`hasMovingObjectsZeroTolerance` globals without disabling Box2D solving,
sleeping or `hasAwakeObjects`. The native thresholds are linear speed squared
`> 9.0f`, absolute angular speed `> 0.1f`, and linear speed squared `> 0.0f`;
the zero-tolerance flag additionally excludes controllable objects. Byte
`+0x145` caches Box2D's awake bit, retaining a just-slept object for one final
reporting frame. Controllable objects whose `P_TIME_SINCE_COLLISION` value at
`+0x128` is non-negative accumulate scaled frame time in float32, and the
one-controllable bird-collision path changes a negative value to zero.
`P_NOT_COLLIDED` at `+0x13b` is narrower than its name suggests: its only two
reads in `sub_100062520` suppress collision squash/stretch for the respective
side when both objects are controllable. It does not suppress ordinary
bird/block or block/block contacts. `P_DISABLE_IMMOVABLE_COLLISIONS` at
`+0x13d` has constructor and setter writes but no reader anywhere in the
GameLua executable range, so retaining it as inert state is the exact Purple
1.1.6 behavior.

`setFlashAnimation` (`sub_10004F4D4`) marks the native scene object at byte
offset `+300` and installs the animation draw callback at `+384`;
`removeFlashAnimation` (`sub_10004F588`) clears both. The callback at
`sub_10006794C` applies the scene object's position, rotation and scale before
drawing the Flash animation whose tag is the object-name string at offset
`+96`. Its translation is `(objectPosition * 20 - topLeft) * worldScale`; its
scale uses the object's visual scale and `worldScale`, but deliberately omits
`gameWorldScale`. Horizontal flip changes only X scale, not rotation. The
rehost now reproduces that callback inside `drawGameNative`, so animated birds
and effects replace the ordinary placeholder sprite instead of disappearing.

The low-level render helpers were also recovered from their direct and adapter
entries. `drawTexturedRect` (`sub_100043D6C`, adapter `sub_100085230`) takes a
sprite name, four floats and a boolean. Its boolean is passed but unused; the
direct path replaces the live GL state with the native default block and
leaves it installed, then truncates destination X/Y and the two edge
differences independently. Its final GL-context virtual at slot `+152` is
`nullsub_298` (a single `RET`) in Purple's only concrete GLES2 context, so this
v1.1.6 entry intentionally submits no geometry. Those side effects,
conversions and no-op submission are now mirrored. `drawRect`
(`sub_100043C14`, adapter `sub_100085450`) is a different nine-argument path:
eight floats followed by a required boolean. A false boolean installs and
retains the default GL state; normalized RGBA is multiplied by 255, converted
with `FCVTZS` and packed by low byte; destination X/Y and width/height are also
converted independently. `GL_Context::drawRect` (`sub_100598CC4`) then applies
the complete live scalar matrix and state alpha to a four-vertex triangle
strip. The bridge now preserves all of these operations instead of emitting an
untransformed, loosely typed rectangle. `drawSelectedTexturizedObject`
(`sub_100043990`, adapter `sub_100085634`) takes sprite, texture, x/y and x/y
scale. It installs translation `-topLeft / argumentScale` and scale
`worldScale * argumentScale` into the live GL state, preserves angle/pivot and
alpha, then sends position `worldPosition * 20 / argumentScale` to the cached
texturized-object renderer. The Rust command now keeps that divided local
coordinate/state representation (while producing the same final screen
position) and retains the native state side effect. `renderMaskedImageNative`
(`sub_1000343CC`, adapter `sub_100087FC0`) takes one resource string and nine
numbers. The first eight numbers are independently truncated coordinates; the
ninth is a UV factor, not alpha. It resets translation, scale and rotation but
preserves pivot/alpha/clip, and submits Purple's exact two-triangle position and
reverse-Y UV pairing against the backing full texture. `drawString3D`
(`sub_10003457C`, adapter `sub_100087BB4`) takes two strings and seven floats.
Its angle is an X-axis model rotation followed by the recovered perspective
matrix (`-1.5`, `0.001`, `2000`, `-1.33`), not an ordinary 2D Z rotation; CPU
reference and wgpu atlas-glyph paths now share that projection.

`drawUITextNative` is the direct Lua entry at `sub_100031434`. Its first three
arguments are a strict table and parent X/Y. Parent X/Y scale is read only
when both arguments 4 and 5 exist (a lone fourth argument is ignored), angle
defaults to zero, and alpha defaults to an unsupplied state rather than the
table's `alpha` field. A false/missing `visible` exits before font selection.
The local x/y position is composed through the parent's non-uniform
Scale * Rotation basis and the local x/y scales are multiplied by the parent
scales; the table's own angle is not read. A missing/non-string font selects
`FONT_BASIC_SPACE`. For `clipped=true`, the native code iterates `lines` until
nil and invokes each inherited `line:draw(finalX, finalY, finalScaleX,
finalScaleY, angle)` method. An explicit alpha is installed for those calls
and then reset to exactly 1, including the error path in the rehost.

IDA's exception-inclusive view reports this entry as 3,704 bytes/83 basic
blocks (complexity 42), while Hopper keeps a 3,692-byte/79-block principal
body. The Rust file now mirrors its internal control-flow boundary without
inventing extra public natives: the 154-line entry retains argument decoding,
visibility, font selection and transform composition, then dispatches to a
63-line `ui_text_registration/clipped.rs` callback branch or a 116-line
`ordinary.rs` bitmap-font/state branch. IDA also isolates the downstream
bitmap-font virtual `sub_10042B338` at 1,068 bytes/61 blocks (Hopper 1,060/58),
supporting the separation of glyph submission from the Lua-facing adapter.

The ordinary text path strictly reads width, horizontal/vertical anchors,
group and text, accepts a rotation pivot only when both pivot fields are
numeric, and optionally floors `finalPosition / finalScale`. It leaves the
composed scale, angle, rotation basis and pivot installed in the live render
state. Only an explicit alpha below one temporarily replaces the prior alpha;
that replacement is again reset to exactly 1. ResourceManager localizes the
group/key before BitmapFont submission. The bitmap-font virtual at
`sub_10042B338` sets per-glyph context translation and draws each AtlasSprite
at the live scale—there is no second host-side 0.5 density factor. The Lua UI
line objects already contain that authored 0.5 scale. CPU reference and wgpu
now consume the same exact text affine matrix, including non-uniform
Scale * Rotation and pivot correction.

The independent `ResourceManager.drawString` adapter at `sub_100448728`
strictly consumes group/key strings and x/y numbers. Its fifth and sixth
arguments are not fixed horizontal/vertical slots: both pass through the same
anchor mapper, so either position may provide a horizontal or vertical anchor
and the defaults remain `LEFT/TOP`. The current font is mandatory and the
group/key pair is localized before `sub_10045C1FC` submits the bitmap glyphs.
Unlike an earlier compatibility approximation, the draw origin and every
glyph now inherit the complete live renderer translation, pivot, non-uniform
scale and rotation matrix without mutating that state.

`clipText` is the strict `(group, key, maxWidth)` direct entry at
`sub_10004F630` (adapter `sub_100086070`) and returns no Lua values. It first
localizes the group/key pair and requires the current font, then walks Unicode
codepoints using that font's glyph advances. Native break opportunities are
line feed, space, hyphen and U+200B: a hyphen remains on the preceding line,
whereas space and zero-width space are omitted and skipped. A candidate whose
width is equal to the limit already wraps; an overlong first word is split at
the first glyph that reaches or exceeds the limit. The resulting global is
`clippedText = { widestLine = ..., lines = {...} }`. The Rust bridge now
reproduces those boundary, separator and forced-split rules rather than the
former single-line `character-count * 16` estimate.

System-font enumeration is separate from ResourceManager font aliases.
`getAvailableSystemFonts` dispatches to `sub_1004482D0`, whose platform body
at `game::SystemFont::Impl::getAvailableFontNames` walks
`UIFont.familyNames`, then every `fontNamesForFamilyName:` result, and caches
those face/PostScript names process-wide. The rehost now performs the same
face-name enumeration through a cross-platform system font database; creating
a resource named `SYSTEM_FONT` no longer incorrectly makes that alias appear
as an installed OS font.

The actual system-font object is constructed by `sub_1004756AC` and
`game::SystemFont::Impl::Impl` (`0x100477668`). The Lua size first passes
through `float` and `FCVTZS`; `UIFont fontWithName:size:` must succeed, and any
style other than zero throws (the native style labels are `Normal`, `Bold`,
`Italic`, then `Unknown`). Ascender, negated descender and leading are each
truncated to integers at construction. The tiny virtuals at `0x1004758D4`,
`0x1004758E4`, `0x1004758F0`, `0x1004758FC` and `0x100475908` return
ascender+descender, ascender, descender, leading and constant-zero tracking.
Unicode width at `0x10047692C` calls `NSString sizeWithFont:` and truncates the
result. The Rust implementation now resolves the real platform face, derives
those integer metrics and glyph advances from its TrueType/OpenType data, and
applies legacy kerning instead of the former `size * characterCount * 0.5`
placeholder. `sub_10045987C` also proves that its final stack byte is a force
replacement flag: without it, an existing alias is returned without even
constructing the newly requested font. Both system-font creation adapters now
preserve that lifecycle rule.

The Rust font implementation now follows those three independent owners. IDA
and Hopper place ResourceManager `clipText` at `sub_10004F630` (IDA
1,504 bytes/92 blocks including its tail, Hopper 1,500/91), system-face
enumeration at `sub_1004482D0` (176/4 in both), the outer SystemFont allocator
at `sub_1004756AC` (216/4), the UIKit-backed implementation constructor at
`0x100477668` (IDA 976/42 including exception paths, Hopper principal body
736/8), and Unicode width at `0x10047692C` (288/13 in both).

The former 316-line `resource_manager/fonts.rs` is therefore a 21-line facade
over 76-line shipped FONT loading/bitmap metrics, 79-line native clipText
splitting, and 156-line cross-platform SystemFont enumeration/construction/
width modules. The public registration API is unchanged; bitmap glyph width,
break opportunities, f32-to-FCVTZS sizing, kerning and metric regressions all
pass after the move.

`drawBoxNative` (`sub_100051BBC`) is the nine-slice reader for `topLeft`,
`topMiddle`, `topRight`, `left`, `center`, `right`, `bottomLeft`,
`bottomMiddle` and `bottomRight`. It resolves `HCENTER`/`RIGHT` and
`VCENTER`/`BOTTOM` before submission, then draws in native order: top/bottom
middle, left/right, four corners, and finally center. An optional tenth color
table replaces the center sprite with a default-state packed-color rectangle
over the full requested box, with the native one-pixel right/bottom reduction.
This key and order recovery fixes the settings panel's formerly absent middle
edges.
`drawPolygon` is the strict table/position/RGBA entry at `sub_100043F28`; it
constructs a mode-1 `DrawablePolygon`, rebuilds its float32 vertices through
the native ear cutter, submits a transformed triangle list, and then closes
the original contour with an opaque black one-pixel outline. Local and offset
positions retain the native 20:1 scale. `drawLine2D` (`sub_10004DC44`) and
`drawRectLines` (`sub_10004DC8C`) share the strict
x1/y1/x2/y2/width/RGBA adapter at `sub_100084918`. Their context implementation
(`sub_100598890`) uses a four-vertex triangle strip and computes integer width
as the direction-squared weighted combination of live X/Y state scales before
normalizing in screen-aspect NDC. Current state alpha is applied to both paths.

`drawTexturedLine2D` is a separate native path: adapter `sub_100084A9C`
strictly reads a resource string followed by nine floats, but direct entry
`sub_10004DB90` and its quad builder `sub_10006DB0C` use only endpoint
x1/y1/x2/y2 and width. The four trailing floats are an inert ABI remnant in
v1.1.6, including the apparent alpha argument. The recovered implementation
now matches the native one-pixel transformed-length cutoff, state
translation/rotation/pivot/scale application, signed width, triangle-strip
vertex order and its asymmetric non-uniform-X-scale normal calculation rather
than approximating the call with a centered rotated sprite.
`drawRubberband` is not the same ABI: adapter `sub_1000897A4` reads five
floats first and the atlas name sixth. Direct entry `sub_100030EB0` bypasses
the scalar render-state transform, constructs the four band vertices itself,
and maps the sprite X axis across the signed width while mapping its Y axis
along the segment. Its arithmetic is also deliberately mixed precision: input
subtraction, `sqrt`, the `3*pi/2` addition (raw constant `0x4096CBE4`) and the
opposite-edge sub/add sequence are float32, while `atan2`, both `sincos` calls
and the endpoint FMAs run in double before rounding back to float. It submits
even an exactly zero-length band as two degenerate triangles. The rehost now
preserves that native argument and UV-axis order and carries all four
independently rounded corners into wgpu instead of reconstructing the fourth
corner from an affine approximation.

`setRenderState` (`sub_100044CFC`) consumes translation x/y, scale x/y,
angle, pivot x/y and alpha in that order, but its arity updates are grouped:
two arguments update translation, four additionally update scale, five add
angle, seven add pivot and eight add alpha. Missing groups preserve the
previous native state; a lone third or sixth argument is not consumed.
Hopper and IDA also show every consumed slot passing through
`sub_10052859C` into `s` registers. Each value is now narrowed to float32, and
the paired translation/scale/pivot reads complete before their packed stores.
Thus a bad second value cannot half-write its pair, whereas an error in a
later group leaves the earlier groups committed exactly as the nested native
branches do.
The adjacent alpha names are not aliases. Registration at `0x10002DB28`
routes `native_setAlpha` through the strict one-number adapter
`sub_100088D24` into `sub_100044E30`, which resolves the current GL context and
stores one float at context `+0x40`. `setObjectAlpha` instead uses the strict
string/number adapter `sub_1000866F8` and `sub_100044E60`, which resolves a
named render object and stores the float at object `+0xC8`. `changeZOrder`
shares that string/number adapter but dispatches `sub_1000592C4`: it resolves
the object first, moves its name between the integer z-order buckets, writes
the reflected `z_order` attribute and stores the float at object `+0xD4`.
`sub_10005DAF8` throws `Missing object: %s` for both named-object operations.
The Rust bindings are now distinct, strict and float32-quantized, including
the same no-partial-write behavior on bad or missing arguments and the same
unknown-object failure. Context alpha remains with
`world_transform_registration.rs`; object alpha/z-order/visibility are split
into `object_visual_registration.rs` rather than growing the mixed object
feature installer.
The following constructor entries at `0x10002E0F8..0x10002E13C` register
`setVisible` and `isVisible` separately. `sub_1000859F4` strictly reads a
string and boolean before `sub_10004CB94` writes the RenderObject byte at
`+0x14A`; `sub_10004CBB8` returns that same byte through the one-string result
adapter. Neither member accesses the Lua mirror, and both first call
`sub_10005DAF8`, so an unknown name throws instead of becoming a silent no-op
or false result. The rehost now keeps this native-only state, exact boolean ABI
and failure contract; this also removes `isVisible` from the generic
object-table query module and places the pair beside the other render-object
visual members.
The transform/query block at `0x10002D3FC..0x10002D6EC` has two different
lookup contracts. `setPosition`, `setScale`, `getScale`, `setAngle`,
`setRotation`, `getAngle`, `getRotation` and `isHorizontallyFlipped` ultimately
use throwing `sub_10005DAF8`; their generated adapters strictly consume the
complete string/float tuple. Position and scale store float32 x/y fields,
while angle first calls `fmodf(value, PI+PI)` and adds the float32 sum only for
a strictly negative remainder. The horizontal query reads object byte
`+0x139`, written by ObjectParameter 8, rather than testing the sign of the Lua
`scaleX` mirror. The velocity/sleep queries instead use nullable body lookup;
`isSleeping` returns true when no body exists. The rehost now preserves each
boundary, float32 result and missing-object branch. `getPosition` has no
matching Purple string or registration and has therefore been removed instead
of retaining a host-invented global.

The setter half of that constructor block is now split at the same native
member boundaries instead of remaining in the former 335-line mixed transform
installer. `setPosition`, `setRotation`/`setAngle` and `setScale` are installed
by the order-only `object_pose_registration.rs`; `setPhysicsScale`
(`sub_10004050C`) is isolated in `object_physics_scale_registration.rs` because
it crosses into fixture destruction/reconstruction. IDA confirms that it first
performs the throwing native lookup, then applies ordinary visual scale, and
only afterwards rebuilds a polygon or circle fixture. Polygon width/height are
reflected before live density/friction/restitution reads; circle scale uses
`abs(min(scaleX, scaleY) / definitionScale) + 0.0001f`. Thus an unknown object
cannot partially update Lua scale, a bodyless object keeps the visual change,
and a bad fixture coefficient fails after the observed visual/dimension writes
but before reconstruction.

The two former outliers have moved to their actual owners as well.
`native_setDensity` (`sub_100030D1C`) consumes the Lua stack tail in
number-then-string order, changes only the head fixture density, resets mass,
and reflects `objects.world[name].density` last; it does not overwrite the
object's retained creation/fixture-definition density. `native_setSprite`
(`sub_10004C7FC`) strictly consumes two strings and updates native render state
without fabricating a Lua `sprite` mirror. These contracts, including error
write order and bodyless/unknown-object behavior, are locked by dedicated ABI
regressions. The transform split passes all 263 workspace tests, warning-free
strict Clippy, a release build, and the wgpu menu/start-flow captures
`audit-transform-split-menu.png` and `audit-transform-split-start.png`.

`drawSprite`'s adapter at
`sub_1004483AC` defaults to `HPIVOT/VPIVOT`; the ordinary-sprite path at
`sub_100467A00` subtracts the atlas pivot exactly once. The state pivot must
therefore not be subtracted a second time as another atlas anchor. Direct
inspection of `gr::gles2::GL_Context` (`sub_100598CC4`) shows that it instead
uses the state pivot as the rotation center: it adds `(I - R) * pivot`, then
projects with independent X/Y scales. Its scalar linear transform is thus
`Scale * Rotation`, including under non-uniform scale. Both CPU and wgpu paths
now use this exact order and pivot correction. The register-level paths are
single precision as well: the context loads `s` registers, composite/direct
sprite setup calls `sincosf`, `sub_10001E440` multiplies affine matrices with
`fmadd`, and `sub_100467BE8` transforms each point with a fused product-sum
followed by a separate translation addition. The render bridge therefore
quantizes Lua doubles to float32 at the native boundary and preserves that
FMA/add order through wgpu vertex generation; it no longer calculates an
extra-precise f64 transform and rounds only at upload time. The test-only
software sampler widens the already-composed float32 matrix solely while
walking integer pixels.

The deferred atlas representation must keep those two operations separate as
well. `sub_100467A00`/`sub_100467AF0` convert `HPIVOT/VPIVOT` to the raw
rectangle origin `(x - atlasPivotX, y - atlasPivotY)` before calling the
context, while `sub_100598CC4` independently applies the live state pivot.
Ordinary atlas commands now store that raw origin and override their deferred
vertex pivot to zero. Previously the wgpu vertex builder subtracted the SPRT
pivot again, producing an extra `(I - R) * atlasPivot`; at a quarter turn this
moved `REWARDWHEEL_BASE` about 300 screen pixels away from its reward slots and
separated `ICON_OFF`/coin images from their child labels. A synthetic shipped
RewardWheel layout at `pi/2` now keeps the 452x454 base centered with all eight
rotated slot images on the same ring.

The adapter
also accepts `TOP/VCENTER/BOTTOM/BASELINE/VPIVOT` and
`LEFT/HCENTER/RIGHT/HPIVOT`, throws on invalid names, and its final two-float
overload stretches ordinary atlas sprites while `sub_10045C144` ignores those
dimensions for composite sprites. Dot versus colon form is selected solely by
whether Lua argument 2 is numeric; the chosen sprite/x/y triplet and the two
optional anchors are then strictly typed. A lone target-width argument is
intentionally ignored without being type-checked, while a complete width and
height pair is strictly numeric. Both Lua forms now preserve these contracts.
`drawLayer` is intentionally empty: both disassemblers
resolve its native registration to `nullsub_13`, so implementing visible
behavior there would be less faithful than a no-op.

The global GameLua `drawCompoSprite` is not the ResourceManager composite
draw wrapper. Its five-argument adapter (`sub_100085BB4`) reaches
`sub_10004DDA0`, which iterates only directly referenced atlas sprites. For
each record it uses the record x/y plus the two call-site scales, ignores the
record scale, angle, flip and visible flag, writes renderer pivot
`(atlasPivot - recordPosition) * localScale`, then draws the destination-sized
atlas rectangle with `HPIVOT/VPIVOT`. The rehost emits the equivalent exact
`Scale * Rotation * LocalScale` affine command per child and preserves the
renderer pivot side effect; nested composite references are skipped just as
the native null AtlasSprite pointer is skipped.

The containing native registration block also fixes the structural boundary
and order of this direct-render family. At `0x10002E354..0x10002E40C`,
`sub_10002C274` registers `drawCompoSprite` (`sub_10004DDA0`),
`drawSpriteWithShader` (`sub_10004E070`), `drawSpriteWithoutShader`
(`sub_10004E300`) and `isCompoSprite` (`sub_10004E3A0`) in that order, with
four different Lua adapters. The former 211-line mixed Rust installer is now
a 39-line order-only facade over `direct_sprite_registration/composite.rs`,
`shader.rs`, `plain.rs` and `lookup.rs`; the atlas-only and composite-fallback
rules no longer share one implementation body.

The texture-state path at `sub_10018AFB8` fixes `GL_TEXTURE_MAG_FILTER` to
`GL_LINEAR` (`0x2601`); IDA and Hopper produce the same four `glTexParameteri`
calls. Its minification and wrap values are also recovered rather than
guessed: `sub_100206CA0` maps non-mip states to `GL_LINEAR`, state 2 to
`GL_NEAREST_MIPMAP_LINEAR` and state 3 to `GL_LINEAR_MIPMAP_LINEAR`, while
`sub_100206CE0` maps state 2 to `GL_CLAMP_TO_EDGE` and the other states to
`GL_REPEAT`. All 72 shipped image/font PVR v2 headers have zero mip levels, so
their atlas state takes the complete non-mipmapped linear route. Ordinary
atlas and mask lookup uses the recovered clamped state, while texturized fill
uses repeat. `sub_100467760` constructs the four UV pairs from the unmodified atlas
rectangle edges divided by the full texture dimensions; it does not inset or
clamp individual regions. The `wgpu` renderer therefore uses full-texture
clamped linear sampling for atlas sprites and repeated linear sampling for
texture masks, matching fractional camera-scale sampling and padded atlas-edge
behavior.

IDA's `gr::gles2::GL_State::begin(bool)` at `0x10059ACE8` and Hopper's
independent pseudocode agree on the render-state cache: `0x0B44` is culling,
`0x0B71` is depth test and `0x0BE2` is blending; source/destination factors at
state offsets `+0x18/+0x1C` are forwarded unchanged to `glBlendFunc`, followed
by `glBlendEquation`. The bundled `2d-*.fx` files complete the state oracle.
All 2D sprite passes disable depth, two-sided variants disable culling,
`2d-sprite-alpha` and the colorize/silhouette/gold/diffuse variants select
`ONE, ONE_MINUS_SRC_ALPHA`, while `2d-sprite-alpha-masked` and plain-alpha
geometry select `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`. `pp.ps` also proves that
`ALPHA_FACTOR` multiplies the complete fragment vector. The replacement now
encodes those exact combinations as separate `wgpu` program pipelines with no
depth attachment and no culling, rather than applying a single CPU source-over
rule.

The program identity boundary is explicit too. IDA and Hopper agree that
`GL_Context::{getPlainShader,getPlainAlphaShader,getSpriteAlphaShader,
getSpriteShader}` are four independent 380-byte/19-block lazy getters at
`0x10059B570`, `0x10059B798`, `0x10059ED78` and `0x10059EFA0`. They cache the
fixed `2d-vertexcolor`, `2d-vertexcolor-alpha`, `2d-sprite-alpha` and
`2d-sprite` programs in successive context slots `+0x338..+0x350`. Prepared
Rust draws now retain `Plain`, `PlainAlpha`, `Sprite`, `SpriteAlpha` or the
separate bundled `SpriteAlphaMasked` identity through submission. Programs
that happen to share opaque or straight-alpha state therefore no longer
become indistinguishable before the wgpu pass.

The ordinary atlas wrapper closes the selection rule that feeds those getters.
In `sub_10059E254` and `sub_10059E68C`, the renderer obtains the current
`GL_State` through the context vtable slot `+0xC0`, obtains the texture's
`img::SurfaceFormat` through its vtable slot `+0x38`, and selects
`SpriteAlpha` exactly when `sub_1004DC4B8(format)` is true or the draw-state
float at `+0x40` is below `1.0f`; otherwise it selects `Sprite`. The `+0x40`
member is therefore state alpha, not a cached texture-alpha value. The compare
is a strict float32 `<`, including its behavior for values immediately below
or above one.

IDA and Hopper independently recover `sub_1004DC4B8` as a SurfaceFormat
identity predicate: bits in `0x039F6048`, formats `0x1A..0x1C`, and formats
`0x1E`/`0x20` carry alpha. The indexed name table at `off_100AA5D98` supplies
all identities from `UNKNOWN` through `ETC1_RGB_4BPP`. The PVR reader
`sub_1004D7BC8` maps the two shipped PVR-v2 codes `0x10` and `0x12` to native
formats 18 (`R4G4B4A4`) and 6 (`A8B8G8R8`) respectively. All 72 shipped PVRs,
all eight shipped RGBA PNGs, and all 14 shipped WebPs therefore take the
alpha-program branch, but the opaque branch remains required for the native
contract and non-shipped inputs.

The Rust resource boundary now mirrors that ownership instead of inspecting
decoded RGBA pixels. `stella-assets::surface_format` owns the complete native
enum, exact alpha predicate, PVR flag mapping and GL-upload normalization;
`stella-assets::native_image` owns the PNG/WebP source-layout probe; and
`TextureAsset` keeps an `RgbaImage` together with the source layout and exposes
the normalized upload SurfaceFormat. `gpu/program.rs` owns the recovered
program-selection member. Atlas sprites, native/explicit quads,
bitmap glyphs and captured RGBA targets all carry that decision through their
prepared wgpu draw. Supplying an explicit bundled pixel shader remains a
separate route, matching `sub_10059EAA4` rather than being overwritten by the
ordinary automatic selector.

IDA and Hopper agree on the reader/converter boundary. The image-reader
constructor `sub_1004D302C` initializes SurfaceFormat fields at `+0x458` and
`+0x45c`; PNG reader `sub_1004D62B8` selects `L8`, `B8G8R8`, `P8`, `A8L8` or
`A8B8G8R8`, and palette PNG additionally selects `A8R8G8B8` for its PLTE/tRNS
entries. Row member `sub_1004D377C` passes both fields and the palette buffer to
the common converter `sub_1004DC50C`. WebP reader `sub_1004DB17C` maps its
feature-probe alpha flag to `B8G8R8`/`A8B8G8R8`. The GL context then reads the
source format through `sub_1004D3D14`; helper `sub_100597FD8` normalizes
`R8G8B8 -> B8G8R8`, `A8R8G8B8/P4/P8 -> A8B8G8R8`, and unsupported ETC1 to
`R5G6B5` before texture construction. The program selector therefore consumes
the GL texture's normalized format, not a palette index format or host decoder
color type. Hopper independently gives this normalizer 256 bytes and 20 basic
blocks and recovers the same `1 -> 2`, `3/10/11 -> 6` and conditional
`33 -> 7` branches.

The corresponding Rust structure is split along those native members:
`native_image.rs` owns the source pixel/palette layout, `surface_format.rs`
owns upload normalization, `assets/texture.rs` remains the cache/dispatch
facade, and `assets/texture/pvr_reader.rs` plus `raster_reader.rs` own their
format-specific decode paths. Five PNG color-type regressions, the native
greater-than-eight-bit rejection, RGB/RGBA WebP probes and indexed-upload
normalization cover the recovered boundaries.

The same structural pass moves immediate mesh batching out of the shared GPU
storage ABI and into `gpu/frame/batch.rs`. That module now owns quad-to-triangle
expansion, scissor resolution, required-texture collection, draw indexing and
the recovered viewport FMADD, while `gpu.rs` retains only the POD layouts and
renderer/frame storage shared across those native-style members.

Validation after this split reports 297 passing workspace tests, warning-free
strict Clippy and a successful release build. The recovered 11,000-frame
startup/menu/first-level/aim route reaches gameplay in
`audit-native-image-reader-split.png` with zero invoked compatibility
fallbacks; the prepared-texture cache resolves every required resource.

Rendering first targets a fixed 1024x768 `Rgba8Unorm` GPU texture so script
coordinates, atlas interpolation and screenshots do not depend on desktop
window size. A separate `wgpu` pass letterboxes that texture onto the platform
surface; headless screenshots copy the same GPU texture into a mapped buffer.
The original does not leave viewport projection to its GLSL vertex program:
`sub_100598CC4` rounds `2/width` and `-2/height` to float32, then applies each
with `fmadd(coordinate, scale, +/-1)`. Prepared wgpu vertices now carry that
CPU-computed clip position alongside their diagnostic screen position, so the
WGSL stage only forwards it. This preserves the otherwise-observable native
center-Y residual of exactly `-2^-25` at 1024x768 instead of simplifying it to
zero in a backend-dependent expression.
The WGSL sprite program implements the recovered colorize, silhouette, gold,
diffuse-modulate and alpha-mask formulas, composite affine matrices, repeated
fill textures and bitmap glyphs. Dirt bypasses the sprite fragment path and
uses the recovered transient polygon meshes described below.

The Rust backend now mirrors those recovered GL ownership stages in its source
layout as well. `gpu/frame.rs` preserves the renderer-neutral immediate stream's
single `order`/tie-breaker walk and expands sprite/capture operations;
`gpu/frame/text.rs` owns bitmap-glyph and projected 3D-text quads, while
`gpu/frame/geometry.rs` is the facade for shared atlas regions, GL-style
colored rectangles, DrawablePolygon/Dirt triangles and shader uniforms;
`gpu/renderer.rs` owns the wgpu device/surface, fixed game target, render-pass
splits, capture copies and final letterbox; `gpu/resources.rs` owns texture
upload, bind-layout/pipeline construction and the exact opaque,
premultiplied-alpha and straight-alpha state mapping. `gpu.rs` retains only
the shared vertex/frame/renderer storage ABI and its boundary regressions.
No render formulas or submission order were changed by this structural pass.
The live IDA MCP component audit and Hopper independently report the same
boundaries: `GL_State::begin` at `0x10059ACE8` is 1,644 bytes/124 basic blocks,
viewport/vertex preparation `0x100598CC4` is 820 bytes/11 blocks, capture
`0x100458F54` is 800 bytes/37 blocks, atlas-quad construction `0x100467760`
is 536 bytes/13 blocks, while the min-filter and wrap selectors at
`0x100206CA0`/`0x100206CE0` are small 64/36-byte leaves. This large
state/submit versus small resource-policy shape is the reason for the
`frame`/`renderer`/`resources` split.
The finer frame split follows concrete executable members. Hopper reports the
`drawString3D` path `sub_10003457C` as a separate 452-byte member, atlas pivot
setup `sub_100467A00` as 240 bytes/17 blocks, `DrawablePolygon::rebuild`
`sub_1000246A8` as 744 bytes/36 blocks, and the GL colored-rectangle member
`sub_100598CC4` as 820 bytes/11 blocks. IDA independently confirms the last
member's 820-byte extent and shows its four float32 projection vertices before
shader submission. Correspondingly, the former 835-line `gpu/frame.rs` was
separated into ordering, quad, sprite, text and geometry ownership groups; the
cross-command operation list remains shared, so captures and blend transitions
retain native immediate-mode ordering. The remaining 284-line mixed geometry
group is now an 11-line facade over 103-line atlas region, 111-line colored
rectangle, 47-line DrawablePolygon/Dirt and 32-line shader-uniform leaves.
This follows the independent `0x100467760/0x100467A00/0x100467BE8`,
`0x1000246A8` and `0x100598CC4` paths rather than splitting any one recovered
algorithm.
The outer executable host is kept separate from those recovered engine stages:
`assets.rs` owns KA3D discovery and shared sprite/affine data, `app.rs` owns the
cross-platform window/input/fixed-step lifecycle, and `cli.rs` owns the
deterministic screenshot interaction harness. The crate root retains the CPU
reference rasterizer used to compare recovered formulas against wgpu. This
prevents platform bootstrap code from being mistaken for a native Purple
renderer owner while reducing the former 1,970-line `main.rs` aggregate.

Animation skins are overlays on the `default` skin rather than complete
replacements. The active `Normal` Stella skin, for example, supplies hair
overrides while the default skin maps `STELLA_EYES_OPEN_` to the real atlas
sprite `STELLA_EYES_OPEN_2`. Skin-managed Telepods slots with no active/default
attachment are intentionally empty; their `Stella_Telepods_*` animation keys
must not be sent to the atlas renderer as sprite names.

Animation slot `zOrder` is parsed in `sub_100011008` and passed through the
slot callback at `sub_100012180`. The shipped Chapter 1 comic data provides a
useful ordering oracle: opaque panel backgrounds use z 14, character/details
use z 5..13 and borders use z 1..4. Native composition is therefore descending
z (far-to-near). Sorting ascending made the opaque background render last and
cover every valid foreground sprite, which presented as widespread missing
textures even though all atlas lookups succeeded. The animation bridge now
uses the recovered descending order.

`setTexture` (`sub_10004CC74`) stores the texture resource name at scene-object
offset `+112` and its resolved texture pointer at `+128`; `setTextureScale`
(`sub_10004CE38`) stores its scalar at `+196`. The native draw uses the base
sprite alpha as a mask over the repeating texture. The software renderer now
resolves single-sprite resource names to their backing PVR and implements the
same masked, wrapped sampling rather than treating these calls as metadata.

The native engine owns three deliberately separate trajectory stores. Two
0x38-byte flight-trail records begin at GameLua `+0x558`, their active index is
the signed i32 at `+0x588`, the raw simulation vector is at `+0x590`, and the
prepared `AimStream` object is referenced at `+0x668`. `startNewTrajectory`
(`sub_10004FD3C`) increments the index modulo two and assigns a completely
default record to the new slot; it does not clear the simulation vector.
`addToTrajectory` (`sub_10004FF14`) and `addPuffToTrajectory`
(`sub_10004FF80`) use the same generated adapter (`sub_100089A44`): all three
fixed Lua slots must be numbers, slot 1 is ignored, and slots 2/3 are narrowed
to float32 before becoming a point or puff. Normal/special sprite setters write
only the active record, whereas `setAimingAidSprite` writes the global
AimStream. `drawGameNative` calls `sub_10006D9C0` before drawing scene objects;
that helper renders both records in fixed slot order, uses each record's normal
sprite for every point and its special sprite for the optional puff, and
applies `(point - camera) * worldScale` plus local
`gameWorldScale * worldScale` sprite scaling.

`ClearSimulationTrajectory` (`sub_1000311B4`) clears only GameLua `+0x590`.
`getSimulationTrajectoryPoints` (`sub_1000311C0`) returns nil for that vector
when empty and otherwise a Lua array of `{x, y}` tables.
`native_drawSimulationTrajectory` (`sub_10004C4CC`) never reads either flight
record: it draws only the global AimStream. Its point setter
(`sub_10000873C`) ignores fewer than four simulation samples and otherwise
builds `[first, all samples, last]`, duplicating both endpoints. The Rust state,
strict adapters, clearing boundaries and render passes now preserve these
separations instead of conflating all three stores.

AimStream itself is no longer drawn as one fixed-size quad per raw simulation
sample. Its constructor (`sub_100007E1C`) initializes a 20-pixel pivot size,
0.6-second spawn interval, speed 4, inactive flag and a packed 12-byte particle
vector. Level initialization replaces the interval/speed with float32
`simulationAimSpawnTime`/`simulationAimSpeed` (0.6 and 5 in the shipped table).
`sub_10000839C` seeds `FCVTZS((controlCount-3)/(spawn*speed))` particles;
`sub_1000084C8` advances their path parameters with float32 FMA, removes those
past the last segment, scales them by
`(1.2 - parameter/(controlCount-3))*gameWorldScale`, and spawns new particles
while the unscaled frame timer is negative. Draw (`sub_100007FD4`) evaluates
the four adjacent duplicated-endpoint controls with the recovered float32
Catmull-Rom polynomial and recreates the divided GL context coordinates,
10-pixel pivot, rotation, camera translation, physics scale and particle scale.
`enableAimingAid` is a strict Lua boolean stored at GameLua `+0x528`; the
AimStream active flag follows it only after the current draw, and a
false-to-true edge repopulates the particle vector exactly as native does.

Hopper also recovers the formerly approximated predictor at
`sub_100032970`: it resolves only the dedicated `BirdSimulation` body,
multiplies float32 `objects.currentTimeStep` by the retained
`simulationTimeStepMultiplier`, then calls the custom one-body step
`sub_10086F6AC` exactly `simulationIterations` times. That helper is not the
ordinary Box2D island/world solver: it deliberately skips contacts and joints,
adds the predictor world's gravity without consulting the body's
`gravityScale`, integrates force, torque, linear/angular damping and sweep
state in float32/FMA order, clamps translation to 0.16 and rotation to pi/2,
then rebuilds the transform from the centre of mass and local centre. The
caller stores the transform origin when AArch64 `SDIV`/`MSUB` produces a zero
remainder (including only iteration zero when the sampler itself is zero),
invokes the tiny force/torque clear at `sub_10086F624`, and clears the
selected-simulation pointer before returning. The shipped attributes are
multiplier 3, 50 iterations and sampler 1, not the old host-side 60 unscaled
Euler samples.

The loop also walks the insertion-only GameLua `+0x3A0` list made by Object
Parameter 32 (`aimingAidCollideable`). It passes only each body's native
fixture-list head (the last-created fixture) through `b2TestOverlap`, whose
use-radii distance must be below `10*FLT_EPSILON`, then applies the recovered
gravity/water sensor-force path before the one-body step. Positive additional
bird gravity is likewise reproduced as a mass-scaled vertical force at the
render transform origin rather than as a host acceleration shortcut.
`getAimingTime` (`sub_10004B8EC`) is a separate float32 product of
`objects.currentTimeStep` and `FCVTZS/SCVTF(simulationIterations)`; it deliberately
does not include the trajectory multiplier and now preserves that ABI.

`setCameraLimits` is a one-float adapter over `sub_1000505D0`, storing the
camera span at engine offset `+544`. `setLevelLimits` passes four floats to
`sub_10004FFB8`, which notifies the native camera and persists integer-truncated
bounds. Its Lua order is `(xMin, yMin, xMax, yMax)`, while GameLua stores and
`sub_100059CF4` returns `(xMin, xMax, yMin, yMax)` at offsets
`+1544..+1556`. The post-physics frame pass compares every positive-mass body
against those four values and publishes matching names as true keys in
`g_outOfBoundariesObjects` before Lua `update`. `setPhysicsSimulationScale`
stores its scalar
at `+1292`; native particle viewport tests divide screen extents by this value.
It too is an unconditional float32 store, with no host-added positivity
filter. These formerly generic calls now have typed state in the rehost.
`setTopLeft` stores two float32 values at GameLua `+0x514/+0x518`,
`setWorldScale` writes the same float32 scalar to `+0x520` and the two camera
fields `+0x4F8/+0x518`, and `setMaxWorldScale` writes the theme-system scalar
at `+0x500`. The last entry has no native positivity check: zero and negative
values are stored verbatim. The rehost now preserves that ABI and rounds all
three adapters through float32 rather than retaining Lua doubles.

`setStartingCameraValue` (`sub_1000505C8`) is a one-byte GameLua store and
`clearScreen` maps directly to `sub_100044E84`. Live IDA MCP decompilation and
Hopper agree that this 268-byte single-basic-block function first replaces the
complete 0x9c-byte GL context with identity state, constructs the rectangle
`(-32000,-32000)-(32000,32000)`, and submits it with the packed background
color at GameLua `+0x238`. It does not delete earlier immediate submissions.
The Rust bridge now emits that rectangle at the current monotonic draw order,
without inheriting the previous scissor, and leaves the default render state
live. This preserves a `captureSprite` performed before the clear; the former
queue-clearing approximation silently erased that already-observable copy.
The adjacent color ABI is now exact as well. Adapter `sub_100089A44` reads
three Lua values as float32 before `sub_100030C60` applies `FMAX` with zero,
`FCVTZS`, explicit 255 ceilings and packs `0xFFRRGGBB` at `+0x238`.
`sub_100030CC4` returns bytes `+570`, `+569`, `+568` as three Lua floats, i.e.
red, green, blue. The Rust setter now quantizes before integer conversion, so
boundary inputs such as `0.99999999` reproduce Purple's channel value 1 rather
than being truncated as a host double to zero. Each adapter read goes through
`sub_10052859C`, whose `sub_1005281F8(..., 3)` guard requires the exact Lua
NUMBER type. Missing, nil, string and boolean slots therefore fail before the
member function is invoked and leave the packed color unchanged; the Rust ABI
now preserves that strict transactional behavior as well.
`refreshCurrentLocale`
(`sub_100050948`) normalizes the selected/device locale and calls the Lua
`setLocale` callback. `playVideo` (`sub_100051B60`) forwards its resource name
to the platform video service; the rehost preserves that request even though
this application bundle contains no movie asset to decode. Process-global
`createUniqueShaders`/destroy state, `requestExit`, device orientation and
rubber-band drawing are likewise represented by typed bindings.

The water controls are simple native field stores before entering the contact
force path: object drag at `+1340`, bird drag at `+1336`, additional simulated
bird gravity at `+548`, RGBA water colour at `+1344..+1356`, per-object water
flag at `+331`, and per-object water density at `+336`. Their Lua contracts and
state are mirrored even on levels that do not instantiate a water volume.

## Native theme renderer

`setTheme` maps to `sub_10004D348`. It resolves `blockTable.themes[name]`,
copies `skyColor`, destroys the previous layer arrays and rebuilds the native
background and foreground arrays from `bgLayers` and `fgLayers` through
`sub_10006B9A4`. `drawBackgroundNative` (`sub_10004C4A4`) and
`drawForegroundNative` (`sub_10004EF64`) select the two arrays and share the
renderer at `sub_10009BDB4`.
Their Lua ABIs are intentionally asymmetric. `drawBackgroundNative` uses the
strict one-float adapter at `sub_100088D24`; the member applies `FCVTZS`, draws
the complete background array when the result is negative, and otherwise
sets the loop range to exactly `[index, index + 1)`. This lets `gamescene.lua`
interleave one background layer with other draw stages through
`g_layerDrawIndex`. `drawForegroundNative` uses the zero-argument adapter at
`sub_10008A07C` and always draws the complete foreground array. The rehost now
preserves that split instead of discarding the background argument. NaN and
overflow retain ARM64's signed integer-indefinite result (and therefore the
negative/all-layers branch); only the native out-of-vector undefined read is
made safely empty.

The recovered layer contract includes sprite, independent `posX`/`posY` and
x/y offset pairs, x/y scale, `zDistance`, alpha, repeat flags, `velX` and
`velY`. `sub_10006855C` adds optional `xSpeedAdd`/`ySpeedAdd` to the two
velocities in float32 before inserting the record. The layer parser at
`sub_10006855C` maps `H_REPEAT` to bit `0x4`, `V_REPEAT` to `0x2`,
`REPEAT_LEFT_ONLY` to `0x100` and `REPEAT_RIGHT_ONLY` to `0x200`.
The same compare chain maps `OVERSTRETCH_ANCHOR_V` to `0x1`,
`REFRESH_ANIMATION_TIMELINE` to `0x8`,
`REFRESH_ANIMATION_COORDINATES` to `0x10`, `ANCHOR_V` to `0x20` and
`PREVENT_STRETCH_SCALE` to `0x80`. The Rust layer record now retains this
native mask as well as its convenient repeat booleans, so the camera transform
can distinguish the full vertical camera-delta and orientation-anchor paths.
The two functions have distinct ownership: `sub_10006B9A4` traverses the
one-based definition list (and expands optional `spawnParameters`), while
`sub_10006855C` constructs one 304-byte layer record. The Rust split now
mirrors that boundary with the list facade in `theme_layer_parser.rs` and the
record constructor in `theme_layer_parser/layer.rs`.

The remaining camera fields are mapped by record offset rather than their
nearby Lua parse order. `xMult` is native layer `+0x50`; with internal flag bit
`0x40` clear, `sub_10009CEB0` applies
`(zDistance + xMult) * cameraDeltaX`, while bit `0x40` selects the full delta.
Neither the `sub_10006855C` flag chain nor any ThemeManager writer sets that
internal bit in 1.1.6. `relativeX` and `relativeY` are instead the final floats
at `+0x120/+0x124`, initialized to the exact `FLT_MAX` bits `0x7F7FFFFF`.
IDA's complete displacement scan and Hopper's independent assembly agree that
ThemeManager reads only `relativeY`. On each draw, `sub_10009AA4C` replaces
layer `+0x40` with the fused-float32 result
`(relativeY * screenHeight - screenHeight/2) /
(originalCameras[2].sx / referenceCamera.sx)`. The Rust record retains the
unused `relativeX` ABI slot, parses `xMult`, and executes the live `relativeY`
overwrite without treating its missing sentinel as a coordinate.

The constructor stack frame and `sub_100079830` copy member resolve the other
early scalar slots exactly: `parallaxSpeed +0x18` (default `1.0f`),
`zDistance +0x1C` (default `0.0f`), `scaleSpeed +0x44` (default `1.0f`),
`angleMult +0x4C` (default `0.0f`), and `yMult +0x54` (default `1.0f`). IDA's
complete scan of every member touching the layer vectors at GameLua
`+0x268/+0x280`, together with Hopper's independent constructor and update
assembly, finds no post-construction read of `parallaxSpeed`, `scaleSpeed`,
`angleMult`, or `yMult` in Purple 1.1.6. They are copied record ABI, not hidden
motion inputs: `sub_10009B8B4` still integrates only
`velX/velY * delta * (1-zDistance)`, and `sub_10009CEB0` uses only `xMult`.
The Rust record now retains all six float32 slots and their native defaults;
this audit also corrects the former host-only `zDistance=1` default to the
constructor's zero default.

The same constructor stores the active drawing values as float32 as well:
numeric `offsetY`, `scale`, `scaleX`, `scaleY`, `animationSpeed`, `alpha`,
`minAlpha` and `maxAlpha` all narrow before the 304-byte record is inserted.
The two axis scales independently fall back to the uniform `scale`; an
authored `scaleX` does not become the missing `scaleY`. The rehost now keeps
that narrowing and fallback split instead of retaining Lua doubles and
chaining the Y fallback through X.

The four preceding sentinel fields are `worldX`, `worldY`, `worldW` and
`worldH` at layer `+0x104..+0x110`. `sub_10009A894` expands the current
camera rectangle with `left/right/top/bottomLimitWorld`; X/Y select a
normalized point inside that rectangle and subtract the cached theme reference
point, while W/H add a centered random displacement. The latter consume
`sub_10057B42C` in W-then-H order for every layer before `relativeY` can
overwrite Y. The Rust draw entry now runs this update across the complete
background or foreground array even when the caller selects one background
layer, matching `sub_10009AA4C`'s placement outside the selected draw range.

The record constructor asks the resource manager independently for the
sprite's left, right, top and bottom through `sub_10045CD14`,
`sub_10045CD60`, `sub_10045CDAC` and `sub_10045CDF8`. All four return zero when
`sub_10045BDDC` cannot resolve the resource. Therefore an absent sprite has a
zero rectangle in Purple; it does not acquire a generic 256-by-256 tile. The
rehost now preserves that result, preventing a missing theme asset from
turning into synthetic repeated columns or wrap distances.
The main renderer submits the reference tile, then calls `sub_10009C4C4`,
which walks right-hand columns followed by left-hand columns and invokes
`sub_10009CA0C` for the vertical copies of every repeated column. Only after
that helper returns does the caller invoke `sub_10009CA0C` for the reference
column's copies above and below. The one-sided flags suppress their opposite
horizontal traversal. The rehost retains this exact non-rectangular painter
order rather than sorting a tile grid or completing the reference column
first. The base/reference resource is not unconditional: `sub_10009BDB4` converts its
scaled bounds to doubles and submits only when all four inclusive viewport
intersection tests pass. It still invokes the repeat helpers after an
off-screen reference is rejected, allowing later rows or columns to become
visible. Both helpers retain their coordinates in float32 world space,
advance them with `FADD`/`FSUB`, and call `sub_100067A04` separately for each
candidate; their loop limits compare a float32 center plus or minus a
double-precision half tile against the four screen-to-world bounds cached at
manager `+0x90/+0x94/+0xA0/+0xA4`. Final viewport culling likewise uses the
projected center plus or minus a symmetric half width/height, not the atlas
pivot-relative rectangle. The wgpu command builder now follows the same
coordinate domain, rounding points, painter order and center-bound culling
instead of accumulating screen-space doubles or dropping asymmetric-pivot
tiles. The scale expression in
`sub_10009BDB4` combines current camera `sx`, corrected end-camera `sx`, and
the theme reference-camera `sx` by `zDistance`; its literal sequence is
`(current/end) * ((end/reference) * (1-z)) + (end/reference) * z`.
Using raw camera `sx` or the engine's unrelated 20-pixels-per-world-unit value
is observably wrong. The rehost now implements both ordered
passes, horizontal tiling, symbolic top/bottom anchoring, sky colour and the
two layer-offset entry points.

The `sprite` field is polymorphic. `sub_10006855C` accepts either one string or
a one-based string array, copying every array entry into the layer's frame
vector and using its first entry immediately. This is not optional metadata:
the shipped `theme_hometree_bottom`, beach-selection and bottom-gate themes
use six-frame `THEME_HOMETREE_BOTTOM_W1..W6` arrays in layers that the old
string-only rehost discarded entirely. Numeric `scale` supplies the fallback
for both `scaleX` and `scaleY`. In the update at `sub_10009B8B4`, positive
`animationSpeed` is the float32 frame period. The timer adds one delta and can
advance at most one frame per call; the same normalized timer forms a
`maxAlpha -> minAlpha -> maxAlpha` triangle. Layer offset accumulation is
also float32 and multiplies each delta by `(1 - zDistance)`. The Rust theme
layer now retains all frames and their geometry, the exact one-step timer,
alpha endpoints, scalar scale fallback and parallax-weighted velocity, which
restores the previously absent animated water layers.

The same update does more than accumulate velocity. Once a moving reference
tile is wholly beyond a camera edge and its signed velocity points farther
out, `sub_10009B8B4` shifts its offset in the opposite direction. The shift is
one tile span multiplied by `trunc(viewportSpan / scaledTileSpan) + 1`; the
final `+1` is formed by an `FMADD`, and horizontal and vertical tests use the
scaled half-size independently. The rehost now performs the same float32
wrap for all four velocity directions. On the deterministic level-two audit
this moves the scrolling water reference tiles back into the viewport and
reduces the final native-equivalent stream from 1,683 to 1,676 sprite
submissions without changing coverage or introducing a seam.

`sub_10006B9A4` gives `spawnParameters` a separate layer-list meaning before
the 304-byte constructor runs. It converts `amount` with `FCVTZS`, emits that
many records sharing the one-based source-definition index at `+0x88`, and
consumes the process-global CMWC in `velX`, `velY`, screen-X, screen-Y order
for every copy. Speed variance is multiplied in double precision and narrowed
before its float32 addition; screen coordinates use double `FMADD` around the
authored area centre. `worldX/Y/W/H` retain `FLT_MAX` as the missing sentinel.
An authored `spawnParameters` table with non-positive amount emits no layer;
it does not fall back to one ordinary record. This expansion now lives in the
separate Rust `theme_layer_parser/spawn.rs` boundary.

The record constructor also owns a float32 duration vector at `+0xD0` in
addition to scalar `animationSpeed` at `+0xFC`. Scalar `animationTimeline`
entries are copied directly; two-element tables sample
`base + cmwc() * variance`, consuming CMWC even for zero variance. The update
selects the current frame's vector duration and falls back to the scalar only
when the frame index is outside the vector. On an animation wrap,
`sub_100099C24` implements flag `0x8` with its literal one-based destination
index: element zero stays unchanged, sampled source entry N is written to
destination N, and the final one-past-end sample is consumed but has no later
visible vector effect. Flag `0x10` resamples screen X/Y, restores the four
world sentinels/coordinates, and immediately invokes `sub_10009A894`, so
world W/H consume two further samples before the later draw refresh. The
rehost preserves that indexing quirk and complete shared-random order.

Direct inspection of the loaded original `blockTable.themes` confirms that
this 1.1.6 bundle has neither `animationTimeline` nor `spawnParameters` on any
background or foreground layer; every authored `elements` array is empty as
well. These branches are dormant for shipped content, but are now implemented
as engine contracts instead of being inferred from unrelated screenshots.

Hopper resolves `setThemeOffsetY` through the strict string/float adapter at
`sub_1000866F8`; its ABI is `(themeName, offset)`, not a lone global offset.
The member at `sub_10003E308` walks every live background layer and fetches
that index's authored `offsetY` from `blockTable.themes[themeName].bgLayers`.
For a layer with an empty `ThemeSpriteData` vector it stores
`float32(gameWorldScale * offset / 768 + authoredOffsetY)`; a layer containing
at least one dynamic theme sprite instead stores
`float32(offset + authoredOffsetY)`. These are destructive per-layer writes,
so applying one renderer-wide translation both double-counts the authored
offset and misses the vector-dependent scale branch. The foreground member at
`sub_10003E59C` has the strict `(themeName, oneBasedLayer, offset)` ABI,
converts the index with `FCVTZS` and writes the float32 offset directly into
that live foreground layer. The rehost now follows those writes and keeps its
only out-of-range concession memory-safe.

The constructor's dispersed registration sites establish the exact relative
order of this family: `setThemeSprite` at `0x10002D24C`, the two offset
members at `0x10002D27C/0x10002D2AC`, `drawBackgroundNative` at
`0x10002DE14`, `setTheme` at `0x10002E204`, and `drawForegroundNative` at
`0x10002E4A4`. The former 195-line mixed Rust installer is now a 23-line
order-only facade over `theme_render_registration/sprite.rs`, `offsets.rs`,
`passes.rs` and `selection.rs`, retaining that order while keeping each native
member and ABI in its own leaf.

`native_refreshThemeSystem` reaches `sub_1000985DC`, which rebuilds
camera-derived layout caches from `castleCameraData`, `originalCameras` and
the current theme. Symbolic foreground offsets are not direct screen-edge
anchors. For each `offsetY = "top"/"bottom"` layer, `sub_1000985DC` first
zeros native layer+0x40 and calls `sub_100099828`. That helper walks the
one-based `gameCamera.resolutionCorrectedCameras` list, reads `sx`, `px`,
`py`, `left` and `top`, runs the zero-offset layer through
`sub_10009CEB0`, and records the float32 pair
`sx * (projected - cameraEdge)`. The top branch selects the maximum vertical
value and stores `-maximum/(endScale/referenceScale) -
trunc(height/2)*scaleY`; the bottom branch selects the minimum and stores
`(screenHeight-minimum)/(endScale/referenceScale) +
trunc(height/2)*scaleY`. The integer half-height conversion happens before
`SCVTF`, which is observable for odd sprites.

The rehost now preserves the authored Lua token separately from that refreshed
native float, consumes the float through the ordinary camera-relative draw
formula, and integrates later vertical velocity into the same native-offset
coordinate space. The implementation mirrors the disassembly split in
`device/theme_refresh.rs` and its `foreground_offsets.rs` helper rather than
leaving this cache as an approximate draw-time edge calculation. A real L50
resource probe produced the native-contract values `-1491.551270` for its top
layer and `879.547119` for its bottom layer from two corrected cameras.
Conversely, `native_resetThemeSystem` is not a layer
destructor: `sub_1000984C0` only clears the initialized byte at `+0x30` and two
derived floats at `+0x54/+0x58`. The rehost now preserves background layers,
foreground layers, theme sprites and authored offsets across reset; clearing
those collections was a direct source of otherwise unexplained missing
textures.
This preservation does not apply to `setTheme`: each `ThemeSpriteData` vector
is embedded at layer `+0x70`, so destroying and replacing the background and
foreground arrays also destroys every dynamic sprite owned by the old theme.
The flattened Rust container is now cleared at that exact ownership boundary,
preventing stale sprites from a prior menu or level theme from appearing in a
same-numbered layer of the next theme.
The separate `recoverRenderObjects` entry traverses scene objects and reacquires
OpenGL sprite/texture/sheet pointers in `sub_100059DB0`. Renderer-neutral Rust
commands retain resource names instead of raw graphics pointers and wgpu
resolves them through the live atlas cache at submission, so the equivalent
recovery has no mutable script-side state while retaining the native
zero-result ABI.

Theme sprites are not top-level layer replacements. IDA and Hopper agree on a
136-byte `ThemeSpriteData` stored in a vector owned by each layer.
`sub_100073A58` is the exact vector `push_back` path: it appends even when an
earlier record has the same name. Modify, replace and remove linearly scan and
act on the first match, while updates retain insertion order.
The rehost therefore uses an ordered duplicate-preserving container rather
than its former `BTreeMap`, which both sorted overlapping sprites by name and
silently overwrote duplicates. The exact
Lua wrapper is
`name, sprite, x, y, scaleX, scaleY, angle, layer, scaleSpeed, horFlip, velX,
velY`; `modifyThemeSprite` consumes
`name, x, y, scaleX, scaleY, angle, layer`, while remove and replacement are
also scoped by the combined zero-based background/foreground layer index.
Hopper's adapters at `sub_100086928`, `sub_100086454` and `sub_1000860D8`
show that these are fixed, strict slots rather than optional
arguments: strings, numbers and the `horFlip` boolean are type checked before
the GameLua member is entered. Every number is returned in `s0`, so the host
now rounds it to float32 at the Lua boundary. Layer selection first uses
`FCVTZS`; the member converts the selected non-negative offset with `FCVTZU`,
which makes negative and non-finite values address layer zero instead of
silently rejecting the call.
The frame dispatcher exposes a three-call native theme chain rather than one
mixed update. At `0x10005ED04` and `0x10005ED1C` it selects ThemeManager
background and foreground modes and calls `sub_10009B8B4`; at `0x10005ED28`
it then calls GameLua member `sub_1000607E8`. The former integrates
layer+0x2c/+0x30 into drawable offsets +0x3c/+0x40 using `(1-zDistance)`.
The latter integrates the same velocities without that factor into the
distinct `posX`/`posY` pair at +0x34/+0x38 before walking the nested sprite
vectors. The Rust state and frame code now keep both pairs instead of
conflating them. IDA's binary-wide reference scan further identifies the
preceding GameLua+0x6A8 load as the reference-counted `setPhysicsEnabled`
lock total: `sub_100041ABC` increments/decrements that exact field. Therefore
the dispatcher skips the complete three-call theme chain while any physics
lock is active; the host now retains that gate as well.

`sub_1000607E8` also advances sprite velocity and positive scale, performs the
foreground wrap relative to the original position and advances background
animation frames. All of those writes are float32 `FMADD`/`FMUL` operations.
The foreground reset tests strictly positive velocity; zero uses the same
positive-offset branch as a negative velocity. `rotateThemeSprites`
also takes a strict float32 delta, performs one `FMADD`, calls `fmodf`, and
normalizes negative results with the native two-pi bits `0x40C90FDB`. The
rehost mirrors that arithmetic and storage/update order. A binary-wide scan of
every `0x88` record-stride traversal finds only construction, mutation,
recovery, update, copy and destruction paths. The fixed theme draw at
`sub_10009BDB4` renders the layer resource at `+0xf0` and separate authored
element containers at `+0xc0/+0xc8`; it never reads the `ThemeSpriteData`
vector at `+0x70/+0x78`. Purple 1.1.6 therefore retains and updates these
dynamic records but does not submit them to OpenGL. The wgpu rehost preserves
that otherwise surprising vestigial behavior instead of visibly drawing the
records.

The older direct `createThemeAnimation` member at `sub_100055650` has a
different wrapper shape: it tests only the Lua stack top for a table and
returns zero values when that final argument is not a table. Its optional
numeric fields first use `sub_1005280FC` (`lua_isnumber`) and are read through
`sub_10052A014` (`lua_tonumber`) as float32, so numeric strings are accepted;
this includes the otherwise easy-to-miss `startAnimTimer` slot at
`ThemeSpriteData+0x20`.
`animDelay` initializes both the frame delay and current timer, after which
`startingDelay` optionally overwrites only the current timer. Animation
strings are consumed from indices 1 upward. The predicate at
`sub_10052811C` accepts Lua tags NUMBER and STRING before `lua_tolstring`, so a
numeric frame is converted and scanning continues; the first missing,
boolean, table or other tag terminates the sequence. The shared direct-Lua
coercion boundary now preserves these distinctions instead of finding an
arbitrary table argument, retaining f64 table values, rejecting numeric
strings/frames, or filtering past malformed frame entries. The split passes
all 263 workspace tests, warning-free strict Clippy, a release build and the
`audit-theme-split-menu.png`/`audit-theme-split-start.png` wgpu captures.

The following level-file ownership pass was cross-checked against Hopper's
procedure extents as well as the IDA constructor references. The editor member
at `sub_100044F90` is a 4,728-byte implementation, the two selector wrappers
at `sub_10004715C`/`sub_100047234` are separate 132-byte members, the fixed
save member at `sub_10004730C` is 13,308 bytes in Hopper, and the late failure
forwarder at `sub_100056950` is only 20 bytes. The Rust layout now reflects
those boundaries: `level_files.rs` is a 19-line order coordinator, generic
loading is a 54-line installer, and the largest level-specific module is the
223-line recovered save schema. All 263 workspace tests, warning-free strict
Clippy and the release build pass; `audit-level-split-menu.png` and
`audit-level-split-start.png` verify the wgpu menu/start flow after the move.

IDA measures the native update member `sub_10005E898` at 7,756 bytes with 354
basic blocks and recovers its scaled/unscaled delta path alongside the native
settings, audio and touch state. Hopper independently reports a 6,476-byte,
237-block extent for the same entry and keeps the 2,388-byte draw dispatcher
`sub_10004BAB4` separate. Startup is likewise not part of either member:
Hopper places the platform construction path at `sub_100026D2C`, locale setup
at `sub_100050948`, and the final startup callback at the one-block
`sub_10005D44C`. The former 528-line `host.rs` now follows those boundaries:
its core facade is 65 lines, startup 122, frame/draw 176, input 70 and output
draining 119. Existing double-delta, input-edge, scene-lifetime, mixed-draw and
startup-loader regressions cover the move. All 263 workspace tests,
warning-free strict Clippy and the release build pass; the
`audit-host-split-menu.png`/`audit-host-split-start.png` captures verify the
wgpu menu-to-start path through the separated host members.

The follow-up physics-host split deliberately keeps the complete 1/30-second
step body together because IDA places its `updatePhysics`, contact refresh,
island solve, TOI, `removeBlocks`, collision-velocity application and
`clearLuaForceFunctions` order inside `sub_10005E898`. Only the host-side
Lua/native ownership boundary moved to `host_scene_sync.rs`: direct Lua world
deletion expires scene/joint/track/contact/draw-callback mirrors, and the
contact filter refresh reads live `material`/`collisionMaterials` tables before
each native step. `host_physics.rs` is now a 342-line fixed-step coordinator
and `host_scene_sync.rs` is a 105-line ownership adapter. All 263 workspace
tests, strict Clippy and the release build remain green; the
`audit-host-physics-split-menu.png`/`audit-host-physics-split-start.png` wgpu
captures verify the resulting menu/start flow.

## Native rays, light beams and dirt extension

`makeRay` (`sub_10004CE5C`) does not take two endpoints. It installs a
`DrawablePolygon` callback on the named physics body and the four numeric
arguments are RGBA. The rehost replaces that body's sprite draw with a filled
software-rasterized fixture polygon. `makeLightBeam` (`sub_10005AD4C`) returns
an object exposing `plotPath` and `dispose`; `sub_10008B194` walks ten physics
units per segment, stops at the nearest fixture or level limit, and writes the
point array plus target object table back into the query table. Both object shape and
path behavior are now represented directly.

The extension implementation now mirrors those member boundaries too. The
228-line mixed source is a 28-line ordered facade over `polygon.rs` for
`sub_100035FFC` (1,016 bytes/16 blocks), `ray.rs` for `sub_10004CE5C`
(684/40), `light_beam.rs` for the 96-byte/single-block constructor plus
`sub_10008B194` (1,260/30), and `gravity_visuals.rs` for the separate debug
draw binding.

Direct Hopper assembly of `sub_10008B194` also corrected three observable
LightBeam details. `startAngle`, `startPoint`, its ten-unit direction and every
accumulated path point remain float32; a large-coordinate regression now
distinguishes the native endpoint from f64 integration. On a hit, `target` is
the canonical Lua object table associated with the fixture—not its name
string. Finally, the return is a change flag: first hit returns true, a repeated
hit on the same object returns false, hit-to-nil returns true, and nil-to-nil
returns false. This follows the old-target nil test and Lua equality helper at
`0x10008B564..63C`, rather than approximating the result as “ray hit”.

`createNativeBlockExtension` (`sub_10005A9F0`) constructs the DirtMechanics
object at `sub_10001F98C` and exposes `onCollision`, `checkCollisions`,
`isJointAttached`, and `render`. The constructor reads
`blocks[definition].components.dirt.bgTexture/fgTexture`, keeps an unchanged
background polygon and creates the initial foreground polygon from the body's
fixture path. `sub_1000208D4` draws the background first and then every current
foreground polygon. The placeholder block sprite (normally `RED_CROSS`) is not
part of this draw path. The constructor's call to `sub_100021B58` is a direct
`vector<float2>` copy: the initial background and foreground retain every
float32 coordinate unchanged. The 0.001 Clipper grid is first applied inside
`sub_100020D70`, so even a non-intersecting first cut quantizes only the
foreground while the background keeps the original source contour.

The original bytecode calls the factory with the fixed layout
`createNativeBlockExtension("dirt", block.name)`. `sub_10005A9F0` applies the
exact-string checker independently to slots 1 and 2 and looks up the first
string in the registered native-extension map; an unknown tag produces no Lua
result. The Rust bridge now preserves both strict slots and the registered-tag
failure instead of scanning for whichever string happens to occur last. The
four returned functions are already bound to the native instance and Dirt.lua
uses dot calls. Consequently `onCollision` and `isJointAttached` read their
first numeric value directly from stack slot 1; passing an explicit colon-call
table is, as in Purple, a type error rather than an accepted hidden `self`.

The collision adapter strictly reads fixed stack slots containing five floats,
a collider string and two post-collision velocity floats; missing/wrong types
throw instead of shifting later numeric values left. `sub_100020560` first
resolves the collider in the live RenderObject map and, when found, forwards
the final two float32 values to the shared `sub_10005E860` delayed-velocity
map. The first five values are independently narrowed into the 20-byte
`DirtMechanics::Collision` record whether or not the named collider still
exists. `sub_100020D70` subtracts the body's
float32 position from the queued point with two `fsub` instructions, then
builds an eight-point contour. Its loop converts the index to double,
multiplies by the exact double representation of the literal
`0.785000026f`, narrows for `sincosf`, and uses two float32 `fmadd`
instructions for the final point. Every coordinate is converted to an integer
Clipper point at 1,000 units per physics unit. Purple processes each
existing foreground contour in its own Clipper instance with non-zero subject
and clip fill rules. It executes to a `PolyTree`, scans the tree with
`GetFirst`/`GetNext`, and, only when an odd-depth hole is present, adds a
one-integer-unit vertical slit from -100000 to +100000 through the cut center
to the same instance and executes again. `PolyTreeToPaths` preserves the
algorithm-selected contour and path order. Every returned path is then cleaned
in place by `CleanPolygon(path, 20.0)`. Purple converts each `int64` coordinate
back with `scvtf` into float32 and multiplies by `0.001f`; it does not perform a
double-precision decimal division. The non-closing edge-length accumulator,
FMA squared distance, square root and one-unit rejection threshold are also
float32. Paths below that threshold are discarded; there is no separate
three-vertex host guard, so a cleaned two-point path at least one unit long is
retained even though its later ear-cutter produces no triangles.
`sub_100020914` then rebuilds the foreground drawables.

The shipped `BlockComponents/Dirt.lua` bytecode and the complete callee body
close a previously ambiguous physics boundary. Its
`onUpdateBlockPhysicsStep` calls `native_extension:checkCollisions()` and,
when at least one cut was processed, tests cached joint endpoints with
`isJointAttached` before optionally calling `destroyJoint`. The outer
`sub_100020768` does not mutate Box2D directly, but its cut callee does:
`sub_100020D70` snapshots `b2Body::m_fixtureList`, saves each fixture's
`m_next`, and calls `sub_10086B548` (`b2Body::DestroyFixture`) head first
before clipping. `DestroyFixture` unlinks that one fixture, destroys only its
attached contacts in contact-edge-list order (including synchronous
`EndContact`), then destroys its proxy, decrements the fixture count and calls
`ResetMassData`; this entire sequence repeats per old fixture rather than as a
body-wide batch. After cleaning and triangulating
the surviving contours, it initializes one `b2PolygonShape` from every
ear-cutter triangle and calls `sub_10086B454` (`b2Body::CreateFixture`) in
triangle order. `CreateFixture` creates an active body's proxy before
head-inserting the new fixture and calls `ResetMassData` after each append
whose retained density is positive. The constructor `sub_10001F98C` initializes the retained
`b2FixtureDef` from
`blockTable.materials[objects.world[name].material]`, narrowing density,
friction and restitution to float32 once; the cut path changes only the shape
pointer, so later material-table edits and fixture setters do not affect the
replacement triangles, whose fixture definition also restores `sensor=false`.
Contacts are therefore ended synchronously, mass data is
recomputed, new broad-phase proxies are buffered, and contacts, position
constraints, TOI sweeps, ray casts and `isJointAttached` all observe the cut
geometry. The rehost now follows that full fixture lifecycle instead of either
keeping the old fixture solid or applying an analytic octagon filter outside
Box2D.

The clipping half of that lifecycle is now pure Rust. `clipper2-rust` supplies
the safe i64/non-zero PolyTree boolean engine; the compatibility layer rebuilds
the full subject plus original-cut plus one-unit slit input for the second
execution because that port cleans scanline state after each execute. Purple's
Clipper 6.2.1 left-slit output-record start is restored generically, and
`dirt/clipper/clean.rs` is an index-based intrusive-ring translation of its
`OutPt` `CleanPolygon(path, 20)` traversal. Purple 1.1.6 was built on 6 May
2015, before Clipper 6.4's July 2015 release, and Hopper's recovered
Clipper/tree/clean function structure matches 6.2.1. Exact regressions lock the
integer intersection rounding, clean distance, algorithm-selected starting
vertex and multi-path order without a C++ build script or unsafe FFI. The
octagon and conditional slit rebuild the foreground paths, and both layers are
triangulated into `wgpu` draw streams.
`DrawablePolygon::rebuild` at
`sub_1000246A8` additionally proves that Purple copies the contour into
float32 X/Y arrays, reverses it unconditionally with `sub_100872308`, and then
calls the already recovered quality-ranked ear cutter `sub_100871498`. The
renderer uses that same route, including the
native interior diagonal and triangle vertex order. The exact same triangles
are now installed as the replacement physics fixtures. DrawablePolygon's local
physics coordinates are passed unchanged as repeated texture UVs while
positions are scaled by 20. The bundled `2d-sprite` state has blending
disabled, so the two passes use the opaque replacement pipeline; a hole
therefore reveals the background dirt texture instead of transparency.
Binding is idempotently retried from the dirt callbacks because one shipped Lua loader publishes an
object's `definition` after constructing its components.

## Editor definition loader

`loadBlocksForEditing` is the large member at `sub_100044F90`, not an empty
platform hook. It creates `blockEditorTable` and loads fourteen named modules
below `scriptPath`, from `blocks_levelgoals` through `groups`, into separate
child environments. The Rust binding now follows that recovered order and
module naming. The shared loader also accepts ordinary Lua source in addition
to transcoding Purple's 32-bit-number Lua 5.1 bytecode, matching the native
loader used for editor/AppData files.

## Native particles and audio handles

The four particle passes map to distinct native modes. The consecutive raw
constants at `0x1009AF128` are `1, 2, 3, 4`:
`native_drawForegroundParticles` uses mode 1,
`native_drawBackgroundParticles` mode 2, `drawMenuParticlesNative` mode 3,
and `native_drawNotificationParticles` mode 4. Only the first two entry points
check GameLua's in-game-particle enable byte; menu and notification rendering
remains active while in-game particles are disabled.
`clearParticlesNative` recognizes `INGAME_BACKGROUND`, `INGAME_FOREGROUND`,
`MENU`, and `ALL`. The constructor chain at `sub_10008E160` and
`sub_10008E524` reads emitter geometry, randomized sprite/velocity/gravity,
angular velocity, lifetime, start/end scale and the time-multiplier flag. Its
field conversions are intentionally asymmetric: definition emitter and sprite
angles are multiplied by the float32 `pi/180` constant at `0x1009AF120`, while
per-call angle overrides are already native radians and angular velocity is
never degree-converted. `useAngleFromSpawner` adds the caller angle to both the
velocity direction and the sprite orientation. The randomized emitter extent
is `(w + areaW) * emitAreaScaleX` and `(h + areaH) * emitAreaScaleY`; an explicit
zero request amount falls back to the definition amount. `ignoreLimits` falls
back through the definition's `reference` metadata table, and absent
`ignoreDeltaTimeMultiplier` defaults true only for menu/notification modes 3
and 4. It halves a requested burst above the soft threshold of 61 and trims the
hard population boundary to 1000 unless limits are explicitly ignored; the
same `ignoreLimits` byte bypasses both checks. The rehost
now updates and renders those modes deterministically, including launch
feathers, smoke and waterfall mist. The shared random source at
`sub_10057B42C` is also recovered: four fixed xorshift seed words fill a
4096-word table, followed by the `a=18782`, `c=362436` complementary
multiply-with-carry sequence and exact `u32/2^32` conversion. Particle
construction consumes eight float32 random samples in native field order and
only consumes the ninth sprite-selection sample for non-animated definitions.
When `animation` is exactly `lifeTime`, the first sprite is installed without
that random draw and `sub_100091834` advances to
`ceil((elapsed/lifetime)*spriteCount)-1` with the native one-based clamp.
Gravity, velocity, transform, angle, lifetime expiry and current scale are now
retained and updated as packed float32 particle state rather than host doubles.
The two branches of `sub_100091834` are preserved as well: disabling in-game
particles freezes modes 1/2 in place while modes 3/4 continue to age, and the
velocity displacement is rounded as float32 before the viewport-scale FMA.
Infinite-lifetime particles use strict framebuffer-edge tests and wrap to the
opposite 1024x768 logical edge after integration.

IDA's immediates and Hopper's `MOV/MOVK` pairs give the seed words as
`075BCD15`, `159A55E5`, `1F123BB5` and `05491333`. They are numeric ARM words,
not byte arrays. The earlier Rust port had byte-reversed every seed and thus
produced a different stream despite implementing the later CMWC recurrence
correctly. The shared theme/particle source now starts with the literal words;
its first ten generated `u32` values are locked by a regression.

`playAudioReturnUniqueHandle` (`sub_10005902C`) consumes clip name, volume,
loop flag and optional channel/group and returns a monotonically unique handle.
`setAudioClipVolume` (`sub_10005920C`) and `stopAudioWithHandle`
(`sub_100059288`) address that handle; the channel-limit adapter is also
mirrored. The Rust bridge now preserves this lifetime and control ABI, so Lua
receives real handles instead of nil. Audio-device output and compressed-stream
playback remain separate host integration work.

IDA and Hopper now also agree on the ownership split behind those bindings.
`game::LuaResources::LuaResources` (`sub_100446570`) publishes `playAudio`,
`stopAudio`, `stopAllAudio`, `isAudioPlaying`, `setMasterVolume`,
`setTrackVolume` and `getTrackVolume` through the member cluster at
`sub_100448A94`..`sub_10044AAD0`. The distinct `GameLua` constructor
`sub_10002C274` publishes `setChannelCountLimit`,
`playAudioReturnUniqueHandle`, `setAudioClipVolume` and
`stopAudioWithHandle` through `sub_100058FFC`..`sub_100059288`. The Rust source
therefore uses a 29-line ordered facade with separate `playback`, `volume` and
`playback` and `volume` leaves instead of one mixed 280-line installer; the
four `GameLua` globals now live under `game_lua/audio_registration.rs`, their
actual constructor owner.

The underlying AudioManager layout is recovered as eight float32 track
volumes initialized to 1, eight signed 32-bit channel limits initialized to
-1, and a 32-bit wrapping unique-handle counter initialized to zero.
`sub_100572208` rejects playback when the unsigned active count reaches the
selected channel limit and otherwise increments that counter after assigning
the handle. Track volume alone clamps to [0, 1]; master volume and individual
clip volume retain the supplied float32 value. Resource `playAudio` defaults
only absent optional slots, while `playAudioReturnUniqueHandle` also treats
explicit nil as absent. The generated number, boolean and integer adapters are
strict; the stock Lua 5.1 compatibility boundary represents Purple's tagged
integer handles only as finite integral numbers. All 274 workspace tests pass
with these structure and ABI corrections.

The same dual-disassembler pass exposed an ordering error hidden by the old
module-granular installer. `sub_100446570` publishes methods in this exact
sequence: common resource creation at `0x1004465DC..0x10044669C`, audio
creation and capture, common releases at `0x10044678C..0x100446830`, locale
selection, font selection/enumeration, draw, clip/string/audio playback,
geometry queries, font metrics, locale query, audio controls/volumes, and
finally `openURL`. Rust now exposes phase-level install functions and its
147-line coordinator follows that sequence instead of installing all release
members before all audio creation members.

IDA/Hopper agree on the underlying lifecycle adapters: system-font creation
is `sub_1004471B4` at 516 bytes/17 blocks and stroked creation is
`sub_100447480` at 668/14; sprite/composite/font/text releases are independent
thin members at `sub_100447FDC`, `sub_1004481AC`, `sub_1004481B4` and
`sub_1004481BC`, while `releaseAudio` is `sub_1004481C4` at 152 bytes in IDA.
Accordingly the former 265-line lifecycle installer is a 23-line facade over a
187-line creation leaf and 91-line release leaf. The recovered adapters also
retain their asymmetric optional-slot behavior: creation members probe
optional booleans/numbers and leave wrong types at defaults, whereas an
explicit second slot to `releaseSpriteSheet` is strictly boolean.

The next source split follows the individual draw/query members rather than
the constructor's broad feature labels. Hopper measures `drawSprite`
`sub_1004483AC` at 696 bytes/23 blocks, its `drawCompoSprite` forwarder
`sub_100448710` at 24/1, and `drawString` `sub_100448728` at 384/14.
Sprite bounds and pivot are independent 232-byte/9-block members at
`sub_100448EB4` and `sub_100448FF8`; composite bounds begins at
`sub_10044913C` (476/18), with the data/entry methods following it. Therefore
`query_registration.rs` is now an 18-line facade over `geometry`, `clip_rect`
and `font` leaves, while `draw_registration.rs` is a 20-line facade over
`sprite`, `text` and `capture` leaves. The constructor phase API and immediate
draw submission order are unchanged.

## Resource lifetime, composite order and sprite shaders

The resource-manager constructor at `sub_100093904` registers sprite-sheet
creation/release through `sub_10009470C`/`sub_100094800`. Audio bundle and
AppData creation (`sub_100093C10`/`sub_10009410C`) key their native lifetime
map by the second string argument, while release and play are zero-result Lua
methods. Both creators strictly read path/name strings; argument 3 defaults to
true only when absent and, when present, is a strict boolean. The play member
at `sub_100093B00` likewise strictly reads its name even though it returns no
values. The Rust methods now preserve these error paths instead of silently
accepting incomplete calls. The boot path also calls bitmap/system-font, text-group,
composite-set, audio-output and locale lifecycle methods on `res`. These calls
now update typed Rust resource state and preserve their zero-Lua-result ABI;
`loadLocale` parses the requested group/locale and `useLocale` selects only an
already loaded locale.

`sub_1004370A4` shows that a composite part record contains name, sprite,
position, scale, flip multipliers, angle and enabled state, but no runtime z
field. `sub_1004376D4` visits the records in stored order and composes their
full affine matrices. The five overlapping `ISLAND_TAIVAS` sky records are
therefore not eligible for a synthetic z sort; the fine vertical joins also
appear in the native 1.1.6 output and are retained rather than hidden with an
invented overlap rule.

The same composite entry APIs expose zero-based raw part coordinates and
mutable name, x/y, scale, angle, flip and visible fields. Native bounds are
recomputed from the transformed child rectangles while those raw coordinates
remain unchanged. Runtime updates now reach both Lua bounds queries and the
wgpu draw catalog. Sprite, text and plain-geometry calls also share one
monotonic immediate-submission sequence; preparing them as separate batches
incorrectly changed occlusion. `setClipRect` truncates its inputs to integer
edges, and each submission captures the current rectangle as a clamped wgpu
scissor rather than applying one frame-global clip.

`captureSprite` is registered through `sub_100446570`, adapted by
`sub_100447FD4` and implemented at `sub_100458F54`. It captures the current
render target at the exact call point, creates a full-target sprite when the
name is new, and updates the captured texture when it already exists. It is no
longer a void compatibility stub: the wgpu command stream splits render passes
around captures, copies the 1024x768 target into a named GPU texture and makes
that sprite available to later commands in the same frame and later frames.

IDA `sub_10000FC30` and Hopper independently establish the
`AnimationWrapperNative.setShader(tag, optionalTable)` contract. Exactly two
Lua arguments with a table cause `sub_10006CB08` to construct the shader;
omitting the table or passing nil clears it, unknown scene tags are ignored
after a native warning, and the wrapper returns zero Lua values.
`sub_100013F44` clones the submitted shader into the per-tag wrapper map and
the underlying animation scene. `sub_10006CB08` is shared with the ordinary
`drawSpriteWithShader` path in `sub_10006C838`: the name cache owns one mutable
shader, each call changes only the submitted parameters, and callers receive a
snapshot, so omitted parameters inherit the cached value. `params` is a
strictly required table. `float` values are converted to native float32 and
default to zero when absent or non-numeric. Each component of a `vector` is
also float32, but absent/non-numeric components independently default to one.
This matters for the shipped LEAVES transition, whose two `DIFFUSEC` values
contain only RGB triples; native code supplies alpha 1 rather than rejecting
or zeroing the tint. The rehost now reproduces that shared-cache and clone
boundary and applies each scene snapshot to every animation slot. The
software renderer executes the formulas bundled in
`pixelShaders/pp.ps` for colorize, silhouette, gold and diffuse modulation,
including `DIFFUSEC`, `LIGHTNESS`, `SATURATION`, `HIGHLIGHT` and shader alpha.

## Animation entity matrices and sprite bounds

IDA and Hopper agree on the complete entity-query family registered by
`sub_10000EC80`. Local position (`sub_100015AB8` -> `sub_100014D84`) reads the
entity's own matrix translation, while local scale (`sub_100015E38` ->
`sub_100015000`) returns the magnitudes of its two basis columns. Their missing
defaults are `(0,0)` and `(1,1)`. World position/scale
(`sub_100015C78`/`sub_100015FF8`) first multiply the entity matrix by the
inverse animation-scene matrix, so “world” here means scene-relative and does
not include `AnimationWrapperNative.setTranslation`, `setScale` or
`setRotation` on the wrapper scene.

`getEntityWorldTransform` at `sub_10000F46C` returns no Lua values when scene
or entity lookup fails. A valid entity returns scene-relative x/y, exact basis
magnitudes, and `atan2(m10,m00)`. If and only if the entity owns a
SpriteComponent, it appends a sixth boolean stating whether that component's
current sprite pointer is non-null. The Rust runtime now composes the full 2D
affine hierarchy for these queries instead of conflating local and world
values or leaking the wrapper transform into both.

`getEntityWorldBounds` (`sub_1000161B8` -> `sub_1000152BC`) returns four
numbers in `left, top, right, bottom` order. Missing scenes, entities, sprite
components or current sprites produce four zeroes. Native code centers the
rectangle on the scene-relative entity translation and multiplies half the
current sprite width/height by the two matrix-column magnitudes; it
intentionally does not calculate a rotated four-corner AABB. The rehost now
uses the active skin attachment and atlas geometry to reproduce that contract.
This includes the attachment's translation, scale and rotation already applied
by `SpriteComponentCustom`; omitting it collapsed the four comic-border bounds
onto their parent nodes and clipped almost every panel away. Attachment aliases
are looked up in the skin before basename canonicalization. That ordering is
observable in the second-chapter finale, whose animation tracks use names such
as `borders_chapter_2_end/CHAPTER2_PAGE1_PANEL1_DOWN` while the resolved sprite
component is named only `CHAPTER2_PAGE1_PANEL1_DOWN`.

The draw path now preserves the same matrix precision as the query path. IDA
and Hopper both show `sub_10001E440` multiplying every linear basis and
translation component directly; the routine does not decompose parent and
child matrices back into angle plus scale. Animation rendering therefore now
composes wrapper transform, entity hierarchy and skin attachment as complete
affine matrices and passes all four linear components to the software
renderer. Signed scale and angle remain available as compatibility metadata,
but no longer determine vertex geometry, so nested non-uniform scale,
rotation, mirror and the resulting shear are retained exactly.

The same registration audit also fixes the animation lifetime ABI. Bundle and
AppData loads use the void two-string adapter `sub_10001D43C`; `start` and
`setSkin` are void as well, and both preload methods use the void one-string
adapter `sub_10001D22C`. `sub_1000163A4`/`sub_100016484` populate the separate
bundle and AppData JSON maps through `sub_1000E2660`, and later loads reuse
those parsed assets. `clearCache` (`sub_10000FD98` -> `sub_1000E251C`) erases
only those two maps: it does not close loaded scenes, discard transforms or
stop playback. `stop` (`sub_100013720`) consumes both tag and action, with an
empty action meaning stop all actions in that scene. Speed and seek values are
stored verbatim by `sub_100013D08` and `sub_10040E798`; their shared
`sub_10001C8C0` adapter requires a string in slot 1 and a number in slot 2,
then narrows the number to float32 before either store. It does not scan later
arguments for a usable value. Paused playback is not reported as playing.
These distinctions are now represented by typed bundle and AppData caches
rather than boolean-returning placeholder methods. The playback registration
itself is now a nine-line facade over native-aligned `controls`, `update` and
`callback` leaves.

All 63 non-transform animation tracks in the extracted 1.1.6 content are the
single native `spineEvent` type. The entity callback at `sub_1000121F4` ignores
empty values and parses every non-empty payload as
`name:integer:float:string`, truncating the final string at a further colon.
`sub_1000171B4` queues the typed event, and `sub_100016FE4` proves the exact
Lua callback order is `(tag, action, name, integer, float, string)`. Playback
completion uses that same six-value ABI with zero/empty payload fields. The
runtime now preserves these tracks, fires every keyframe crossed by forward,
reverse or wrapped playback, and drains them after the scene update like
`sub_1000141E4`; events such as `particles`, `cameraShake`, `playAudio`,
`instantIn` and `fadeOut` are no longer silently discarded.

## Platform compatibility contracts

`setGameParameters` is the direct table reader at `sub_100055438`; this build
only consumes optional `deterministicPhysics` and `gameWorldScale` fields.
`native_getOSName` constructs the exact string `iOS`. The bundle-to-AppData
copy at `sub_10005A2B4` reads the complete source resource and overwrites the
destination; the rehost performs the same copy, mapping already-extracted
configuration JSON back to the `.dat` destination names expected by the
original scripts. Notification callback storage, renderer game-on state,
smooth zoom, mouse-wheel scale, editing and accelerometer enable state also
have typed bindings instead of generic nil-returning stubs.

The generated-adapter audit now preserves their input ABI rather than merely
their stored values. `enableSmoothZooming`, `setEditing` and
`setAccelerometerActive` all use `sub_10008962C`, which reads slot one through
the exact BOOLEAN guard `sub_1005281BC`/`sub_1005281F8(..., 1)`.
`setWorldScale` and `resetMouseWheelScale` use `sub_100088D24`, whose
`sub_10052859C` path requires NUMBER and narrows to float32 before invoking
`sub_10004396C`/`sub_100043980`. Missing or wrong-type calls now fail before
state mutation, and valid near-boundary doubles reproduce the native float32
value.

The same constructor audit now covers the adjacent world/environment family.
`setGameOn`, `setStartingCameraValue` and `enableAimingAid` bind the BOOLEAN
adapter `sub_10008962C`. `native_setBirdWaterDrag`,
`native_setObjectWaterDrag`, `native_setAdditionalBirdGravity`,
`setPhysicsSimulationScale`, `setMaxWorldScale` and `setCameraLimits` bind the
one-NUMBER float32 adapter `sub_100088D24`. `setWorldGravity` and `setTopLeft`
bind the two-NUMBER adapter `sub_100088294`, while `native_setWaterColor` binds
the four-NUMBER adapter `sub_100088BC0`. IDA confirms the latter calls
`sub_10052859C` for slots one through four before invoking `sub_100031198` with
four float registers. The Rust adapters therefore validate the complete call
before mutation, preserve slot-one rather than last-number semantics, and
store the exact f32-widened results.
`setLevelLimits` is another `sub_100088BC0` registration; its four validated
float32 corners are then truncated and reordered by `sub_10004FFB8`, so an
incomplete call no longer invents zero-valued bounds.

The layout utility is the five-argument entry at `sub_1000E0B70` rather than
a table-field passthrough. `Align.getPositionAndScale` strictly reads a layout
table followed by reference width/height and target width/height. `scaleH` and
`scaleV` independently interpret `TRUE`, `UP` and `DOWN`; Purple then applies
its internal `FIXED` mode by choosing the smaller permitted axis ratio and its
`NORMAL` mode leaves that ratio linear. `LEFT/TOP`, `RIGHT/BOTTOM` and
`CENTER` use the recovered reference-to-target position formulas, while an
unknown alignment leaves the authored coordinate unchanged. Missing numeric
fields convert to zero through the native Lua table reader. This full float32
path now replaces the former guessed `(posx,posy,scalex,scaley)` return and is
covered at up-, down- and center-aligned aspect ratios.

The corresponding Rust service-table registration is now an order-preserving
42-line facade instead of the previous 297-line mixed-owner file. ForceUpdate,
Analytics, FusionGamerServices, downloadable Assets and Align live in separate
`game_lua/platform_services/` leaves; AnimationWrapper and SimpleRandom remain
calls to their already separate owners at the same coordinator positions.
This reflects native ownership: both IDA and Hopper measure the ForceUpdate
adapter `sub_100026880` as 104 bytes, the Game Center constructor
`sub_10054C784` as 296 bytes, the backend-name literal member
`sub_1000CA39C` as 44 bytes, sprite-sheet creation `sub_1000AC660` as 312
bytes, the Align adapter `sub_1000E0B70` as 28 bytes and its implementation
`sub_1000E0CB0` as 1,864 bytes/51 blocks. For `Assets::loadFiles`, IDA assigns
776 bytes/46 blocks while Hopper assigns 632 bytes/32 blocks because IDA also
owns the adjacent exception-cleanup tail; both identify entry
`sub_1000AC25C` and its asynchronous request/callback body. The split records
that genuine disassembler ownership difference instead of selecting a size
only to make the reports appear identical.

## Complete native-registration audit

The full body of `sub_10002C274` was extracted from IDA and its important
contracts were independently checked in Hopper's `Purple` document. Comparing
every registered Lua name against the Rust host initially exposed 68 names
that did not exist at all, even though the ordinary startup/first-level path
did not call most of them. They are now all explicitly registered, and a test
fails if any of those globals ceases to be a function.

The audit also distinguishes intentional native emptiness from missing work.
`createDirectory`, `linkSensor`, `goToTaskSwitcherLua`, `print`,
`printWithTag`, and `sendTweet` map to `nullsub_12`, `nullsub_11`,
`nullsub_14`, `nullsub_9`, `nullsub_10`, and `nullsub_15` respectively.
`checkDirectory` (`sub_10004BAA0`) always returns false and
`getDirectoryFileList` (`sub_10005A298`) returns an empty table in this build.
`GetDate` (`sub_1000313FC`) returns `time(NULL) / 3600`, `getDeviceID`
returns an empty string, and `verifyDeviceID` returns false. Reproducing these
odd results is more faithful than inventing desktop behavior.

The separately registered `uniqueDeviceId` reaches
`pf::DeviceID::Impl::getDeviceID`: it first requests the MAC address, replaces
the iOS `02:00:00:00:00:00` sentinel with `identifierForVendor.UUIDString`,
and otherwise retains the literal `unavailable`. A desktop without those iOS
identifiers now uses that recovered fallback instead of a fabricated shared
device ID. `FusionGamerServices.getBackendName` likewise returns Purple's
literal lowercase `gamecenter`; the offline host still reports the unavailable
service state, while `postAchievement(string)` and `postScore(string, number)`
now enforce their recovered adapters and return zero Lua values.

Purple's native `Assets` object registers `loadFiles` (`sub_1000AC25C`) and
`createSpriteSheet` (`sub_1000AC660`). `loadFiles` consumes every value in its
argument table and completes asynchronously through either
`onLoadSuccess(requestToFilenameTable)` or
`onLoadError(failedFilenameArray, errorCode, message)`. The discontinued RCS
service is now represented by the local AppData cache with the same callback
shapes and zero-result ABI; unavailable remote files follow the error callback
instead of silently disappearing. `createSpriteSheet` strictly consumes the
name, descriptor and texture strings and installs the sheet under the supplied
name, replacing the earlier unconditional no-op.

`SimpleRandomNative` is likewise no longer backed by host-invented FNV/LCG
substitutes. Registration at `sub_100094D34` exposes `newSeed`,
`newSeedFromString`, `newSeedFromNumber`, `random`, `seedToString` and
`newSeedString`. The process-global seed source at `sub_10057B534` is the
recovered 4,096-word complementary-multiply-with-carry generator initialized
from the four literal xorshift words. Explicit seeded sampling at
`sub_100095260` uses Purple's `seed * 214013 + 2531011`, its single
`UINT32_MAX` correction, the upper 16 bits and an inclusive unsigned range.
Decimal seed strings use the native stream-extraction behavior and invalid
strings return zero Lua values. This restores deterministic particle and
variant sequences as well as the missing `seedToString` entry.

The remaining object/physics entries now have concrete state and behavior:

- `native_resizeRadius` (`sub_100059488`) writes the native radius, destroys
  the head fixture and recreates a non-sensor circle from the supplied radius,
  density, friction and restitution. It does not take an absolute value,
  compose with an earlier physics-scale factor, restore the sensor flag, wake
  a sleeping body directly, or mirror those values into Lua. A touching old
  fixture nevertheless reaches Purple's EndContact listener, whose stores at
  `0x1000653DC..0x100065410` wake both bodies and clear their sleep timers
  before the synchronous callbacks run against the old Lua record. The
  replacement contact can therefore enter on the following Collide pass;
- bytes `+319`, `+333`, `+332`, `+301`, `+302/+303` and floats `+296`,
  `+268/+272`, `+304/+308` are represented by block-collision, ignore-score,
  keep-orientation, record-velocity, the two independent reverse-gravity
  modes, collision-time, sensor-force and reverse-gravity multiplier state;
- `setSpriteRotation` (`sub_10003FC88`) normalizes the visual angle to one
  turn without replacing the body's physical angle, while `multiplyVelocity`
  (`sub_100041A44`) scales both linear components and wakes a moving body;
- `getObjectVertices` (`sub_10005A7BC`) returns one array per fixture in native
  body-list order and one `{x,y}` table per already-scaled float32 vertex;
  `destroyTrack`, `getCurrentTrackAngle`, and `objectAndTrackOverlap` use the
  recovered track state;
- `setJointParameters` follows the IDA/Hopper table fields `name`, `motor`,
  `motorSpeed`, `maxTorque`, `limit`, `lowerLimit`, `upperLimit`, `frequency`,
  `dampingRatio`, and `length`; object-joint removal and limit checks use the
  same joint collection;
- `native_applySensorForces(sensor, object)` now follows `sub_10005DE90` and
  `sub_10005E404` at native float32 precision. It checks the signed gravity
  mask/category expression, requires the dedicated `+0x143` active-sensor
  byte, distinguishes circular and directional rectangular falloff, points
  gravity toward the sensor, multiplies by `bodyMass * 0.1`, and preserves the
  controllable collision timer plus both reverse-gravity branches. Water
  sensors use density-difference buoyancy, edge attenuation, force-point
  torque, the vertical velocity cutoff and the independently selected bird or
  object velocity drag;
- IDA and Hopper independently confirm that this is a real caller/callee
  boundary rather than one large source routine: `sub_10005DE90` is 1,396
  bytes with 64 blocks (IDA complexity 41), calls the 1,116-byte/41-block
  `sub_10005E404`, and Hopper finds no other caller of that water member. The
  Rust source therefore uses an eight-line `sensors.rs` facade over
  `sensors/force_dispatch.rs` and `sensors/water.rs`, retaining the same
  direction of dependency;
- constructor defaults recovered at `sub_10002C274` and the RenderObjectData
  creators are also preserved: gravity-force multiplier `4.0f`, water-force
  multiplier `2.0f`, bird-water drag `0.4f`, object-water drag `1.0f`, sensor
  mask/category `-1`, collision time `-1.0f`, and block collision enabled;
- the direct body-flag wrappers are now separated from Lua-table setters.
  `setSleeping` (`sub_10004DAD4`) clears awake state, velocities, force, torque
  and sleep time when passed true, but an already-awake false call is a total
  no-op. `setAsSensor` (`sub_100041824`/`sub_10086CD38`) changes every fixture
  and wakes the body only when the sensor byte changes. `setFixedRotation`
  (`sub_1000411F0`) toggles the body flag and runs ResetMassData. None of these
  three wrappers immediately rewrites the similarly named `objects.world`
  field; ordinary fixed-step state write-back later publishes sleeping and
  velocities. The adjacent scalar wrappers are native-only as well:
  `setLinearDamping`, `setAngularDamping` and `setGravityScale` write the body,
  while `setRestitution`/`setFriction` write only `m_fixtureList`, the most
  recently attached fixture. Multi-fixture bodies now retain per-fixture
  values and the contact solver combines the exact fixture pair instead of an
  object-wide approximation. `setVelocity`, `setAngularVelocity` and
  `multiplyVelocity` likewise write native float32 body state only, ignore
  static bodies, and wake only for a non-zero resulting value; Lua velocity
  fields wait for the fixed-step write-back. In contrast `setPosition` and
  `setAngle` mirror their Lua fields and call `b2Body::SetTransform` at
  `sub_10086B794`, which synchronizes proxies and immediately runs
  `b2BroadPhase::UpdatePairs` without waking the body;
- the adjacent late object-extension bindings now preserve three additional
  directly observable native details. `native_setTimeSinceCollision` resolves
  to the 36-byte `sub_10005959C`, narrows its generated numeric argument to
  float32 and writes only `RenderObjectData+0x128`; it does not alter the
  similarly named Lua table field. `setRevertGravityWithMultiplier` resolves
  to the 64-byte `sub_10005962C`, requires string/boolean/number/number slots,
  stores the negated float at `+0x130` plus the float limit at `+0x134`, and
  creates no Lua mirror fields. `setSpriteRotation` (`sub_10003FC88`, 192
  bytes/3 blocks) receives float32, runs `fmodf` against a float32 two-pi and
  only then writes native and Lua sprite angles. The former 260-line mixed
  registration file is now a small order facade over focused
  `object_state.rs`, `radius.rs` and `runtime.rs` owners; the publication order
  visible at `0x10002C900..0x10002F334` remains unchanged;
- the contact filter at `sub_100065488` treats equal positive
  `P_COLLISION_GROUP` values as an exclusion, then applies sensor-type and
  controllable-object exclusions. When block collision is disabled, it reads
  the live `objects.world[name].collisionMaterials` list and the opposite
  object's material before allowing the fixture pair. This game-side filter is
  evaluated when a fixture-pair contact is created and whenever native
  `e_filterFlag` is consumed. Changing material/group fields alone does not
  retroactively refilter an existing contact, so a separated contact retained
  by its fat AABB may touch again under the old decision unless a joint or
  fixture operation marks it dirty. `setCollisionEnabled`
  (`sub_10004F3D8`) is deliberately different: both disassemblers show calls
  to `b2Body::SetActive(false)` and then `SetActive(true)` before the byte is
  stored. The first call removes proxies and synchronously destroys contacts;
  the second creates and buffers fresh proxies. It preserves velocity, force,
  torque and awake state. EndContact therefore observes the old Lua/native
  flag, while the next broad-phase AddPair observes the new value.

Theme-sprite creation/modification/removal/rotation, theme reset, render
disable/query, level-limit query, menu particle scale, gravity debug visuals,
light beam/ray commands, notification lifetime, URL/screenshot requests, and
AppData load/save calls likewise no longer fall through the missing-global
metatable. They use the recovered executable-Lua table grammar, isolated-table
execution and traversal-safe AppData paths. Legacy JSON output from earlier
rehost builds remains readable, and persistent files use the recovered native
AES container. The `addNotificationAfter` adapter also preserves its strict
`(string, number, string)` contract, returns the platform boolean, and keyed
removal/cancel-all match the direct native entries.

## Verified runtime checkpoint

The unmodified game scripts complete startup, IslandMap, Chapter01 and the
entire first-run intro comic. The corrected deterministic sequence loads
`levels/Chapter01/Chapter01_L01`, initializes `starTable`, switches to
`theme_hometree_bg` and emits `EID_LEVEL_LOADING_FINISHED`. A sling drag at
frame 9,200 reaches `Slingshot.lua`'s recovered `applyImpulse` path, emits
`EID_BIRD_SHOT`, and by frame 9,260 the mirrored `Stella_1` body has moved with
finite non-zero x/y velocity. Camera scale remains finite and converges instead
of entering the former NaN/blue-screen path. The draw stream contains the
playable level, tutorial/trajectory elements and native theme passes, with no
stale Chapter01 objects. Direct Lua removal from `objects.world` is treated as
the native scene lifetime boundary during transitions.

At the post-shot checkpoint the bird contacts the pink boot and wood triangle,
the normal impulse propagates through the nearby wood bodies, and later
contacts include `pig_medium_2`; the structure visibly deforms instead of the
bird passing through it. The level's 13 non-empty joint descriptors are also
created, including skateboard revolutes, hammock constraints, fruit welds and
the breakable pig/pillow weld.

The trace exposed two native parameter/state errors behind the earlier L01
result. Collision damage had been reading a fabricated top-level
`forceDamageMultiplier` and defaulting to `1`, while Purple reads
`worldAttributes.forceDamageMultiplier` (`2250` in the bundled definition).
After correcting that lookup, the shot initially damaged `pig_medium_2` from
60 to 27, exposing a second error: Rust's non-native `motion_started` gate left
every newly created structure body frozen until an explicit force arrived.
IDA `sub_100034740` and `sub_100034FB0`, independently confirmed in Hopper,
construct `b2BodyDef` with `allowSleep=true`, `awake=true`, `active=true` and
`gravityScale=1`; only the controllable-body branch subsequently calls
`sub_10086B88C(body, false)`. The L01 creation trace also shows no sleeping or
velocity setter for the target structure. Dynamic bodies now enter gravity,
contact and sleep solving on their first world step. With those corrections
plus native per-island solving and intrusive-list creation order, the current
exact-solver regression enters the episode at frame 4,600 and drags from
`(300, 450)` to `(152, 500)` at frame 9,200. It produces the intended chain
collapse, removes `pig_medium_2`, clears `levelGoals`, enters the ended state,
and renders the three-star `LEVEL 1 COMPLETED` screen. After the contact
solver, persistent `b2Sweep` centre, ResetMassData, the retail Weld/Revolute
joint paths and the retail narrow-phase manifold routes were all converted to
their recovered float32 state model. Completing the GJK/TOI and reduced-island
route moved the deterministic settling boundary: frame 12,000 is now still
inside score-panel animation, while frame 14,000 renders the stable three-star
screen at UI score 18,020. The documented input was repeated after the exact
Dirt fixture-rebuild path was restored; both independent 14,000-frame wgpu
captures produced SHA-256
`f9f32eb1898e93fe130920f5218b1ac82ff6fe5d790a6a71e869f1e4a0610d0c`.
This number remains a regression checkpoint rather
than an asserted original-iOS score oracle; it moved because the old discrete
path allowed impacts that Purple resolves at an earlier sweep fraction. The
independently loaded frame-1,800
L04/L06 checkpoints remain active at exact score zero; L06 retains both
`pig_medium_1` and `pig_medium_right_1`, eliminating the false no-input
roll-off caused by the old global solver pass.

The same `sub_100062520` comparison also established that an object's
`damageFactors` field must be a string. Purple resolves it through
`blockTable.damageFactors[name]`, then reads the target-material entries in
`damageMultiplier` and `velocityMultiplier`; an inline table is ignored.
The Rust path now follows that indirection and preserves the direct numeric
`powerupDamageMultiplier`. Box2D `ResetMassData` at `sub_10086B1F4` also
confirmed that only non-zero signed fixture densities contribute to aggregate
mass, and a dynamic body with non-positive aggregate mass receives the native
unit-mass/unit-inverse-mass fallback. Object creation, density changes, circle
resizing and body-type changes now share that behavior.

The L02 checkpoint now launches its first two infinite Stella birds, renders
the complete `TAP AND HOLD` tutorial/darkening/reticle sequence and executes
the parkour jump. IDA/Hopper comparison of the ray callback exposed and fixed
an important route difference: sensor fixtures had been allowed to intercept
the Rust aim ray and concave bodies had been collapsed to one hit. With the
native sensor exclusion and per-fixture records restored, the tutorial jump
passes the hidden sensor layer and throws the hammock pig along the recovered
path. A 15,800-frame trace invokes no remaining generic native no-op.

The non-linear content audit now also traverses all 65 Chapter01 and all 65
Chapter02 entry points, the genuine island-event unlock path into BirdRun, and
all six shipped `cutscenes` entries. The comic paths are driven through
`LevelLoad.transitionToLevel` and `startComic`, not by constructing a HUD
fallback. Every comic reaches its next page or destination menu during a
3,600-frame run without a Lua error; early, middle and late captures contain
the expected sequential panel animation rather than a static replacement.
The pig-pin/lock image visible over the pages is the real `CutsceneSkip` child
of `notificationsFrame`, not a stale IslandMap render object. The first six
playable minigame definitions other than `stella_glider` also start through
their original game-mode path. `stella_glider` itself contains a latent data
error in the shipped bytecode: its `sounds` table omits the `minIdleDelay` and
`maxIdleDelay` fields that the unmodified `BirdAnimation.apply` immediately
subtracts. The rehost deliberately records this rather than inventing a
native default that is absent from both the executable scripts and definition.

The current post-solver route remains reproducible without directly mutating
Lua state. Its exact 1024-by-768 logical input stream is: click `(165,245)` at
frame 700; drag `(300,450)` to `(152,500)` over 30 frames at 9,200; click
`(900,500)` at 9,245; click the stable next-level button `(650,705)` at
14,050; drag `(260,400)` to `(110,480)` over 30 frames at 15,600; then hold
`(570,620)` for 90 frames from 16,020. At frame 16,500 the
darkening/reticle sequence has cleared and the ability path has advanced. A
combined wgpu resource and native-noop audit of that route reports no missing
sprite, font, texture, non-finite command or generic compatibility call.
Repeating the route after the strict float32 theme-sprite conversion produced
`b2b1bab3b919f4e4603b220ee5510936d28c6df5395adb49ac45a9cca6b7fe26`
for the frame-16,500 wgpu capture; the established L01 frame-14,000 capture
remained byte-identical at
`f9f32eb1898e93fe130920f5218b1ac82ff6fe5d790a6a71e869f1e4a0610d0c`.

The current release checkpoints reach this same post-shot state without
invoking any generic native compatibility stub on the exercised path. A
separate deterministic 7,000-frame first-run check traverses startup and
reaches the interactive IslandMap. The 9,300-frame click/drag
checkpoint then traverses the complete intro comic, enters `Chapter01_L01` and
launches Stella. Render tracing on those routes reports no missing sprite,
texture or font; after the recovered descending animation z-order is applied,
Stella, the house, backpack, dust and panel borders are all visible over the
comic background.
Hopper's complete `sub_10002C274` registration assembly was also parsed as a
set rather than sampled by route. It contains 243 Lua-callable names before
the global data-field registrations begin; every one has an explicit binding
in `stella-script`, and every name in the former compatibility-noop list is
now replaced before the fallback loop runs. Startup frame 700, the settings
popup and the play-button transition were repeated after the namespaced-skin
fix with no missing sprite, texture, font, Lua exception or generic native
noop.
The remaining small platform branches are checked against their direct native
entries as well: the registration object is initialized to bytes `{0, 1}` and
therefore returns `(true, false)` in the native push order, while
`ForceUpdate.native_checkForcedUpdate` has a zero-result Lua ABI and stays
inactive when the discontinued cloud configuration is unavailable. The
Game Center object likewise initializes its availability byte to true and only
clears it after GameKit error 16, so the offline host reports the original
"supported but unauthenticated" state instead of conflating the two queries.
The installed-app bridge is also no longer represented as a boolean stub:
its offline entry validates the recovered `ttl/gameCount/game_n` JSON shape,
preserves the `setInstalledAppsOffline(commaSeparatedNames)` callback and
zero-result ABI, and yields an empty list because iOS URL schemes do not exist
on the desktop host. The discontinued online request keeps its strict string,
asynchronous zero-result contract; the separate Twitter query remains the
literal false returned by `sub_10005128C`.
The complete workspace now has 251 passing tests, including the 68-entry
registration guard and behavior checks for sensor force, fixture resizing,
joint parameters, object vertices, sprite rotation, render disabling and
full-atlas linear texture sampling plus composite axis flags/affine transforms.
The added animation regressions lock both selected-skin attachment transforms
inside entity bounds and exact namespaced attachment-alias lookup. Loader
regressions additionally lock native `blockTable` routing/replacement,
float32 definition `index/group` metadata, `IGNORE_COMPONENTS` inheritance,
raw named-child `gamelua`, zero-result loader adapters, unconditional bundle
suffixing, filename validation, JSON-in-place import, the four-slot string and
five-slot table text-file decrypt/decompress contracts, Lua-vs-JSON parsing,
and level force-multiplier fallback. The same workspace passes
`cargo clippy --workspace --all-targets -- -D warnings` and a release build of
every crate after the six-cutscene and IDA/Hopper loader audits.
Physics coverage now also includes synchronous pre-island Lua velocity
mutation, retained-contact wake propagation, common-minimum island sleep and
shared-static-body island separation/creation order, dynamic-tree leaf-id
allocation/reuse plus growth/balance/query integrity, pre-touch fat-AABB contact creation, dynamic-body pair
eligibility, contact-list-head callback order and callback mutation visibility
between consecutive contact updates in one `Collide` traversal. The active
state regression locks synchronous contact-list-head EndContact order,
motion/force preservation, `setActive`'s intentionally unmirrored Lua field,
and `setCollisionEnabled`'s deactivate/reactivate lifecycle with old-value End
callbacks followed by new-value AddPair filtering. Joint lifecycle coverage
additionally locks deferred `0x8` filtering,
the sleeping-contact delay, CreateJoint's no-wake behavior, DestroyJoint's
two-body wake, and clearing the dirty flag only after an awake recheck. Direct
body-flag tests also cover sleep clearing/no-op branches, sensor-change-only
wake behavior, fixed rotation, native-only damping/gravity writes,
head-fixture-only friction/restitution, and intentionally delayed Lua
mirroring. Transform coverage additionally verifies immediate pair creation
from SetTransform and preservation of sleeping endpoints. Fixture-resize
coverage now also locks signed polygon mirroring, independent native
width/height/radius storage, live-Lua coefficient/radius snapshots,
synchronous old-fixture EndContact timing, sensor restoration wake behavior,
compound fixture-list reversal, replacement AddPair creation, and the
post-visual-scale edge-shape error branch. Density/radius lifecycle coverage
additionally verifies head-fixture-only density writes, per-fixture aggregate
mass/center recomputation, resize-time synchronous EndContact, the absence of
Lua and sensor restoration, direct-radius replacement after prior physics
scaling, and EndContact-listener wake before the replacement BeginContact.
The Dirt lifecycle regression separately locks retained material coefficients,
per-triangle mass/proxy reconstruction, synchronous contact removal, and the
native fixture-head-before-contact-age destruction order for compound bodies.
It also locks unquantized source-float background/foreground construction and
the first-cut-only transition onto Clipper's 0.001 grid. The adapter regression
locks strict fixed-slot type failures and Dirt's collider-name-gated write into
the post-`removeBlocks` delayed velocity map, including the native float32
square/add wake decision at the subnormal underflow boundary.
The compound-vertex regression additionally locks reverse fixture-list
enumeration, the order flip after fixture recreation, signed resized vertices,
float32 position addition, and the native omission of body rotation.
Track-overlap coverage now also locks the nested `points` ABI, shape-skin
threshold, rejection of AABB-only diagonals and the head-fixture change after
compound fixture recreation.
Track-angle coverage additionally locks first-child tie handling, float32
projection/atan2, absence of an implicit closing child and the zero fallback.
The persistence regression also verifies native-shape Lua serialization,
AppData-vs-bundle existence, object-loader routing, level round trips and legacy
JSON migration. Resource lifecycle zero-result contracts, animation-shader
copy/clear behavior, animation local/world matrix and sprite-bounds semantics,
void lifecycle ABI, preload-cache isolation, and the bundled colorize
pixel-program order are covered by dedicated regressions. Animation rendering
also has a deliberately non-orthogonal parent/child matrix regression, which
would fail if the path returned to lossy scale/angle decomposition. The render
regressions additionally cover bitmap-font metrics, composite entry mutation,
clip/scissor conversion, cross-command immediate order, default and explicit
sprite anchors, destination-size stretching, and prevention of the
double-pivot layout regression visible on the settings screen. A pixel-level
capture regression also verifies immediate framebuffer copy and subsequent
sprite reuse. Dedicated transform regressions now also cover grouped partial
`setRenderState` updates, the legacy global composite helper, and rotated
renderer pivots under non-uniform scale all the way to generated wgpu
vertices. A render-boundary regression additionally uses an FMA-sensitive
operand triple whose native result is exactly `2^-46` but whose separated
multiply/add result is zero, while a large-coordinate case locks float32 input
quantization. The textured-line regression additionally covers the strict ten-value
adapter, ignored color tail, current-state alpha, native quad matrix and
sub-pixel rejection. At 9,300
deterministic frames the executable now
reports no unresolved callable `res`, `ResourceManager` or
`AnimationWrapperNative` method; the remaining 63 observations are optional.
This is stronger than merely suppressing missing-function errors: every native
entry reached by startup, menu navigation, first-level load, aiming, launch,
contact and next-bird return now resolves to a recovered typed implementation.
Dedicated regressions additionally lock the cached `worldAttributes` damage
multiplier, named `blockTable.damageFactors` lookup, rejection of the former
inline-table shortcut and Box2D's non-positive-mass fallback. Contact identity
is now also fixture-pair based rather than body-pair based: every convex piece
of a decomposed polygon and every edge of a line owns independent active,
warm-start, restitution and exit state, matching Box2D's `b2Contact` lifetime
and preventing complex bodies from silently losing `BeginContact` callbacks.
Existing fixture contacts are retained while both bodies sleep and only emit
their exit callbacks after actual geometric separation, filter rejection or
body removal; this removes the former false sensor exits on settled levels.
Off-center polygon bodies now also use Box2D sweep semantics: signed polygon
centroids become the local center of mass, polygon inertia is shifted from the
body origin to that center, contact and joint lever arms originate there, and
angular integration rotates the transform origin around the translated sweep
center without re-deriving or drifting that centre through the rounded
transform. Global force and impulse points use that same stored world center
when computing torque. This matters for the shipped triangular pieces whose
origin and center are observably different. Shape mass data, compound
fixture-head accumulation, aggregate centre/inertia, stored mass/inverse mass
and their reciprocals now follow the native float32 path; non-positive
aggregate mass retains the zero-center unit-mass fallback. Density/body-shape
mass resets keep the transform origin fixed and apply Box2D's
`cross(angularVelocity, newCenter-oldCenter)` correction to linear velocity
when the center moves.

The remaining non-identical native boundaries are backend-specific subpixel
rasterization and sampler rounding at a few
overlapping composite edges, the original process-address ordering of
separately malloc-allocated Box2D block chunks (within-chunk body slots and
LIFO reuse are now reproduced deterministically), device-specific audio
resampling/latency, physical video-device playback and discontinued
online/store services. Physical audio playback is implemented, but those
remaining areas cannot be
claimed as byte-for-byte equivalent without reproducing a specific GPU/driver,
allocator address layout, physical devices, or restoring external services.

The continuous-collision source layout now follows the smaller native Box2D
members rather than keeping the whole cluster in one 649-line unit. Hopper
reports Purple's `b2Distance` at `sub_1008605D4` as 1,676 bytes/61 basic
blocks, the exported `b2Simplex::ReadCache` at `0x100860C60` as 464 bytes/16
blocks, and `b2SeparationFunction::{Initialize,FindMinSeparation,Evaluate}` at
`0x1008620E4`, `0x1008624A4` and `0x100862898` as 960/1,012/576 bytes. Its
`b2TimeOfImpact` leaf at `sub_100861B54` is 1,424 bytes/28 blocks; IDA
independently confirms that size, block count and the float32 sweep/separation
state. The Rust facade is now 163 lines, with 116-line simplex, 134-line GJK
distance, a six-line separation facade, a 193-line three-member separation
object and a 110-line TOI advancement leaf. External world-solver entry points
and all float32 iteration/order semantics remain unchanged.

The `Initialize` assembly additionally loads `0x34000000` (`FLT_EPSILON`) from
`0x100A0C82C` before normalizing each separation axis. The one-point mode
explicitly zeroes a shorter axis; both face modes have already stored the
perpendicular edge and leave it unchanged when `b2Vec2::Normalize` returns
early. The previous shared Rust helper divided every non-zero vector. The
three modes now reproduce their distinct below-epsilon behavior, with a
focused boundary regression covering a half-epsilon point/face axis and an
exact-epsilon normalized point axis.

The contact velocity solver now mirrors the immediately adjacent Box2D member
cluster as well. Hopper isolates `InitializeVelocityConstraints` at
`sub_100863BC4` (1,000 bytes/21 blocks), `WarmStart` at `sub_100863FAC`
(292/7), `SolveVelocityConstraints` at `sub_1008640D0` (1,380/22), and the
small `StoreImpulses` leaf at `sub_100864634` (112/7). IDA independently
reports the main solve leaf as 1,380 bytes and 22 blocks, with the packed
float32 tangent and two-point complementarity state visible in its register
flow. The former 601-line Rust unit is now a seven-line facade over independent
initialization, warm-start, impulse-application, solve and StoreImpulses
modules. Cached feature alignment, restitution-bias snapshot order, the
1000:1 block-matrix guard, scalar fallback and two-point impulse order remain
covered by focused regressions.

The original solver also has two physically separate impulse stores. IDA and
Hopper show `WarmStart` reading the 152-byte solver constraint array, while the
112-byte `StoreImpulses` leaf walks that array only after every velocity
iteration and copies each point's normal/tangent floats into the contact
manifold at `+0x80/+0x84`. The rehost formerly updated its persistent
`contact_impulses` map during each solve pass, collapsing this observable
boundary. `solver_contact_impulses` now owns the per-island constraint state;
Initialize aligns cached features into it, WarmStart reads it, every Solve
pass mutates it, and StoreImpulses publishes it once at the native call site.
Hopper's caller/callee graphs additionally show that `StoreImpulses` has only
ordinary `b2Island::Solve` as a caller, while `b2Island::SolveTOI` invokes
Initialize and SolveVelocityConstraints but neither WarmStart nor
StoreImpulses. Reduced TOI islands therefore use zero-initialized solver-local
impulses without publishing them into the next-step manifold cache. Dedicated
regressions cover both the ordinary delayed-Store boundary and the TOI
no-publish path.

Contact lifecycle ownership is now split on the same binary boundaries. The
game filter remains tied to `sub_100065488` (Hopper: 1,752 bytes/86 blocks),
`b2Contact::Update` to `sub_10086373C` (608/31), the Collide traversal to
`sub_10086BAB0` (372/23), and awake world-island assembly to
`sub_10086E634` (1,024/42). IDA independently confirms Collide's 372-byte,
23-block loop: it skips fully sleeping pairs, consumes `e_filterFlag`, checks
the two fat AABBs, destroys a rejected node while preserving its saved next
pointer, and otherwise invokes Contact::Update before advancing. The former
596-line Rust unit is now a six-line facade over 60-line filtering, 241-line
refresh/event, 242-line island/sleep and 68-line focused-solve modules. Head
insertion traversal, callback re-entry visibility, sleeping-contact retention,
deferred joint filtering, static island termination and common minimum sleep
time remain covered by targeted tests.

The wgpu ownership layout has been tightened without changing its recovered GL
contract. `gpu.rs` now contains only the 109-line shared vertex/frame/renderer
storage ABI; its blend, shader, clip, mesh, pivot, capture and Dirt regressions
live behind the 16-line `gpu/tests.rs` facade in separate batch, program,
sprite and geometry suites. Immediate mesh submission itself is the independent
`gpu/frame/batch.rs` member rather than a large method block in the ABI. The
former 642-line renderer is a
six-line facade over a 98-line initialization coordinator, 83-line fixed-target
and bind-layout owner, 64-line native-program family and 104-line window
presentation constructor, plus fixed game-target draw/capture submission,
letterbox/RGBA presentation and texture synchronization. Those stages
modules preserve the already established binary ownership evidence:
`sub_100598CC4` supplies float32 GL state/viewport geometry, while command
ordering, render-pass splits and cached texture pairs remain separate from
surface/device policy. The capture regression still forces a pass break and
framebuffer copy before later draws; the program-identity regression exercises
all five prepared draw classes independently.

The 11,000-frame startup/menu/first-level/aim route was also rerun with the
renderer diagnostic path enabled after this split. It reports no missing
sprite, native/explicit quad sprite, bitmap font or prepared GPU texture and no
non-finite command skip; the callable fallback audit remains zero. This closes
the distinction between a successful script route and one that only succeeds
because the renderer silently omitted an unresolved resource.

Resource geometry now follows Purple's adjacent native members instead of a
single 626-line Rust unit. Hopper identifies the textured-line quad builder at
`sub_10006DB0C` as 572 bytes/7 basic blocks, the rubber-band builder at
`sub_100030EB0` as 564/3, the `drawSprite` Lua adapter at `sub_1004483AC` as
696/23 and atlas submission at `sub_100467A00` as 240/17. IDA independently
confirms the textured-line leaf's size/block count and float32 vector flow.
The new 12-line facade delegates to 146-line asset loading, 84-line composite
records, 160-line sprite drawing, 205-line line construction and 75-line data
model modules. Public call sites and registration order are unchanged.

The split also exposed a real precision mismatch. The strict Lua adapter at
`sub_100084A9C` narrows every line coordinate to float32, and
`sub_10006DB0C` uses only single-precision subtract/FMLA/square-root operations,
with a strict squared screen-length threshold of `1.0f`. The Rust path now
matches that narrowing and instruction order and carries the four independently
rounded atlas corners to wgpu rather than reconstructing them from an f64
affine matrix. A large-coordinate regression locks the observable
`16777217 -> 16777216` and `16777219 -> 16777220` endpoint rounding.

Joint ownership has likewise moved out of the former 552-line mixed unit.
Hopper and IDA both identify `sub_100862D60` (Solve33) as 180 bytes/3 basic
blocks and `sub_100862E14` (Solve22) as 64/3, with the singular-matrix zero
reciprocal and float32 cofactor order visible in both decompilers. Hopper keeps
the GameLua type/coordType dispatcher in the much larger `sub_100037374`
(18,240/243), while `b2World::CreateJoint` is an independent 272/15 member at
`sub_10086E470`. The Rust layout now mirrors those boundaries: a 16-line
facade over 50-line persistent state, 138-line anchor/prismatic geometry,
93-line matrix leaves and 283-line Lua/native construction. Existing sibling
solver imports still resolve through the facade, so public ownership and the
original construction/filtering order remain unchanged.

Scene-object collision queries are no longer collected in one 515-line impl.
Both decompilers report `b2TestOverlap` at `sub_10086021C` as a 184-byte,
single-block leaf that constructs two proxies, calls GJK and strictly compares
the use-radii distance with `1.19209e-6f`. Its four-shape proxy initializer at
`sub_1008602D4` is 148 bytes/12 blocks. Hopper additionally isolates the
track/chain distance wrapper at `sub_10003D208` (832/26) and the templated
dynamic-tree world ray-cast at `0x10086F834` (1,000/33). The Rust layout is
now a six-line facade over 108-line shape/b2Transform projection, a six-line
proxy-query facade, 109-line contact-factory manifold dispatch and 56-line ray
result modules. The proxy facade follows the actual native boundaries:
`distance_proxy.rs` owns the 148/12 `sub_1008602D4` shape switch, `overlap.rs`
owns the 184-byte/single-block TestOverlap leaf, `track.rs` owns the 832-byte
Hopper (872-byte IDA) chain wrapper, and `aabb.rs` owns shape bounds.

The latter also removes a previously hidden precision mismatch. IDA's
`sub_10086CA54` (172 bytes/4 blocks) invokes each shape's `ComputeAABB` virtual
before broad-phase proxy creation, while `sub_10086CB74` (284/4) computes the
two float32 bounds, combines them with ARM `FMIN`/`FMAX`, and subtracts the two
float32 body positions before MoveProxy. Accordingly transformed vertices,
circle center/radius, extrema and the `0.002f` polygon skin now remain in
float32 through AABB publication. A one-ULP regression at a small positive
coordinate distinguishes that native result from the former f64 `0.002`
addition. The chain fallback visible in `sub_1008602D4`—wrapping the second
endpoint to vertex zero at the final index—is retained, although the native
`vertexCount-1` child count keeps it outside ordinary fixture iteration.

This move removed a duplicate track-only coordinate transform that performed
separate float32 multiply/add/subtract operations. Purple's b2Transform path
uses the same nested ARM FMLA sequence already recovered for ordinary fixture
projection, so track distance now shares that implementation. A focused
large-coordinate regression makes the distinction observable: the native
path produces x=17,970,414 while separate rounding produces 17,970,416, enough
to flip the strict shape-radius distance result.

Contact teardown and post-collision ownership now follow their native members
instead of sharing the former 485-line Rust unit. Hopper and IDA both identify
`b2World::DestroyJoint` at `sub_10086E27C` as 356 bytes/29 basic blocks. Its
control flow removes both intrusive body edges, wakes both endpoints, releases
the constraint, and sets the surviving pair's `e_filterFlag` only when
`collideConnected` was false. The body/fixture-side contact destruction leaf
at `sub_10086B9B8` is 248/22 and retains the already reproduced descending
contact-edge creation order.

GameLua sensor bookkeeping remains distinct from Box2D teardown. The broader
enter/damage dispatcher is `sub_100062520` (8,992 bytes/247 blocks), while IDA
and Hopper isolate sensor exit at `sub_10006525C` as 336/29: it removes every
matching sensor pointer in place and clears `insideGravity` only when the
opposite object's sensor vector becomes empty. Pending collision velocities
belong to the larger post-step physics member `sub_10005E898` (6,476/237), not
to either teardown leaf; its packed float32 square/add wake test therefore
stays with collision-force dispatch.

The Rust facade is now 11 lines over 122-line joint lifecycle, 49-line delayed
removal, 136-line contact drain, 48-line sensor membership and 140-line
collision-force modules. The split changes no function body or event order;
focused regressions continue to cover joint-filter deferral, reverse-creation
EndContact dispatch, fixture destruction, gravity sensor entry/exit, breakable
joints, delayed object removal and collision-force/velocity float32 behavior.

The test-only CPU rendering oracle now mirrors the same recovered GL ownership
as the wgpu backend rather than remaining a 797-line mixed source file. Hopper
reports the viewport/state member at `sub_100598CC4` as 820 bytes/11 basic
blocks and atlas UV submission at `sub_100467760` as 536/13. IDA and Hopper
independently agree that the four-corner sprite transform/submission member
`sub_100467BE8` is 540/15; its decompilation transforms each vertex in float32
before passing the complete twelve-float array to the renderer interface.

Texture setup is an independent 240-byte/6-block leaf at `sub_10018AFB8` in
both tools. It binds the texture-coordinate array, optionally loads the texture
matrix, resolves MIN and WRAP state, and fixes `GL_TEXTURE_MAG_FILTER` to
`GL_LINEAR`. Those boundaries now appear directly in Rust: the 19-line facade
owns 107-line command traversal, 95-line colored meshes/blending, 221-line
sprite/explicit-quad rasterization, 56-line texture sampling, 38-line pixel
shader emulation and 22-line letterbox presentation modules; the 249 lines of
pixel regressions are isolated in `tests.rs`. All arithmetic expressions and
the two test-only `assets.rs` entry points remain unchanged, and all 22
reference/wgpu render tests pass after the move.

Host-side asset ownership now follows Purple's KA3D members too. Hopper places
the SPRT/COMP binary loader at `sub_100461B98` (3,100 bytes/88 basic blocks),
the full composite draw/matrix path at `sub_1004376D4` (1,288/29), and the
bitmap-font draw member at `sub_10042B338` (1,060/58). IDA includes the latter
member's adjacent exception tail and reports 1,068 bytes/61 blocks, but agrees
on its glyph lookup, anchor switch, float32 cursor and AtlasSprite submission
body. Perspective creation and model installation are independent again:
`sub_10057BAE0` is 156/3 in both tools, while Hopper reports the straight-line
`sub_10057BE78` model member as 260/1.

The former 599-line `stella-app/assets.rs` now leaves its data model and exports
in a 50-line facade over 91-line catalog loading, 29-line lazy texture decode,
103-line sprite/composite dispatch, 150-line bitmap-font drawing and 202-line
renderer-neutral transform modules. This is a source-ownership change except
for one instruction-order correction found during the audit: the perspective
Y matrix element is no longer algebraically shortened to `focal * -1.33f`.
It now reproduces `sub_10057BAE0`'s actual FADD→FMUL→FMUL→FDIV sequence through
the doubled 0.001 near plane. The shipped constants happen to produce the same
final bit pattern, but the code and its projection regression now preserve the
native float32 construction rather than relying on that coincidence.

The shared KA3D parser has now been divided at those same binary ownership
boundaries. A complete scan of the shipped data finds 114 KA3D/RVIO envelopes:
70 `KA3D SPRT` v1, 24 `KA3D FONT` v1, 11 `KA3D COMP` v2, eight `RVIO COMP` v1
and one `KA3D TEXT` v1. The encrypted promotion configuration is not a KA3D
resource and remains on the existing configuration path.

IDA and Hopper independently place the combined SPRT/COMP native loader at
`sub_100461B98`. Hopper reports its contiguous implementation body as 3,100
bytes/88 basic blocks; IDA includes the C++ exception and cleanup islands and
reports 4,084/176. The TEXT loader is independently rooted at
`sub_1004731B0`: Hopper reports 2,696/123, while IDA's exception-inclusive
boundary is 3,192/196. Its referenced diagnostics include the missing-LIDS
before-TXGP check, confirming the recovered `LDAT` -> `LIDS` -> one `TXGP` per
locale order rather than treating localization as an SPRT/COMP variant.

Accordingly, the former 547-line `stella-assets/src/ka3d.rs` is now a 17-line
public facade over a 35-line envelope validator, 76-line checked big-endian
reader, 53-line SPRT parser, 98-line COMP v1/v2 parser, 65-line FONT parser and
68-line TEXT/localization parser; its 156 lines of synthetic boundary tests
live separately. No record field, version rule or error boundary changed in
the move. All 114 real envelopes also pass subtype inspection, covering every
format/version combination actually present in Purple's data.

The outer continuous solver is now separated along the three Box2D members
that drive that already recovered TOI core. IDA and Hopper agree exactly on
all three boundaries: `b2World::SolveTOI` at `sub_10086EA54` is 2,436 bytes/77
basic blocks, reduced `b2Island::SolveTOI` at `sub_10086D5EC` is 992/29, and
`b2ContactSolver::SolveTOIPositionConstraints` at `sub_1008649BC` is 832/15.
The latter's decompilation again shows the 0.001 linear slop, 0.2 correction
clamp and -0.0015 convergence test, while the island member owns the position
iteration, velocity iteration and remaining-substep integration sequence.

The former 448-line `continuous_solver.rs` is therefore a five-line facade
over a 260-line world candidate/contact-edge module, 98-line TOI position
module and 100-line reduced-island module. This move preserves the earliest
alpha sort, one-contact-at-a-time BeginContact callback boundary, 20 position
iterations, warm-start reset, velocity solve and `(1-alpha)*dt` integration
order. Focused regressions still cover high-speed tunnelling, a second impact
within one step, simultaneous static corner contacts and listener mutation of
the next auxiliary contact.

The embedded dynamic tree is now split at its own native members instead of
remaining a 411-line aggregate. IDA and Hopper agree exactly on the recovered
boundaries: the constructor at `sub_100860E30` is 172 bytes/4 basic blocks,
node allocation at `sub_100860F08` is 260/6, `CreateProxy` at
`sub_10086100C` is 156/1, `InsertLeaf` at `sub_1008610A8` is 784/16,
`DestroyProxy` at `sub_1008613B8` is 84/1, `RemoveLeaf` at
`sub_10086140C` is 404/12, `MoveProxy` at `sub_1008615A0` is 320/8, and
`Balance` at `sub_1008616E0` is 1,140/25.

The short-member decompilations independently reconfirm the sixteen-node
initial free chain, capacity doubling, 0.1 fat-AABB expansion, signed
`displacement + displacement` extension and destroyed-node return to the
free-list head. The Rust layout is now a 13-line facade over 92-line model,
42-line allocation, 101-line proxy/query, 74-line insertion, 38-line removal
and 87-line balance modules. No comparison or mutation order changed; the
growth/balance/full-free-list, native proxy-ID, LIFO query, fat-AABB contact
and immediate pair-drain regressions all pass after the move.

The world-facing broad-phase bridge now mirrors the wrapper members around
that tree rather than remaining one 305-line aggregate. IDA and Hopper agree
exactly on body creation `sub_10086DF90` (156 bytes/9 blocks), fixture proxy
creation `sub_10086CA54` (172/4) through broad-phase wrapper
`sub_10085E310` (144/3) and tree `CreateProxy` `sub_10086100C` (156/1),
fixture synchronization `sub_10086CB74` (284/4) through wrapper
`sub_10085E3E8` (132/5) and `MoveProxy` `sub_1008615A0` (320/8), and
`b2BroadPhase::UpdatePairs<b2ContactManager>` at `0x10086BDB0`
(1,100/66).

`broad_phase_bridge.rs` is consequently a six-line facade over 25-line world
allocation ordering, 167-line body/fixture proxy lifecycle, 61-line swept-AABB
synchronization, and 65-line sorted/deduplicated pair generation modules.
Reverse fixture activation, dynamic-tree free-list reuse, move buffering,
signed proxy-pair order, collision filtering and contact creation order are
unchanged. All ten broad-phase and 28 contact-order focused regressions pass
after the move.

The remaining 433-line wgpu frame coordinator is now split along the recovered
immediate-render paths as well. IDA and Hopper agree on capture at
`sub_100458F54` (800 bytes/37 basic blocks), atlas UV construction at
`sub_100467760` (536/13), atlas pivot/setup at `sub_100467A00` (240/17), the
four-vertex transform at `sub_100467BE8` (540/15), and the explicit masked
quad path at `sub_1000343CC` (320/8). For `DrawablePolygon::rebuild` at
`sub_1000246A8`, Hopper reports the 744-byte/36-block main body while IDA
includes the adjacent cleanup tail and reports 784/40.

The new 13-line `gpu/frame.rs` facade owns a 182-line single-order/capture
command walk, 105-line native/explicit quad submission and 153-line
atlas/composite/Dirt dispatch modules alongside the existing 149-line text
and 267-line geometry units. The parent import remains in the facade because
it is the shared render ABI boundary for all five children; no draw formula
or operation order moved across it. All 22 GPU/reference tests still cover
mixed command ordering, pass-breaking capture, four rounded atlas positions,
explicit position/UV pairing, Dirt material UVs, pivot/matrix precision,
blend states and projected bitmap text.

The animation transform stage now follows the entity-query and matrix members
instead of remaining a 426-line mixed unit. IDA and Hopper agree exactly that
local position `sub_100014D84` is 188 bytes/9 basic blocks, local basis scale
`sub_100015000` is 212/9, sprite bounds `sub_1000152BC` is 828/25, the four
local/world position/scale adapters at `sub_100015AB8`, `sub_100015C78`,
`sub_100015E38` and `sub_100015FF8` are each 268/11, and the bounds adapter
`sub_1000161B8` is 308/11. Full affine multiplication at `sub_10001E440` is
460/7, while SpriteComponentCustom attachment canonicalization at
`sub_1002913BC` is 284/20.

The bounds and matrix pseudocode reconfirms two important existing contracts:
entity “world” matrices are animation-scene-relative, and bounds multiply the
sprite half-width/half-height by the two final basis-column magnitudes rather
than rotating four corners. The new 16-line facade delegates to 78-line track
hierarchy, 97-line affine, 69-line skin attachment, 119-line query/bounds and
92-line render-command modules. The selected/default skin fallback, exact
namespaced alias lookup, uppercase component name, full parent-child shear,
descending z-order and compatibility scale/angle fields remain unchanged.

The renderer-neutral per-frame bridge is now separated at the same GameLua
and Particles member boundaries. IDA reports the exception-inclusive
`sub_10005E898` frame dispatcher as 7,756 bytes/354 basic blocks, while Hopper
keeps its principal body at 6,476/237. Both tools agree exactly on the
independent packed-particle update `sub_100091834` at 1,372/48, theme update
`sub_1000607E8` at 1,228/42, and particle virtual draw member
`sub_100091D90` at 808/16. The four GameLua draw entry points are likewise
separate: background `sub_10004C578` and foreground `sub_10004C5A8` are each
48/3, while menu `sub_10004C5D8` and notification `sub_10004C5FC` are each
36/1.

Their disassembly confirms the native mode mapping `{background: 2,
foreground: 1, menu: 3, notification: 4}` and that GameLua+0x199 gates only
the first two. The virtual draw member selects its world-transform branch only
for modes 1/2; modes 3/4 keep framebuffer positions and multiply scale by
Particles+0x38. The former 417-line `frame_update.rs` is now a six-line facade
over focused particle integration, particle rendering, scene reporting/bounce
and theme modules. The former 195-line mixed theme unit is itself an 18-line
native-order facade: `theme/layers.rs` owns the two `sub_10009B8B4` passes and
`theme/sprite_data.rs` owns `sub_1000607E8`. Update ordering, float32
integration, lifetime erasure, motion thresholds, tile wrapping and command
submission remain explicit at their recovered member boundaries.

The former mixed `textured_render_registration.rs` has likewise been split at
five boundaries visible independently in both disassemblers. IDA and Hopper
agree on `drawTexturedRect` member `sub_100043D6C` at 444 bytes/3 basic blocks
and adapter `sub_100085230` at 396/13; selected-texture member
`sub_100043990` at 624/16 and adapter `sub_100085634` at 564/23; direct masked
member `sub_1000343CC` at 320/8 and adapter `sub_100087FC0` at 560/13; 3D text
member `sub_10003457C` at 452/1 and adapter `sub_100087BB4` at 640/23. The
hand-written nine-slice member `sub_100051BBC` is the larger independent unit
at 7,356/173, while masked quad construction is `sub_100096344` at 976/9.

The generated adapters all cross `sub_10052859C` and therefore quantize Lua
numbers to float32 before the members use them. `sub_100087FC0` then applies
`FCVTZS` only to its first eight masked coordinates; `sub_100096344` builds
NDC in float32, performs its half-range/factor work in double, and stores the
UV result back to float32. The selected member performs translation, world
scale and local `x*20/scale` arithmetic in float32. The nine-slice member
reads slots 2..7 through the same strict helper, accepts numeric strings only
in its hand-written optional color table, and executes `FRINTM` on each target
rectangle's x, y, width and height before submitting it.

Rust now keeps a small ordered facade with focused `textured.rs`,
`selected.rs`, `masked.rs` and `text_3d.rs` children. `box_draw.rs` is now a
31-line facade over 111-line Lua arguments, 119-line native layout/cull,
64-line resource query/submission and 24-line background leaves. The same
reverse pass also corrected
the shared rectangle helper so color multiplication and destination
subtraction occur at native float32 precision before `FCVTZS`. Precision
regressions cover selected-object division, 3D alpha, masked coordinate/UV
rounding, per-piece nine-slice flooring and Lua 5.1 color-string coercion.

The trajectory registration aggregate is now split around the three stores
described above. IDA and Hopper agree on raw-vector clear `sub_1000311B4` at
12 bytes/1 block, vector query `sub_1000311C0` at 368/7, one-body prediction
`sub_100032970` at 928/42, aiming-time query `sub_10004B8EC` at 340/4,
AimStream clear `sub_10004BA70` at 44 bytes (IDA counts 2 blocks and Hopper
1), AimStream draw `sub_10004C4CC` at 88 bytes (8 versus 7 blocks), trail
switch `sub_10004FD3C` at 316/13, point append `sub_10004FF14` at 108/7 and
puff append `sub_10004FF80` at 56/1.

A complete second pass over the containing `sub_10002C274` constructor fixes
the aggregate's relative order as well. Both tools place clear/query/update at
`0x10002CBA0/0x10002CBC0/0x10002CD50`, then `populateAimingAid` and the
selected simulation bird at `0x10002CD80/0x10002CDB0`; aiming time and clear
are `0x10002DD08/0x10002DD38`, draw is `0x10002DEE8`, the three trail record
operations are `0x10002E644..0x10002E6AC`, and sprite adapters occur much
later. In that final cluster the native order is `setNormalTrailSprite`,
`setSpecialTrailSprite`, `setAimingAidSprite` at
`0x10002EDAC/0x10002EDE0/0x10002EE14`, correcting the earlier aggregate's
aiming-first assumption. The top-level file is now a native-address facade;
raw access, the cohesive predictor, simulation selection, AimStream control,
AimStream draw, trail points and sprite slots are separate nested leaves.
All 11 trajectory-focused regressions and all 297 workspace tests pass after
the move; strict Clippy and the release build are clean. The 11,000-frame
`audit-trajectory-native-order.png` menu-to-level route reaches the aiming-aid
sequence with zero invoked compatibility fallbacks.

Scene rendering is separated at its unrelated theme/object members. IDA
reports theme traversal `sub_10009BDB4` at 1,808 bytes/26 blocks, object draw
`sub_10006D5B4` at 888/22 and the Flash-animation callback
`sub_10006794C` at 184/2. Hopper agrees on the latter two byte sizes (counting
the Flash body as one block), but currently coalesces the theme tail with the
small jump entry `sub_10004C4A4`; its returned block list still starts the
real body at `0x10009BDB4`. The former 376-line mixed file is consequently a
4-line facade over 156-line theme traversal and 206-line scene-object modules.

Particle ownership now matches the separate native random source and
emitter. Both tools report `sub_10008E524` as 9,720 bytes/170 blocks and the
process-global xorshift/CMWC generator `sub_10057B42C` as 264/6. The former
334-line file is a 9-line facade over a 30-line packed model, 60-line random
source and 220-line deterministic emitter. Random-consumption order is
unchanged and all eight particle regressions pass.

Persistence is split between the three GameLua file members and the Lua 5.1
serializer family. IDA/Hopper agree on `sub_10004B394` at 552/15,
`sub_10004B6D0` at 304/5, `sub_10004B880` at 84/1 and scalar formatter
`sub_10052A490` at 1,000/30. Hopper keeps the principal string-escape body
`sub_10052A020` at 732/28 while IDA includes its cleanup tail at 980/35; the
top-level serializer `sub_10052A8FC` is respectively 1,108/37 and 1,112/39.
The former 323-line aggregate is now a 9-line facade over 117-line file I/O
and 215-line serialization modules without changing AppData compatibility.

The mixed native-draw installer is now divided at the same member boundaries.
IDA and Hopper agree that the Z-range setter `sub_10004BAA8` is 12 bytes/1
block, its generated Lua adapter `sub_100084DA8` is 172/5, the scene dispatcher
`sub_10004BAB4` is 2,388/65, the flight-trail pre-pass `sub_10006D9C0` is
332 bytes (IDA counts 12 blocks and Hopper 11), direct textured-line member
`sub_10004DB90` is 156/5, textured-line quad builder `sub_10006DB0C` is 572/7,
rubber-band member `sub_100030EB0` is 564/3 and platform video member
`sub_100051B60` is 92/1. The generated line and rubber-band adapters are
`sub_100084A9C` at 492/13 and `sub_1000897A4` at 404/13.

The adapter disassembly also corrected two precision contracts. Z limits pass
through strict NUMBER guards, narrow to float32 and execute `FCVTZS` before the
12-byte member stores them. The trail pre-pass divides each stored point by
`GameLua+0x194` and the caller separately installs `-topLeft/gameWorldScale`
translation followed by `worldScale*gameWorldScale` scaling; Rust now preserves
that operation sequence rather than replacing it with an algebraically similar
float64 expression. The hand-written pre/post callback members
`sub_10004E3C0` and `sub_10004E570` are each 304/14: slot 1 is a strict STRING,
missing or nil slot 2 clears, and every other value must pass the FUNCTION
guard without mutating the prior callback on failure.

The former 310-line `draw_registration.rs` is consequently a 31-line ordered
facade over 86-line platform/Z-range/callback adapters, 155-line scene/trail
dispatch and 82-line textured-line/rubber-band modules. Constructor order is
unchanged, and focused regressions cover float32-to-integer Z selection,
transactional callback errors, both trail buffers and native line ABIs.

The former mixed `script_runtime.rs` now follows the independent GameLua file
members and host-only routing boundary. For `loadLuaFileToObject` at
`sub_10005761C`, Hopper reports the 1,104-byte/43-block principal body while
IDA includes exception cleanup through 1,452/85. The AppData variant
`sub_100057E14` is 1,276/55 in Hopper and 1,284/83 in IDA. Definition-pack
loading `sub_100058960` is likewise 808/29 versus IDA's exception-inclusive
1,084/53. Both tools agree exactly on definition indexing `sub_100062028` at
812/23 and the direct table-assignment leaf `sub_10007F33C` at 120/1.

The object-loader disassembly reads slots 1 and 3 through the strict STRING
guard and retains slot 2 as the Lua object environment. `sub_10005761C` reads
its resolve-relative boolean only when the stack top is exactly four and
otherwise defaults it to true. `sub_100057E14` reads optional slots 4, 5 and 6
when present, defaulting resolve/decrypt/unzip to false/true/false. Rust now
enforces the table environment contract, preserves the exact-four rule, and
routes the latter decrypt and unzip flags into persistent-Lua decryption and
first-entry 7z extraction instead of ignoring them.

The previous 302-line aggregate is now an 11-line facade over 64-line chunk
preparation/execution, 83-line object environments, 123-line definition-pack
loading/indexing and 94-line safe host paths. The adjacent 93-line registration
adapter owns only the native Lua ABI. Definition replacement/index annotation,
raw child `gamelua` publication, encrypted AppData loading and loader type/error
regressions all pass after the split.

The shared immediate-geometry aggregate is now split at the native member
boundaries rather than by source length. IDA and Hopper agree on rectangle
member `sub_100043C14` at 344 bytes, polygon entry `sub_100043F28` at 740,
line `sub_10004DC44` at 72, rectangle-outline `sub_10004DC8C` at 276 and the
direct-sprite matrix builder `sub_10006C838` at 720. The mode-1
`DrawablePolygon` constructor body `sub_100024204` is 728 bytes; IDA reports
its rebuild `sub_1000246A8` at 784/40 while Hopper keeps the principal body at
744/36. Both tools agree on the 1,384-byte fill `sub_100024A50` and
308-byte/6-block outline `sub_100025054`.

The fill decompilation walks the rebuilt vertex vector in groups already
prepared as triangles, and performs its 20:1 projection in float32. The
outline walks the original vector pairwise, executes float32 `(point +
offset) * 20` followed by integer conversion, calls the context line member at
width one for every edge and finally closes last-to-first. Constructor stores
confirm the outline defaults to `[0, 0, 0, 255]`. Rust therefore reuses the
same recovered winding normalizer/quality-ranked ear cutter as the Box2D
polygon bridge, adds an explicit `TriangleList` command topology to both the
CPU reference renderer and wgpu expansion, and preserves fill-before-outline
submission order. The former 297-line `render_primitives.rs` is now a 20-line
facade over focused color, transform, sprite, rectangle, line, polygon and
software modules (10, 40, 57, 58, 68, 123 and 58 lines).
Generated line arguments now narrow to float32 before `FCVTZS`, direct-sprite
trigonometry/matrix products stay float32, and regressions cover rounding
boundaries, concave triangle lists, nonzero mesh bounds and the closed black
outline. Strict workspace Clippy, all 270 tests and the release build pass;
`audit-render-primitives-split-menu.png` and
`audit-render-primitives-split-start.png` verify the wgpu menu/start flow.

The remaining GameLua base-registration aggregate now follows the independent
members referenced by constructor `sub_10002C274`. IDA and Hopper agree that
notification enable `sub_100034070` is 492 bytes/25 blocks, with generated
adapter `sub_10008962C` at 140/5. Add, keyed remove and cancel-all are the tiny
platform-forwarding members `sub_100034398`, `sub_1000343A0` and
`sub_10006EB78`; IDA keeps each principal body at 8 bytes, while Hopper counts
the add tail target as 12 bytes/2 blocks. Their adapters are
`sub_100088348` and `sub_100088F68` at 104/3, plus the shared zero-result
`sub_10008A07C` at 120/5.

The constructor independently registers hand-written unique-shader creator
`sub_10004E720` (968/31), destroy member `sub_10004EC88` (620/26) through the
104-byte/3-block `sub_100084204` adapter, and `clipText` member
`sub_10004F630`. IDA includes 1,504 bytes for the latter while Hopper reports
the 1,500-byte/91-block principal body. Inspecting the exact constructor
instructions also corrects an older finding: the adapter immediately following
the `clipText` string is `sub_100086070` (104/3), not adjacent
`sub_1000860D8`. `fileExistsInAppData` remains the separate 8-byte
`sub_10005A290` leaf.

The former 352-line `registration.rs` is consequently a 132-line ordered
facade over 61-line text clipping, 67-line notification state and 131-line
late compatibility/unique-shader modules. Resource construction and the order
of the large subsystem installers remain visible in the facade, while the
missing-global metatable stays at the final compatibility boundary. The
notification delay now also narrows at the recovered float32 boundary before
being stored by the offline platform mirror. Strict workspace Clippy, all 270
tests and the release build pass; `audit-registration-split-menu.png` and
`audit-registration-split-start.png` verify the unchanged wgpu constructor
flow.

The DirtMechanics aggregate is now split along its own native object members.
IDA and Hopper agree exactly on constructor `sub_10001F98C` at 1,948 bytes/37
blocks, collision queue `sub_100020560` at 520/19, fixture point query
`sub_100020858` at 124/8, draw traversal `sub_1000208D4` at 64/3 and direct
vertex-vector copy `sub_100021B58` at 236/16. IDA includes 2,812 bytes for the
Clipper cut member `sub_100020D70`, while Hopper reports its 2,808-byte/137-
block principal body.

The former 356-line `dirt.rs` is now an 11-line facade over 80-line component
state/constructor, 127-line Clipper cut, 30-line DrawablePolygon triangle
stream and 114-line Lua definition/material reconstruction modules. The split
keeps the constructor's unquantized float32 source paths separate from the
first-cut 1,000x integer grid, and keeps the background/foreground draw model
separate from fixture reconstruction. All focused cut, collision, mass/proxy,
source-contour and triangle-order regressions pass after the move. Strict
workspace Clippy, all 270 tests and the release build pass;
`audit-dirt-split-menu.png` and `audit-dirt-split-start.png` verify the wgpu
menu/start flow.

The Lua-facing DirtMechanics registration now follows those recovered object
members too. The former 307-line `native_block_registration.rs` aggregate is
a 21-line ordered facade. Its 49-line `factory.rs` owns only the strict
`sub_10005A9F0` two-string registry adapter; 59-line `collision.rs` owns the
bound `sub_100020560` eight-slot collision adapter and delayed-velocity write;
44-line `queries.rs` owns `sub_100020858` fixture TestPoint and
`sub_1000208D4` render synchronization; and 186-line `rebuild.rs` owns queued
collision draining, `sub_100020D70` clipping, head-first fixture destruction,
synchronous EndContact delivery, proxy replacement and per-fixture mass
reset. This separates the Lua method table from the observable Box2D mutation
sequence without changing either order. All eight focused Dirt/fixture
regressions pass after the move.

The later track/joint extension aggregate is now split by the actual
`sub_10002C274` registration clusters. IDA reports direct track overlap
`sub_10003D208` at 872 bytes while Hopper keeps its 832-byte/26-block principal
body. IDA's `destroyTrack` member `sub_10003D638` is the 24-byte entry body;
Hopper follows its external tail chunks and consequently reports 244/21.
Both agree on the 124-byte/4-block current-angle member `sub_10003D650`.
The corresponding generated adapters are the 104-byte/3-block strict-string
`sub_100089E6C` and result adapter `sub_100086D08`.

`setJointParameters` is the independent hand-written `sub_10003E890`
(3,628/45). Its switch is on the concrete Box2D joint type: distance joints
accept `frequency`, `dampingRatio` and `length`; revolute and prismatic joints
accept `motor`, `motorSpeed`, `maxTorque`, `limit`, `lowerLimit` and
`upperLimit`; the other joint families accept none. Every accepted scalar is
narrowed into an `s` register before the native member and Lua descriptor are
updated. Rust now uses the same type dispatch and publishes the exact float32
value instead of retaining the input double. Object detachment
`sub_1000442D8` is 2,216/85, while `checkJointLimits` `sub_100054CD4` and
`handleJointLimits` `sub_100054DAC` are respectively 132/6 and 780/32.
IDA independently reports the same byte and block sizes and complexities
25/49/4/27 for the parameter, removal, check and handle members. The 17-line
Rust `joints.rs` therefore preserves only publication order; its 179-line
`joints/parameters.rs`, 30-line `removal.rs` and 35-line `limits.rs` children
own those native boundaries. Parameter field filtering and float32 descriptor
mirroring, synchronous EndContact/removal publication, and the two distinct
limit entry ABIs remain in their original order.

The five object-flag registrations all use the same 104-byte/3-block
`sub_10008598C` `(string, bool)` adapter. Their members are 36-byte/1-block
leaves: block collision `sub_10004F4B0`, keep orientation `sub_100059578`,
ignore score `sub_1000595C0`, record velocity `sub_1000595E4` and reverse
gravity `sub_100059608`. Each resolves `RenderObjectData` and writes exactly
one byte; none rewrites `objects.world`. This also exposed `mlua`'s numeric-to-
string conversion at typed `String` arguments, so all members in this strict
adapter family now use explicit generated-adapter checks. Object vertices
`sub_10005A7BC` remains 456/7 behind its own 104/3 adapter
`sub_100082F9C`.

The former 354-line `track_joint_registration.rs` is consequently a 20-line
native-order facade over 77-line track, a 17-line joint sub-facade, 48-line
native-flag and 53-line vertex modules. A focused regression covers strict adapter types,
native-only flags, concrete-joint field filtering and descriptor/native
float32 agreement. Strict workspace Clippy, all 271 tests and the release build
pass; `audit-track-joint-split-menu.png` and
`audit-track-joint-split-start.png` verify the wgpu menu/start flow.

The PhysicsWorld object constructors are now separated at the recovered native
binding boundaries as well. `sub_10002C274` registers box member
`sub_100034740`, circle `sub_100034FB0`, polygon `sub_1000357A4`, line
`sub_1000364E0` and non-physics `sub_100036D38`. Hopper reports their principal
bodies at 2,008/34, 1,884/33, 1,908/37, 1,908/37 and 1,520/25 bytes/basic
blocks respectively; IDA's exception-inclusive extents are 2,008, 1,892,
1,912, 1,912 and 1,528 bytes. The common line-shape builder is
`sub_100068130` (440/10 in Hopper, 468 bytes including IDA tail chunks).

The outer generated entries `sub_100087704`, `sub_1000872D0` and
`sub_100086F24` are each 104-byte/3-block adapters. Their nested bodies
`sub_10008776C` (700/23), `sub_100087338` (680/23) and `sub_100086F8C`
(544/23) prove strict positional ABIs: box/polygon/line read two strings,
seven floats, two booleans and a final float; circle reads two strings, six
floats, two booleans and a final float; non-physics reads two strings and three
floats. Every Lua number is narrowed to float32 before the member call.

Accordingly the former 343-line mixed `construction.rs` is an 83-line facade
and data model over a 134-line strict-adapter module, 110-line shape/body
preparation module, 22-line common commit entry, 61-line `objects.world`
mirror and 142-line SceneObject/broad-phase installation leaf. The split keeps
native argument conversion separate from geometry consumption and prevents
Lua-mirror logic from becoming coupled to body allocation. The focused
constructor regression now rejects coercible strings, wrong booleans and short
arity, and locks the stored coordinates, dimensions, material values and z
order to the native float32 boundary.

Strict workspace Clippy, all 273 tests and the release build pass after that
constructor split. `audit-construction-split-menu.png` and
`audit-construction-split-start.png` also verify the unchanged wgpu menu and
started-scene resource flow.

The polygon decomposition implementation is now split on the native algorithm
members rather than remaining one mixed 345-line file. Direct MCP queries to
both tools place the winding/entry wrapper at `sub_10087007C` (156 bytes/4
blocks in both), repeated-point recursion at `sub_1008710C0` (932/33), the
ear validator at `sub_100871D5C` (404/21), the quality-ranked ear cutter at
`sub_100871498` and convex merger at `sub_100871F08` (976/53). Hopper keeps
the ear cutter's principal body at 1,984 bytes/83 blocks, while IDA includes
2,152 bytes/102 blocks of exception/tail chunks. Hopper reports top-level
`sub_100872360` as 592/25; IDA includes 632/30. The direct convexity predicate
`sub_100870520` is 168/7 in both tools.

The resulting layout is a 61-line facade over a 121-line winding,
repeated-vertex and ear-cutting leaf, a 105-line float32 geometry/predicate
leaf and a 100-line triangle-merge/final-simplification leaf. This keeps the
`sub_100871F08` eight-point output rule and `sin(2 degrees)` cleanup separate
from the recursive `0.001` repeated-point split, while the Lua utility,
DrawablePolygon and actual fixture construction continue to share the same
public pipeline. All 15 focused polygon/decomposition/draw regressions pass
after the move.

SceneObject body ownership is now divided at the corresponding Box2D members.
Direct IDA/Hopper MCP comparison agrees on `b2Body::CreateFixture`
`sub_10086B454` at 244 bytes/11 blocks, `ResetMassData` `sub_10086B1F4` at
456/17 and `DestroyFixture` `sub_10086B548` at 260 bytes (Hopper counts 15
blocks and IDA 16). This is the native boundary between fixture-list mutation,
per-shape ComputeMass aggregation and the body mass/sweep update; preserving
that order is important because fixture vectors are the reverse of the
intrusive `m_fixtureList` traversal.

The former 321-line `scene_object_body.rs` is now a five-line facade over a
96-line fixture lifecycle/property leaf, 133-line float32 shape/fixture mass
leaf and 100-line awake plus Reset/SetMassData state leaf. Ten focused mass,
inertia, density, centre-of-mass and Dirt reconstruction tests plus the
head-fixture/contact-order regression pass after the move.

After all three structural batches, strict workspace Clippy, all 273 tests and
the release build pass together. `audit-reverse-structure-menu.png` and
`audit-reverse-structure-start.png` verify the final wgpu menu/start flow with
complete resources and unchanged scene layering.

## Pure-Rust Dirt clipping and recovered-suite layout

The last native build dependency has been removed from `stella-script`.
`sub_100020D70` still uses the recovered float32-to-int32 1,000x grid,
non-zero difference, conditional `x-1..x` slit, preorder PolyTree flattening,
20-unit cleaning and open-contour length rejection. `clipper.rs` owns the
boolean-operation and Clipper-6 cycle-start compatibility; its `clean.rs`
child owns only the translated `OutPt` ring. The old vendored C++ bridge and
`build.rs` are no longer part of the source tree or build graph.

The regression corpus now mirrors the native subsystem hierarchy instead of
one 12,664-line file. A 43-line `tests.rs` facade registers 32 focused modules:
definition/data loading; discrete and continuous Box2D world work; broad and
narrow phase; callback, constraint and solver phases; joint construction plus
prismatic, rope, weld and revolute families; sensors, tracks, body bindings,
Dirt and queries; particles, platform, resources and audio; global/object
render state and submission; trajectory, scene render and object motion;
themes and AnimationWrapper. Direct MCP queries measure the enclosing
`sub_10005E898` member at 7,756 bytes/354 blocks in IDA (including exception
and tail chunks, cyclomatic complexity 173) and 6,476 bytes/237 blocks in
Hopper's principal-body view. The Rust fixed-step orchestrator therefore
remains cohesive while tests and downstream implementation units follow its
recovered callees and registration clusters.

The same boundary is now explicit in production code. `host_physics.rs`
retains `sub_10005E898`'s accumulator, call sequence, auto-clear,
`removeBlocks`, pending-collision velocity application and `objects.world`
writeback. `host_physics/contact_manager.rs` preserves one-at-a-time
`Contact::Update` callback visibility during `b2ContactManager::Collide`;
`islands.rs` preserves DFS-island solve order, contact/joint/track iteration
order and per-island convergence; `toi.rs` preserves the unlocked listener
boundary and rescan loop of `b2World::SolveTOI`. This is a structural move only:
the complete 237-test `stella-script` suite passes with the same callback and
float32 results.

The joint solver now follows the concrete virtual members recovered from the
five shipped Box2D joint vtables rather than keeping initialization and warm
start in one island-sized Rust function. IDA exposes `_ZTV15b2DistanceJoint`
at `0x100AB1760`, `_ZTV16b2PrismaticJoint` at `0x100AB1960`,
`_ZTV15b2RevoluteJoint` at `0x100AB1A60`, `_ZTV11b2RopeJoint` at
`0x100AB1AE0` and `_ZTV11b2WeldJoint` at `0x100AB1B60`. In every table the
last three slots are `InitVelocityConstraints`, `SolveVelocityConstraints`
and `SolvePositionConstraints`. Their entries are respectively
`0x100865270/0x1008655D0/0x1008656D4`,
`0x1008674D4/0x1008678E0/0x100867C68`,
`0x100868B90/0x100868F20/0x1008692B4`,
`0x10086983C/0x100869B30/0x100869C48`, and
`0x100869F54/0x10086A214/0x10086A398`. IDA and Hopper independently agree on
their principal sizes/basic blocks: Distance 864/15, 260/1, 496/9;
Prismatic 1,036/21, 904/14, 1,096/20; Revolute 912/25, 916/23, 812/20; Rope
756/10, 280/3, 468/4; Weld 704/4, 388/1, 668/4.

Accordingly `joint_solver.rs` is now a 136-line island traversal and virtual-
style dispatcher. Its 111-line `joint_solver/impulses.rs` child owns cached
impulse scaling/clearing and the common float32 body writes, while each
concrete joint module owns its own initialization, warm start, velocity solve
and position solve. Prismatic and Revolute are five-/six-line facades over
their separate initialization, velocity and position member files; Rope and
Weld are no longer combined. No joint-solver source is longer than 148 lines.
The recovered contact-before-joint initialization and joint-before-contact
iteration order is unchanged; all 25 focused joint regressions pass after the
move.

Runtime compatibility auditing is also call-based now. `missing_globals()`
continues to report ordinary absent Lua table/data reads because many shipped
scripts intentionally probe optional values. The separate `fallback_calls()`
set records a ResourceManager, AnimationWrapper or compatibility fallback only
when its generated function is actually invoked. `--list-missing` prints both
sets independently, so a long list of harmless optional reads no longer hides
a real unimplemented call. Explicit platform methods whose disassembly is an
empty native adapter are not classified as fallbacks.

## Body motion member split and native body-local coordinates

The former mixed `scene_object_motion.rs` unit has been separated at the
actual Box2D and GameLua ownership boundaries. IDA reports `b2Body::SetType`
at `sub_10086B0CC` as 296 bytes/11 basic blocks, `ResetMassData` at
`sub_10086B1F4` as 456/17, fixture synchronization at `sub_10086B3BC` as
152/4, the SetMassData-style member at `sub_10086B64C` as 240/7, and
`SetTransform` at `sub_10086B794` as 248/8. Hopper independently reports the
same five boundaries.

Hopper's `sub_10086B0CC` decompilation exposes the complete SetType state
transition: reject a locked world or unchanged type, store the new type, call
ResetMassData, zero velocity and synchronize every fixture only for a static
destination, wake the body, clear force/torque, then traverse every body
contact edge and set its filter flag. The rehost previously reproduced the
mass, velocity, wake and force effects but omitted the final contact walk.
`set_native_body_type` now reports whether a transition occurred, and the
world-side owner marks every broad-phase contact attached to that body for
the already recovered deferred ContactManager filtering path. The leading
`b2World::e_locked` test is now modeled as well: the flag surrounds contact,
island and TOI work, so SetType from a Begin/EndContact callback is a silent
no-op while the same call from the pre-Step `updatePhysics` callback remains
valid.

Joint and contact coordinates are now split from fixture coordinates too.
IDA's joint construction sequence at `0x10003867C..0x1000386E8` loads each
body's float32 translation and rotation directly, transforms both stored
anchors with nested `FMUL`/`F(N)MSUB`/`FMADD`, and never reads the game-side
fixture scale. Box2D stores joint anchors and contact manifold witnesses in
the b2Body local frame; `physicsScale` was already consumed when replacement
fixture vertices were built. The old shared transform helper incorrectly
reapplied that scale while creating joints and divided by it while rebuilding
position constraints. The new `native_transform_body_point` and
`native_inverse_transform_body_point` preserve the recovered float32 order
without either operation. Fixture shape projection remains in
`scene_object_collision/shape.rs` and still applies the scale exactly once.

The resulting seven-line facade delegates to 68-line type transition,
16-line bounce, 20-line force accumulation, 108-line sweep and 34-line body
transform modules. Focused regressions cover SetType contact flagging, scaled
fixture joint anchors and scaled-contact witness round trips; workspace
formatting, strict Clippy and all 286 tests pass after the move.

The wgpu replacement host has been split at the corresponding host boundaries
as well. Purple keeps its engine update/draw member, input adapters and capture
render path separate; winit lifecycle is necessarily a cross-platform rehost
concern rather than copied UIKit code. The former 345-line `stella-app/app.rs`
mixed all four responsibilities. It is now a 59-line state/constructor facade
over 75-line fixed-step runtime, 30-line letterboxed input, 110-line scripted
headless screenshot and 97-line winit window/event modules. No arithmetic,
frame order, capture consumption or error transition changed in this move.

The native joint-construction leaf now exposes the same internal source
boundaries. `sub_100037374` is the large GameLua type/coordType switch, while
the final Box2D insertion is the independent 272-byte/15-block
`sub_10086E470`. The former 269-line `construction/physics.rs` combined the
coordinate switch, class-specific parameter defaults, persistent Rust record
write and world/contact side effect. It is now a 22-line facade over a
173-line anchor/Initialize switch, 56-line parameter decoder, 30-line decoded
model and 58-line CreateJoint insertion leaf. The Lua-visible distance length
write remains in the anchor phase where Purple reads b2DistanceJoint+0xA4;
deferred e_filterFlag ownership remains solely in the insertion leaf.

## Theme regressions and remaining native member splits

The theme regression aggregate now follows the same recovered ThemeManager and
ThemeSprite ownership already used by production code. The former 782-line
`tests/themes.rs` is an eight-line module facade over 133-line lifecycle,
88-line background/foreground render, 137-line repeat/cull, 121-line motion,
212-line ThemeSprite-vector and 101-line animation-adapter suites. All eleven
test bodies and their Lua inputs moved intact. The full workspace still has
286 passing tests, strict Clippy is warning-free, and the 11,000-frame
`audit-theme-structure.png` route reaches gameplay with zero invoked fallbacks.

The final mixed contact-manager units have also been separated at concrete
Box2D function boundaries. IDA and Hopper both measure `b2World::Solve` at
`sub_10086E634` as 1,024 bytes/42 basic blocks and `b2Island::Solve` at
`sub_10086CE84` as 1,840 bytes/72 blocks. Accordingly the old 242-line
`contact_manager/islands.rs` now retains only the 184-line world island graph
assembly; its 66-line `islands/sleep.rs` child owns the independently recovered
`0x10086D494..0x10086D584` minimum-sleep-time tail.

Likewise `sub_10086373C` is the independent 608-byte/31-block
`b2Contact::Update` member called by the larger ContactManager traversal. The
former 241-line `refresh.rs` is now a five-line facade over a 60-line
`b2ContactManager::Collide` traversal/cleanup module, a 117-line single-contact
update module and a 74-line listener-event mapping module. Callback mutation
remains observable by the very next live list node, and sleeping contacts still
retain their old touching bit and manifold; the two focused order regressions
and the complete suite pass after the move.

Text-resource loading follows four separate recovered entry points rather than
one Rust source aggregate. IDA measures the raw/encrypted byte pipeline
`sub_1000512D8` at 744 bytes, the Lua table/value adapter `sub_100051810` at
548 bytes, JSON import `sub_100057450` at 168 bytes and Lua chunk execution
`sub_10052AE8C` at 172 bytes. Hopper agrees except that its principal-body view
of `sub_100051810` is 540 bytes. `game_lua/text_files.rs` is now a 13-line
facade over the 60-line raw/AES/7z pipeline, 20-line extracted-resource path
resolver and 147-line Lua/JSON/copy adapter layer. The strict five-slot text
loader and void-ABI JSON import regressions remain unchanged.

## Primitive and object-feature registration order

Hopper string xrefs and the `sub_10002C274` assembly place `drawRect` at
`0x10002D9F8` with member `sub_100043C14`, then `drawPolygon` at
`0x10002DA48` with member `sub_100043F28`. The line pair is a later cluster:
`drawLine2D` at `0x10002E308` calls `sub_10004DC44`, and `drawRectLines` at
`0x10002E338` calls `sub_10004DC8C`; both use the same generated adapter
`sub_1000848B0`. The former 170-line mixed installer also registered polygon
after both line helpers. `primitive_render_registration.rs` is now a 17-line
ordered façade over rectangle, polygon and line leaves, restoring the four
members' relative native order without changing their render-command math.

The RenderObject feature registrations are interleaved in the constructor as
well. `setSensorGravityMask` and `setObjectGravityCategory` occur at
`0x10002CC80/0x10002CCB0`; `setDecorationObjects` and `setPivotOffset` at
`0x10002D4B8/0x10002D4E8`; `removeObject` at `0x10002D908`;
`setRevoluteJointSpeed` at `0x10002DAA8`; the large
`setObjectParameter`/`sub_10004EF74` switch at `0x10002E4E8`; and flash add
and remove at `0x10002E578/0x10002E5A8`. The feature coordinator now exposes
that exact seven-stage relative sequence. Gravity fields and the parameter
switch are separate leaves, while the switch itself deliberately remains
cohesive rather than being cut by line count. All 297 tests, strict Clippy,
release builds and both 11,000-frame
`audit-primitive-native-order-split.png` and
`audit-object-feature-native-order.png` routes pass with zero invoked
fallbacks.

The adjacent particle and audio aggregates have the same pattern. Purple
registers the four particle draw passes at `0x10002DF48..0x10002DFD8`, clear,
tagged clear and the in-game enable gate at `0x10002E038..0x10002E098`, but
does not register `native_addParticlesWithMode` until `0x10002F6D8`.
`particle_registration.rs` now publishes the Lua containers and delegates to
separate draw, clear and spawn leaves; the diagnostic definition dump is a
spawn-local helper rather than part of registration flow.

The GameLua audio entries are contiguous and ordered differently from the old
Rust installer: `setChannelCountLimit` is `0x10002EE48`, followed by
`playAudioReturnUniqueHandle` at `0x10002EE6C`, `setAudioClipVolume` at
`0x10002EE8C`, and `stopAudioWithHandle` at `0x10002EEAC`. The new audio
facade follows channel, play, volume, stop exactly and delegates to channel,
playback and volume leaves. The 13 focused audio/particle regressions, all 297
workspace tests, strict Clippy, release build and 11,000-frame
`audit-audio-particle-native-order.png` route pass with zero invoked
fallbacks.

Scene-object rendering is now split at the two concrete members recovered by
both disassemblers. Hopper measures `sub_10006D5B4` as 888 bytes/22 basic
blocks; it constructs the ordinary object render state, reads the optional
shader, iterates decoration entries and calls `sub_10006C838` for submission.
The flash path `sub_10006794C` is an independent 184-byte/one-block member
calling the animation translation, scale, rotation and draw methods. The old
206-line `scene_render/objects.rs` combined both. It is now a five-line module
façade over `objects/state.rs`, `submission.rs` and `flash.rs`; z sorting,
callback save/restore and immediate command order remain unchanged. Seven
focused scene regressions, all 297 workspace tests, strict Clippy, release and
the 11,000-frame `audit-scene-object-native-boundaries.png` route pass with
zero invoked fallbacks.

The theme frame update has now been separated at the next concrete frame
dispatcher boundaries. IDA and Hopper agree on the exact native order:
ThemeManager background `sub_10009B8B4` at `0x10005ED04`, the same member in
foreground mode at `0x10005ED1C`, then GameLua `sub_1000607E8` at
`0x10005ED28`. The former 195-line Rust aggregate is an 18-line order facade
over `frame_update/theme/layers.rs` and `sprite_data.rs`.

The split also corrects two previously collapsed pieces of record state.
`sub_10006855C` maps `velX+xSpeedAdd` and `velY+ySpeedAdd` to layer
+0x2c/+0x30, `posX`/`posY` to +0x34/+0x38, and `offsetX`/`offsetY` to
+0x3c/+0x40. `sub_10009B8B4` updates the latter pair with `(1-zDistance)`;
`sub_1000607E8` subsequently updates the former pair with the full velocity.
IDA's binary-wide +0x6A8 reference scan further ties the preceding dispatcher
gate to the `sub_100041ABC` physics-lock reference count, so the host skips all
three calls while any named or unnamed lock remains. Two focused regressions
lock these distinctions. All 299 workspace tests, strict Clippy and the
release build pass; the 11,000-frame
`audit-theme-frame-native-boundaries.png` wgpu route reaches gameplay with zero
invoked compatibility fallbacks.

The next structure pass separates `setPhysicsScale` at the concrete member
boundaries recovered independently in IDA 9.3 and Hopper. IDA reports
`sub_100040304` as 188 bytes/1 block with the exact sequence
`world[name] -> scaleX -> scaleY -> getRenderObject -> +0xBC/+0xC0 ->
+0xCC/+0xD0`; it is now the shared 36-line `object_scale_member.rs` used by
both direct `setScale` and `sub_10004050C`. This also closes a prior contract
gap: if `objects.world[name]` is not a table, the member fails before changing
native visual scale, while a valid Lua-only table keeps both reflected writes
when the later render lookup throws.

The strict string/float/float adapter remains in the 25-line
`object_physics_scale_registration.rs` facade. The 117-line `member.rs` owns
the high-level `sub_10004050C` order, and `arguments.rs` owns its live Lua
coefficient/definition reads. IDA identifies polygon helper
`sub_100067CE8` as 712 bytes/42 blocks (Hopper: 704 bytes/31 blocks); the new
`fixture_rebuild/polygon.rs` follows its save-shapes, head-first destroy, and
saved-order recreate phases. Circle replacement and the shared
`b2Body::DestroyFixture`/`CreateFixture` lifecycle live in `circle.rs` and
`lifecycle.rs`. Replacement no longer collapses every fixture into one bulk
broad-phase removal: each head fixture drains its attached contacts
synchronously, releases its proxy, refreshes mass, and each new fixture
installs its proxy and positive-density mass refresh in native order.

The visual-scale and box-layout regressions raise `stella-script` to 254 tests
and the workspace total to 301. All 301 workspace tests pass after the
structure/lifecycle and nine-slice splits, as do strict Clippy and the release
build. The
11,000-frame `audit-physics-scale-structure.png` wgpu route reaches live
gameplay with zero invoked compatibility fallbacks; the release executable is
superseded by the `drawBoxNative` build recorded below.

## `drawBoxNative` native ownership and placement split

The next source-structure pass uses the live IDA MCP server and Hopper's
independent `Purple` document to split the former 216-line
`textured_render_registration/box_draw.rs` by executable ownership. Both tools
measure the hand-written Lua member `sub_100051BBC` at 7,356 bytes with 173
basic blocks. IDA further reports 98 cyclomatic complexity. The resource calls
inside it are separate native members: width query `sub_10045CD14` and height
query `sub_10045CD60` are each 76 bytes/8 blocks in IDA (Hopper includes 84
bytes/7 blocks), ResourceManager draw dispatch `sub_10045C144` is 184/8, and
the atlas branch `sub_100467AF0` is 240 bytes/17 blocks. The Rust source now
has a small install facade over `arguments.rs`, `layout.rs`, `resource.rs` and
`background.rs`, matching those responsibilities without pretending that the
large native member itself contains artificial helper calls.

Direct assembly also corrects the old conventional nine-slice approximation.
Top/bottom middle pieces receive the full requested box width, left/right
pieces receive the full box height, and the four corners are submitted at the
box boundary points with packed horizontal/vertical anchor flags
`0x200000002`, `0x2`, `0x200000000` and `0`. The atlas adapter applies those
flags using the resource's unscaled native width/height before drawing the
independently scaled destination rectangle. Center and the optional color
rectangle cover the full requested box rather than an inset remaining area.
All target coordinates retain per-component float32 `FRINTM` behavior.

The member also performs a complete vertical visibility short circuit using
the scaled `topMiddle` and `bottomMiddle` heights and the 768-pixel drawable;
this skips the color branch as well as every sprite. Resource width/height
queries return zero for unknown names and `sub_10045C144` makes the draw a
no-op, replacing the previous synthetic 1x1 fallback. The optional tenth Lua
slot is consumed only when `lua_istable` succeeds, while component table
values use Lua 5.1 `lua_isstring` semantics and therefore coerce numeric names.
Focused regressions cover the native order, full-box geometry, original-size
corner anchors, missing resources, non-table tenth values, color coercion and
offscreen rejection. The resulting release executable is SHA-256
`1276ac1f3a15ed8a47e27fae5ff937becf25a94eeb4b863dc0cfb1f41467d182`.
The 11,000-frame `audit-box-draw-native-split.png` wgpu route reaches the
three-star L01 completion screen and reports zero invoked compatibility
fallbacks; its capture SHA-256 is
`be2efa6d2428ff810ab8eb64398aab58b831f082cee446a4fcd403f7ed1b3c71`.

## Dynamic drawable scissor and AnimationWrapper sprite pivots (superseded)

The renderer-owned width/height path at `sub_10006E1F4` and resolution-change
callback at `sub_10006E990` also constrain LuaResources state beyond the Lua
globals: the initial full-screen clip/scissor follows the live drawable, not a
hard-coded 1024 x 768 rectangle. The rehost now constructs ResourceRuntime
with the renderer extent and carries a full-screen clip across resize while
preserving an explicitly narrowed script clip. Focused regressions cover both
branches at 2009 x 1080 -> 1429 x 768.

A read-only pull of the connected Android 1.1.5 APK provided an independent
runtime check of Chapter 1's first comic card. Its decrypted page-one
animation/skin resources are byte-identical to the iOS 1.1.6 inputs, excluding
resource drift. The screenshot comparison once suggested that
SpriteComponentCustom used the rectangle center and led to an explicit
half-width/half-height override. The later complete constructor and quad-path
analysis documented below supersedes that inference: the native component
uses its authored SPRT pivot. The old RGB MAE/correlation values remain a
historical visual observation, not proof of the transform contract.

## BasePopup transition and Settings platform-state boundary

The decrypted `menus/BasePopup.lua` and `menus/SettingsPopup.lua` separate the
remaining settings-page differences into engine rendering and platform state.
BasePopup submits `drawFullscreenRect(0,0,0,0.5*bgDarkening,false)` before its
ScalableLayout draw. Its opening tween is linear from zero to one over 0.7
seconds: the popup begins at `y=-screenHeight`, uses the recovered three-part
`(0.975,1.1)` squash/overshoot curve, and finishes at identity scale and y=0.
The close tween is `tweenEaseCubicIn` over 0.4 seconds toward
`y=-1.5*screenHeight`. Hopper confirms the underlying color path at
`sub_100598CC4` as an 820-byte/11-block `GL_Context::drawRect`; its plain versus
plain-alpha selection and transformed four-corner submission agree with the
existing wgpu colored-geometry path.

The connected Android 1.1.5 Settings capture uses authenticated Google Play
Games images. `SettingsPopup:handleGamerServiceButtons` selects those images
only when the script subsystem returns backend `googleplay`, then derives
visibility from `isAvailable()` and enabled state from
`isPlayerAuthenticated()`. Purple 1.1.6's native
`FusionGamerServices::getBackendName` at `sub_1000CA39C` instead returns
`gamecenter`; the offline rehost deliberately reports supported=true and
authenticated=false, producing the target platform's visible but disabled
`BTN_TROPHY_GREY`/`BTN_WINNERS_GREY` state. A shipped-script regression now
checks that service boundary and BasePopup's 0.35-second midpoint/final tween
state at the live 1429 x 768 drawable.

## Closed native tables and ResourceManager audio ownership

The remaining Lua-table audit no longer depends on route coverage. IDA string
xrefs enumerate all 52 methods published by `game::LuaResources` in
`sub_100446570` at `0x1004465D8..0x100446D98`; Hopper independently confirms
the same ARM64 registration order. The legacy `ResourceManager` constructor
`sub_100093904` publishes exactly six `native_*` methods, and
`AnimationWrapper::AnimationWrapper` at `sub_10000EC80` publishes 31 methods
at `0x10000ED90..0x10000F264`. Test-only inventories now assert that every
entry is an explicit raw Lua function. The development-time `__index`
closures formerly manufactured and cached empty functions (or `true`) for
unknown method names; they now retain only missing-name diagnostics and return
`nil`, matching a closed native table instead of hiding implementation gaps.

This pass also recovered behavior behind the legacy table rather than merely
its registration names. `ResourceManager::native_playAudio` at
`sub_100093B00` first calls the complete `LuaResources::playAudio` adapter at
`sub_100448A94`: argument 1 is a strict name, optional argument 2 is a number
with default `1.0`, argument 3 is a strict Boolean with default `false`, and
argument 4 is a number converted through float32 `FCVTZS` with default channel
zero. It then increments the ResourceManager-owned name/count tree at offset
`+0x90`, even if the underlying play returned `-1`, and returns zero Lua
values. The Rust implementation now uses the shared audio runtime for the
real active instance, preserves that independent counter, and reproduces the
zero-result surface. `native_releaseAudio` follows the native
`sub_100094608 -> sub_10045B654` lifetime path: an existing output is asked to
stop the named clip before the resource entry is erased. Focused regressions
cover strict optional types, float32 channel conversion, active playback,
counter state, release/stop behavior, and zero result counts.

The same registration audit covers the platform tables. The GameLua global
constructor has 245 publication sites but only 243 unique global callables
because `createDirectory` and `checkDirectory` are repeated; the later
`particles.native_addParticlesWithMode` publication is table-owned rather
than a global. ForceUpdate now includes the zero-argument
`native_launchAppStore` member retaining Purple's literal product
`875251011`/mode `3`, and its update checker plus all four Analytics entries
enforce their recovered adapters. The update checker uses the type-code-6
`sub_100528760` wrapper for slot 2, just like
`AnimationWrapper.setPlaybackEvent`; this is a Lua callback function, not a
table. `registerKey` consumes exactly three
strings, while `drawLayer` consumes one number and dispatches to the four-byte
`nullsub_13`. The complete registration regression therefore reports no
generic compatibility bindings before any gameplay route is run.

All 314 workspace tests pass after this correction (32 app/wgpu, 19 assets,
one core and 262 script/physics tests), strict workspace Clippy is
warning-free, and the optimized application builds successfully. The
unchanged 16,500-frame menu/chapter/L02 input route reaches live gameplay with
no invoked generic fallbacks, no remaining compatibility globals, and no
missing sprite, font, texture, explicit-quad or non-finite-render diagnostics.
Its visually checked wgpu capture is
`audit-closed-native-tables-l02-16500.png`, SHA-256
`19f4e338764a017004fb762e998b26b4f0edd3126984e640d8acb1d4d1ddd1da`;
the corresponding release executable is SHA-256
`f0d17613b470e1a8213775b1ec1d85c95054f671249e4dc9461e02c10f74c446`.

## Legacy ResourceManager byte counters

IDA and Hopper agree on the two diagnostic globals published by the six-entry
legacy `ResourceManager` table. `native_createSpriteSheet` at
`sub_10009470C` clears the graphics upload accumulator around
`sub_100457E38`, stores the resulting positive byte delta in the map at
ResourceManager `+0x30`, sums the complete map, and publishes that sum through
the float global setter `sub_10007E538` as `g_usedTextureMemory`.
`native_releaseSpriteSheet` at `sub_100094800` preserves the map node with a
zero value, re-sums it, and always republishes the global. The rehost now parses
the referenced SPRT sheet, sums the PVR v2 `data_length` values that represent
new cache uploads, and retains texture reference counts so a shared or
duplicate PVR is not charged twice. Purple's shipped
`CONNECTION_SCREEN_SHEET_0.dat` independently produces its exact 96,350-byte
PVR payload value.

The create-audio adapters `sub_100093C10` and `sub_10009410C` call the returned
AudioClip virtual at object-vptr offset `+0x48`, which resolves to
`sub_100571E44 -> sub_100574D50` and returns the decoder's total byte field at
`+0x3C`; the neighboring `sub_100571E34 -> sub_100574D40` is only the block
alignment field and must not be confused with it. The streaming branch of
`sub_10045A1C8` retains its decoder, while the non-streaming branch drains it
in 4,096-byte chunks and wraps the decoded PCM vector in a memory stream.
Consequently `g_usedAudioMemory` records the WAV `data` chunk, decoded static
MP3/Vorbis PCM, and no value for a streaming MP3 whose native length remains
negative. RIFF chunk walking remains format-specific, while MPEG frame/Xing-
LAME accounting and Ogg final granules provide streaming duration. Static
compressed clips are now actually drained through the in-process pure-Rust
decoder, so their published memory is the resulting PCM allocation rather
than a source-size estimate. Its 18,250-byte result for the shipped
`metal_hit_01.mp3` matches an independent `mpg123 -s` decode exactly.

Synthetic bundle/AppData fixtures cover publication timing, duplicate sheet
loads, release-to-zero, and accumulation across two audio entries.

## Audio device construction, composite clips and physical output

The iOS 1.1.6 binary and its shipped bundle are the sole references for this
implementation; no Android build or capture participates in this pass. IDA and
Hopper agree on the output creation chain
`sub_100447914 -> sub_100459F40 -> sub_10057944C -> 0x100579E34` and the input
chain `sub_10044794C -> sub_100459FF4 -> sub_10057AA08 -> sub_10057ABA4`.
Channels, sample bits and sample rate are therefore retained explicitly. The
output staging buffer follows the recovered 25-millisecond expression, rounds
to frame alignment, and then to the next power of two; stereo 16-bit 44.1-kHz
construction consequently produces 8,192 bytes.

`createCompositeAudio` at `sub_100447CBC` passes only the ResourceManager,
name and collected clip vector in `x0..x2` to
`sub_10045A9CC -> sub_10057919C -> sub_1005790B0`. It walks the Lua sequence
from index one to the first nil and retains only successfully resolved clips.
The apparent fourth SIMD argument in one decompiler view is not present at the
call boundary. The resulting virtual methods at `0x1005792B0` and neighbors
read child clips sequentially, advancing only when a child returns no data,
and sum known child lengths. The Rust runtime now freezes that resolved source
vector at construction rather than inventing random or simultaneous playback.

The desktop host maps the recovered runtime state to `rodio` without leaking a
backend object into the Lua/native-ABI layer. It opens the default physical
device when a window resumes, decodes shipped WAV/MP3/Vorbis files, sequences
composite sources, applies clip/master/track volume and whole-clip looping, and
reports completed one-shot handles back to GameLua. A missing sound device is
non-fatal and deterministic screenshot mode does not open one. Every audio
clip retained after the shipped start-screen initialization is asserted to
resolve to an actual iOS bundle file.

Start and stop semantics were checked in both disassemblers rather than
treated as generic pause/resume. `audio::AudioOutputImpl::startOutput` at
`0x10057972C` creates the worker only while the active byte at `+0x158` is
clear. `stopOutput` at `0x100579870` clears that byte, releases the worker,
calls `alSourceStop`, unbinds and deletes the OpenAL source and six buffers,
deactivates the final audio session, and suspends the context. The host thus
pauses physical players while output is stopped and resumes their retained
decoder cursors on a later start. This reproduces the separate manager state:
the same native stop path calls `sub_1005721DC`, whose complete two-instruction
body is `STRB WZR, [X0,#0xC4]; RET`; it disables new manager playback but does
not erase the active `AudioClipInstance` vector.

That manager gate is also enforced at play time. `sub_100572208` reads
`AudioManager+0xC4` before channel accounting or allocation and immediately
returns unsigned `0xffffffff` when output is stopped. The Rust Lua surface now
returns `-1` in that state, while existing handles remain queryable and resume
after `startAudioOutput`. Each instance freezes its resolved file/composite
source and duration when played, matching the intrusive clip pointer stored in
the native 32-byte instance instead of looking up a later same-name resource.
Same-name replacement also follows pointer identity. Both standalone creation
at `sub_10045A1C8` and composite creation at `sub_10045A9CC` look up the old
map node and call `sub_10057960C -> sub_100572910` before assigning the new
pointer. Direct instances of the replaced top-level name are therefore marked
finished, while an older composite continues through the child pointers it
retained at its own construction. The Rust runtime now makes exactly that
distinction instead of either keeping every old instance or stopping all
playback that merely refers to the same child name.

Standalone construction is also transactional. `sub_10045A698` first builds
the bundle stream, and `sub_10045A1C8` calls format detection at
`sub_1004FB774` plus streaming/static decoder construction before it searches
or mutates the named clip map. `io::FileInputStream::Impl` at `0x100506A58`
throws when `fopen` fails, and exact reads at `sub_100574A48` throw on a short
header. The decoder dispatch at `sub_100574364` is subtler than a generic
"unknown format" failure: type zero is accepted as headerless two-channel,
16-bit, 44.1-kHz PCM by `sub_100578FD0`; only a recognized but unsupported
non-audio type reaches the exception branch. The Rust adapters now validate
the file and decoded stream before publishing either lifetime/memory state or
a new asset. A failed same-name construction therefore preserves the old
top-level clip pointer and its active instances, while a successful
replacement retains the pointer-identity behavior above.

IDA's instruction view and Hopper's independent assembly agree on the exact
type detector. `sub_1004FBA20` reads a four-byte host word and executes `REV`,
so the constants in `sub_1004FB774` are file-order signatures: `BM`, JPEG,
`DDS `, `8BPS`, PNG, `GIF8`, both TIFF forms, PVR v2/v3, MP3 `FF FB`, `ID3`,
`hgrf`, and `RIFF` followed by `WAVE` or `WEBP`. Magic wins over the filename;
otherwise the uppercased extension table selects types 0 through 17. In
particular, `OggS` has no magic case and Ogg/Vorbis is selected only by
`.ogg`, an unknown extension selects raw PCM, while the explicitly known
`.raw` extension maps to unsupported type 15. The rehost preserves these
counterintuitive target rules and carries raw PCM configuration beside the
path so the desktop output does not depend on host decoder guessing.

The WAV constructor at `sub_100578858` also has intentionally permissive EOF
semantics. It rejects a non-RIFF/WAVE 12-byte header, a non-PCM `fmt` chunk,
or `data` encountered before `fmt`, but it returns successfully if clean EOF
occurs immediately after `WAVE` or after `fmt` without `data`. Unknown chunks
advance by their declared size without RIFF even-byte padding. A header-only
WAV therefore becomes a valid zero-data clip rather than a construction
error. The Rust RIFF walker now reproduces that boundary, separates successful
construction from optional host duration, and keeps undefined native cases
such as a truncated `fmt` payload as deterministic failures rather than
emulating uninitialized stack bytes.

Purple's worker `threadFunc` at `0x10057999C` initializes six OpenAL buffers
and repeatedly calls `fillBuffer` at `0x100579C98`; the mixer at
`sub_100573424/sub_1005738C8` calls `sub_100571E54`, which marks the instance's
`+0x1E` finished byte when a non-looping decoder returns zero. The following
`sub_100573250` pass erases precisely those finished instances. Screenshot,
headless and no-default-device hosts now run a device-independent output clock
from parsed WAV sample frames, MP3 gapless frame counts, or Ogg final granules.
It advances independently of scaled game time, freezes while output is
stopped, preserves loops, and retires one-shots so finite native channel limits
cannot fill permanently. Every audio asset retained by the shipped start
sequence has a resolved iOS file and a known decoded duration. The physical
backend uses the same `sub_100571E54` gate for its rare host-decoder failure
path: a non-looping handle finishes, while a looping handle remains logically
alive and its known failure is cached instead of being retried every frame.

Audio input is intentionally not mapped to the host microphone. This is a
property of Purple 1.1.6, not an omitted desktop backend: the registered
`startAudioInput` chain `sub_10044AA88 -> sub_10045D784 -> sub_10057AB98`
only validates that an input object exists and reaches a two-instruction leaf
returning one, while `stopAudioInput` reaches `sub_10045D8C4 -> nullsub_288`.
IDA and Hopper expose no registered sample-read member on `LuaResources`.
Construction and validation state remain reproduced, but opening a physical
microphone would add behavior that the target game's Lua surface cannot use.

Output replacement has a stronger ownership boundary than merely stopping the
old instances. `sub_100459F40` clears and releases the pointer at
`LuaResources+0x38` before constructing its replacement. Each
`AudioOutputImpl` embeds a fresh mixer at `+0x28`; its constructor
`sub_100571ED0` clears through mixer `+0xC4`, writes eight track gains of
`1.0`, writes eight channel limits of `-1`, and thereby resets the next handle
at `+0xC0` to zero. The outer constructor at `0x100579E34` separately writes
the `-1.0` master-gain sentinel at output `+0x130`. On first start,
`initializeBuffers` at `0x100579A44` queries OpenAL `AL_MAX_GAIN`, substitutes
that value only if the sentinel remains exact, and installs the result on the
source. `sub_10045D624 -> sub_100579648` proves that `setMasterVolume` is a
void no-op with no output and otherwise writes this output-owned field.

The Rust lifecycle therefore reconstructs instances, the wrapped handle
counter, track gains, channel limits and master gain on every output creation,
including a constructor failure after the old output has already gone. It
continues to retain the independent LuaResources clip/composite maps. A host
generation accompanies each output allocation so physical players and the
device-independent clock cannot confuse a newly reused handle zero with the
previous output's handle zero. Hopper independently shows the same `0x65`-byte
mixer clear, `0xbf800000` master sentinel, first-start max-gain substitution,
and destructor call to `stopOutput` before the embedded mixer destructor.

The full workspace now passes 337 tests: 34 app/audio/wgpu, 19 assets, one
core, and 283 script/physics tests.

The audio registration source now follows the same recovered ownership rather
than remaining one 316-line aggregate. Its facade preserves the interleaved
`LuaResources` publication points, while `devices.rs` owns
`sub_100459F40/sub_100459FF4` device replacement, `configuration.rs` owns the
format guards and output-buffer calculation, `clips.rs` owns standalone and
composite construction, and `controls.rs` owns the four start/stop adapters.
No adapter, closure lifetime, or registration order moved across this split.

The exact L01-to-L02 input stream above was rerun twice after physical audio
integration. Both runs reached frame 16,500 with zero invoked fallbacks and
zero compatibility bindings. Full-image hashes differ because the live
Box2D/animation route is not an image-sequence lockstep test: an uncompressed
pixel difference bounds the only changed region to `(276,588)-(339,646)`, the
moving hammock/cage object. Every other pixel—including all static atlases,
UVs, pivots, trees, background and tower geometry—was byte-identical. Dynamic
level captures are therefore checked by route completion, diagnostics, visual
inspection and localized pixel bounds rather than incorrectly treating one
whole-frame hash as authoritative.

The same 16,500-frame route was rerun after installing the independent silent
output clock. It again reached L02 with 88 optional data reads, zero invoked
fallbacks and zero compatibility bindings; the resulting artifact is
`build/audit-silent-audio-l02-16500.png`. This pass exercises real one-shot
retirement even though deterministic screenshot mode intentionally opens no
sound device.

After matching same-name `AudioClip*` replacement, the route was repeated as
`build/audit-audio-pointer-l02-16500.png` with the same 88/0/0 diagnostic
result. This verifies that stopping direct instances during resource
replacement does not regress the complete L01-to-L02 gameplay path.

After matching failed-construction transaction order, the route was repeated
as `build/audit-audio-transaction-l02-16500.png`, SHA-256
`d63ca17d802a815c2f0122af2b92975a572febe495af6e9b518b27537b56e614`,
again with 88 optional data reads, zero invoked fallbacks, and zero remaining
compatibility bindings. The corresponding release executable is SHA-256
`60853ee6d41e9197f02a6db8c14c2a0c7383302e5a7f1b32fa45957e73a37a7c`.

After reconstructing the full output/mixer lifecycle and adding host output
generations, the exact route was repeated from a fresh isolated AppData root as
`build/audit-audio-output-manager-l02-16500.png`. It reached the same live L02
post-ability state with 88 optional data reads, zero invoked fallbacks, zero
remaining compatibility bindings and an empty stderr log. The capture SHA-256
is `83f67918901ff1f60d8f140289e79b11f7551b889acf665f3f445eed278f6af7`;
the corresponding optimized executable is SHA-256
`0721efc85666fd7e9dec9f487fc5d6100da24d9c0a4081a713cd79a6a8ecd4b0`.

Two additional runs used independent empty AppData directories with the same
read-only bundle symlink. Their generated `highscores.lua` and `bi_data.lua`
were byte-identical. Decrypting the native AES-CBC `settings.lua` containers
showed that the only plaintext difference was a `birdsShot` key rendered by
Lua as `device_function: 0x...`, i.e. the process address of a function object;
all level/event seeds were identical. The screenshot difference was again
confined to `(276,586)-(330,633)`. Render-command inspection identifies the
changed sprites as `BLOCK_HOMETREE_STRING` and `BUCKET`; their resource name,
texture, `0.561` scale, pivot and alpha are identical, while only position and
angle differ slightly. At the capture boundary `BURCKET_1` is intentionally
still awake, with residual linear velocity around `1.4e-6` and angular
velocity around `2.8e-8`, and several linked string bodies are also still
being solved. This is a live joint-chain phase difference coupled to the
already documented process-address boundary, not an atlas, UV, anchor or
wgpu placement error; forcing the chain to a screenshot-specific pose would
depart from Purple's physics behavior.

After reconstructing native audio type detection, raw PCM and permissive WAV
EOF handling, all 341 workspace tests pass (35 app/wgpu, 19 assets, one core
and 286 script/physics), strict workspace Clippy remains warning-free, and the
optimized application builds successfully. The exact 16,500-frame route was
repeated from another empty AppData root as
`build/audit-audio-reader-boundaries-l02-16500.png`. It again reached live L02
with 88 optional missing globals, zero invoked fallbacks and zero compatibility
bindings. Visual inspection found no missing or displaced atlas content. The
capture SHA-256 is
`a68927214f865bb20ff6331da4204edac53678de31774b1a73a1507cf53bf658`;
the corresponding release executable is SHA-256
`7c34efcb052d2d0e2cfe392f9bb6d8ab21044a554e1d0184d227cf8baa006e80`.

The clip's source ownership was then aligned beyond mere path identity.
`sub_100571B28` retains the already opened file stream and decoder for the
streaming branch, while the non-streaming branch in `sub_10045A1C8` drains the
decoder before constructing `sub_100571C68`: that constructor owns a
`MemoryInputStream` over the resulting byte vector and reconstructs a raw
reader from the decoder's channel/bit-depth/sample-rate triple. Rust audio
assets now retain their creation-time bytes and streaming tag, so direct and
composite clips do not reopen a path at play time. A regression overwrites and
then removes the source file after clip/composite construction and verifies
that all three frozen instances remain valid.

Static WAV storage now follows the native representation exactly: only its
declared `data` payload is retained as PCM with the parsed configuration, not
the RIFF container. `sub_10045A1C8` sizes the vector from decoder `+0x3C`,
zero-fills it, and performs one read without shrinking to the returned count;
the Rust walker consequently zero-fills a declared data tail when the host
file ends early. Streaming WAV retains the encoded stream instead. Headerless
raw PCM likewise becomes an owned memory clip when non-streaming and a retained
raw stream when streaming.

After these ownership and static-memory corrections, all 343 workspace tests
pass (36 app/wgpu, 19 assets, one core and 287 script/physics), strict Clippy
remains warning-free, and the exact fresh-AppData L01-to-L02 route again
reports 88 optional missing globals, zero invoked fallbacks and zero
compatibility bindings. The visually checked capture is
`build/audit-audio-retained-stream-l02-16500.png`, SHA-256
`39f3123440180cdf3a2dc2c3ac92868633f1538822cccdb375af01a3dc1a99eb`;
the corresponding release executable is SHA-256
`50b22b5866b762f232120b99b771bb011f94f27355218ba873f423e655f6c3c4`.

The remaining non-streaming compressed-audio boundary was then closed.
IDA's `sub_10045A1C8` shows that the static branch repeatedly requests 4,096
decoded bytes until a short read, then constructs `sub_100571DD4` from that
PCM vector and the decoder's channel/bit-depth/rate triple. In contrast, the
streaming branch constructs `sub_100571C64` around the already initialized
decoder. The Rust resource layer now performs the same eager decode for MP3
and Vorbis and publishes `PcmData`; only streaming clips retain compressed
bytes. Decoder construction is still performed immediately for streaming
clips, preserving the native pre-publication failure boundary. `rodio`'s
decode-only features are shared by the script crate without `cpal`; the app
alone enables physical playback.

The shipped `metal_hit_01.mp3` yields 9,125 mono 16-bit samples in both the
pure-Rust decoder and the historical generic mpg123 calibration. This first
checkpoint established matching frame count, channel count, sample rate and
byte length while leaving a handful of one-LSB synthesis differences for the
later generic-mpg123 compatibility pass documented below. All 538 shipped MP3
files were regression-decoded into static native-style memory clips, and every
clip loaded by the original start-screen sequence is asserted not to retain a
non-streaming compressed source. A separate short-WAV regression proves the
native one-read behavior: bytes missing from a declared `data` payload remain
zero-filled in the owned vector.

All 344 workspace tests now pass (36 app/wgpu, 19 assets, one core and 288
script/physics), and strict all-target Clippy is warning-free. The complete
fresh-AppData route was repeated as
`build/audit-audio-static-decode-l02-16500.png`; it reaches the same live L02
post-ability state with 88 optional missing globals, zero invoked fallbacks
and zero compatibility bindings. Visual inspection again shows complete,
aligned atlases. Its SHA-256 is
`314f722a014820e3c1e0cef651957bb9dd20ad42e725b79d2c1367c570d9c7b6`;
the final optimized executable for this pass is SHA-256
`35bd8d1f5d912cb90d7a5e317f9d4a536f9cca38d1e4a651570bd055289c074e`.

The physical host no longer delegates every active clip to a separate
floating-point backend player. IDA's `sub_100573424` and
`sub_1005738C8`, independently checked against Hopper, establish one integer
mixer owned by `AudioOutputImpl`: it clears a signed 32-bit accumulator for
each output block, removes instances already carrying the `+0x1E` finished
flag, reads each surviving decoder, and applies `FCVTZS`-quantized
clip-times-track gain. Sixteen-bit gain uses a scale of 4,096; equal-channel
and mono-to-stereo samples shift by 12, while stereo-to-mono sums two values
shifted by 13. The final accumulator is saturated to signed 16-bit only after
all instances contribute. Eight-bit output uses scale 256 and retains the
target's surprising unsigned centering omission in both channel-conversion
branches. Even a gain below one advances the reader before skipping its mix.

The Rust backend now follows that topology in the separate
`audio/native_mixer.rs` module. One infinite native-format stream is connected
to the desktop device; instance and track gains are evaluated inside the
integer block mixer, while master gain remains on that single stream after
saturation, matching the OpenAL source property at output `+0x130`. It retains
the 8,192-byte block size for the shipped stereo/16-bit/44.1-kHz output,
duplicates or averages only the recovered mono/stereo combinations, preserves
32-bit wrapping accumulation before saturation, advances muted instances, and
reports completion on the following block-removal pass. ARM `FCVTZS` NaN and
overflow keep the integer-indefinite `INT_MIN` result rather than Rust's
ordinary saturating float cast.

The mixer's format check compares source bit depth to output bit depth and
does not inspect source sample rate. `fillBuffer` then passes the configured
output rate to `alBufferData`, so a 16-kHz decoded clip is consumed frame for
frame in a 44.1-kHz output block rather than being resampled independently.
The retained source format is now explicit even for compressed streaming
clips, and the device-independent output clock uses decoded frame count divided
by the output rate. This keeps screenshot/no-device channel retirement aligned
with the physical mixer instead of using the file's nominal duration.

All 348 workspace tests now pass (39 app/audio/wgpu, 19 assets, one core and
289 script/physics), and strict all-target Clippy remains warning-free. The
optimized application is SHA-256
`416d0f027ef2e42b5ed7c1c5b0df6bfaf1c9648e3362d0288eddf08263a4d16a`.
The complete route from a fresh AppData root was repeated as
`build/audit-native-mixer-l02-16500.png`; it reached the live L02 post-ability
state with 88 optional data reads, zero invoked fallbacks and zero remaining
compatibility bindings. Visual inspection shows complete, aligned scene and
atlas content. The capture SHA-256 is
`370c30c50732b73f75570a9ee673b4bc4d7912bd4ea337b3132a3812916d1c47`.
This implementation and validation use only the iOS 1.1.6 `Purple.app`, its
bundle, IDA, Hopper and desktop tests; no Android binary, capture or handset is
part of the reference chain.

The host reader beneath that mixer was then changed from eager per-playback
materialization to an incremental byte reader. This follows the observable
contract of `AudioClip::read` at `sub_100571DD8`, format dispatch at
`sub_100574808`, and composite reading at `sub_1005792B0`: static PCM owns a
seekable memory cursor, retained MP3/Vorbis owns an incrementally advanced
decoder, and a composite retains distinct child readers plus its child index.
`sub_100571E54` deliberately retries a short read only for looping instances.
A non-looping composite which reaches a child boundary therefore returns a
short block with a silent tail and begins the next child on a later block;
looping playback continues reading and can wrap within the same requested
block. Empty looping sources retain their native never-finished state without
allowing the target's unbounded retry loop to wedge the host callback.

`initializeBuffers` at `0x100579A44` also performs six mixer calls before it
queues the buffers and starts the single OpenAL source. Physical output now
reconstructs that prefill on every start: six native blocks advance reader
cursors and completion/removal edges before the first audible sample. A stop
destroys that one physical stream and discards queued data; a later start
creates a fresh stream and prefills again from the still-retained logical
instances, rather than pausing and resuming a collection of host players.

IDA additionally proves that a target `AudioClip` shares its `AudioReader`
while each `AudioClipInstance` carries a 32-bit byte cursor; MP3 and Vorbis
readers seek/cache against that requested cursor. The cross-platform decoder
keeps an equivalent incremental cursor per physical playback because the
pure-Rust decoder does not expose mpg123/vorbisfile's identical shared random-
access state. This preserves emitted PCM, cursor movement, looping and
completion behavior while avoiding repeated full-stream redecodes when two
instances interleave.

All 351 workspace tests pass after these boundaries (42 app/audio/wgpu, 19
assets, one core and 289 script/physics), with strict all-target Clippy clean.
The new release executable is SHA-256
`53f1c409dafad85ed60d1bb9f114cdf168709ba786b06c4b97b2c7c5dd398646`.
The fresh-AppData route again reaches live L02 as
`build/audit-native-stream-reader-l02-16500.png`, with 88 optional data reads,
zero invoked fallbacks, zero compatibility bindings and visually complete
atlases; its SHA-256 is
`98462f2cbfbea789e3b9a7f7d1b78b6912166a75ab9f716a739286e7214956df`.

The device-independent host clock now follows the same buffer boundary as the
physical mixer instead of subtracting wall-clock duration from each clip.
`AudioOutputImpl` stores output channels, bit depth and sample rate at
`+0x18/+0x1C/+0x20`, while its worker owns the configured byte count at
`+0x13C`.  Each elapsed output block therefore consumes exactly
`bufferBytes / (channels * bytesPerSample)` decoded source frames, regardless
of the source's encoded sample rate.  Start performs the six synchronous
`initializeBuffers` fills; stop freezes all retained cursors and discards the
fractional output-block accumulator; restart performs another six fills.
Unsupported explicit channel/bit-depth combinations remain registered but do
not advance, matching the early continue in `sub_100573424`/`sub_1005738C8`.
For non-looping clips, one block can set the instance finished flag after a
zero read and only the following block removes it and publishes completion.
Output generation is part of the clock identity, so a reconstructed native
output can safely reuse the same numeric playback handle.

All 351 workspace tests continue to pass (42 app/audio/wgpu, 19 assets, one
core and 289 script/physics), strict all-target Clippy is warning-free and the
optimized application is SHA-256
`b7c28fe727efa7f87baf0c34d391b775ac8d2caff47266f2e21b2ff9b047fcb2`.
The complete fresh-AppData route was repeated as
`build/audit-native-audio-clock-l02-16500.png`; it reaches the live L02
post-ability state with 88 optional data reads, zero invoked fallbacks and
zero compatibility bindings.  Visual inspection shows complete, aligned
scene content.  The capture SHA-256 is
`4bb99622b910f2f96feef6d6cb2c3495b4c6ac7fef9cdc6ee5a1b17ae08abaf0`.
This pass, like the preceding mixer work, used only the iOS 1.1.6 executable,
IDA/Hopper, its bundled resources and deterministic desktop regressions.

The worker cadence was subsequently recovered instead of treating buffer
consumption as a continuous duration countdown.  IDA and Hopper independently
show `threadFunc` at `0x10057999C` calling `fillBuffer` once immediately after
initialization and then sleeping 10 ms between calls.  The full assembly of
`fillBuffer` at `0x100579C98` queries `AL_BUFFERS_PROCESSED` (`0x1016`) and
deliberately does nothing while the result is below two.  At two or more it
unqueues every reported buffer, holds the output mutex while invoking the
integer mixer once per buffer, requeues the whole batch, and restarts a source
whose state is `AL_STOPPED`.  This keeps between four and six blocks queued
ahead rather than refilling every completed block independently.

Both physical output and the device-independent clock now reproduce that
six-buffer queue, 10 ms polling phase, two-buffer threshold and batched
refill.  The physical source captures volume/track/new-instance state when
each future block is actually mixed, preserving the native queue-latency
envelope during fades instead of concatenating the initial six blocks and
then running only one block ahead.  The silent clock advances ideal OpenAL
playback continuously but publishes EOF/removal only on a worker poll; a
16,000-frame clip in the shipped 44.1-kHz output therefore crosses its second
refill/removal pass at 190 ms, not at an arbitrary host update boundary.
Short clips removed during synchronous prefill are collected before the next
VM tick on the physical path as well as the headless path.

All 352 workspace tests pass after this correction (43 app/audio/wgpu, 19
assets, one core and 289 script/physics), strict all-target Clippy is clean,
and the optimized executable is SHA-256
`8f922ffd4c8d8d21c2fbef613f492ded68d3be60ad9e94c92e427bc80badb474`.
The fresh-AppData 16,500-frame route again reaches the live L02 post-ability
state as `build/audit-native-audio-poll-l02-16500.png`, with 88 optional data
reads, zero invoked fallbacks, zero compatibility bindings and visually
complete/aligned content.  Its SHA-256 is
`df901b35562390119ca773b0877184c1df835d81481a7cb155ef76d0e5622428`.

The embedded MP3 synthesis and gapless boundary were then calibrated rather
than approximated through the host decoder. `sub_100570E04` constructs the
mpg123 handle with a null decoder name, opens a feed, and supplies source bytes
in 2,048-byte chunks without setting optional decoder parameters. IDA and
Hopper independently show `sub_10056CBF0` installing only
`sub_10057046C`/`sub_100570914`, reporting `Decoder: generic`, and rejecting
decoder indices two or greater because this build contains one implementation.
The embedded `optimize.c` diagnostic line 515 fingerprints the same historical
mpg123 source family as generic 1.19.0, 1.20.0 and 1.21.0 builds; all three
produce identical PCM for the calibration resources.

`sub_10057046C` accumulates the generic float synthesis window, clips above
32,767 and below -32,768, and executes ARM64 `FCVTZS` for every ordinary
sample. The previous nearest-even conversion was therefore incorrect. Pure
truncation removes almost all differences, but a full 538-resource comparison
also exposes target gapless delay removal: 334 tagged MP3s discard exactly 529
interleaved samples at the start. The remaining float-synthesis delta is
19,175 samples out of 24,104,441 (0.079550%), spread over the fixed shipped
assets and normally one LSB.

The cross-platform runtime remains pure Rust. A generated 538-entry profile,
keyed by encoded-byte FNV-1a plus length, records the target output length,
the optional 529-sample skip, and only those sparse target sample values. Both
the eager `PcmData` path and retained `StreamingDecoder` apply the same cursor;
unknown external MP3 data falls back to the recovered clip/saturate/`FCVTZS`
contract without asset-specific correction. `stella-mp3-audit` reproduces the
profile from a historical generic decoder, while the runtime ships only the
172,410-byte read-only encoded table and has no C library, subprocess or
Android dependency. Full-stream regressions cover both an untagged resource
and a tagged 529-sample resource; their output lengths and complete FNV hashes
match the historical target in both static and streaming paths.

The same profile length is also authoritative for retained streaming clips
and the device-independent output clock. The older MPEG frame scanner counted
12,096 frames for a representative tagged resource even though Purple emits
10,991; using the decoded profile removes that 1,105-frame completion delay
without opening or draining the stream early.

All 354 workspace tests pass (43 app/audio/wgpu, 19 assets, one core and 291
script/physics), and strict all-target Clippy is warning-free. The optimized
application is SHA-256
`bebabeccb141e749042c50b1026843a767d4f5a3b69ef4175938ea6c93c810ff`.
The complete fresh-AppData route reaches the live L02 post-ability state as
`build/audit-mpg123-gapless-l02-16500.png`, with 88 optional data reads, zero
invoked fallbacks, zero compatibility bindings and visually complete/aligned
content. Its SHA-256 is
`8d1ee396599b5e0289617d0b1b56f9863af7565e53667dbb66a07fa7378b798b`.
This calibration and validation use only Purple 1.1.6, its iOS bundle,
IDA/Hopper and deterministic desktop tooling; no Android executable, capture
or handset is involved.

## GameLua constructor tables and native pointer/zoom input

The remaining constructor-owned Lua tables were recovered directly from
`sub_10002C274` and checked independently in Hopper.  After publishing the
screen size and three key tables, Purple constructs `multitouchSweep`,
`multitouchZoom` with `zoomCoolingTime = -1.0f`, and a `clippedText` table whose
identity remains fixed for the complete GameLua lifetime.  `sub_10004F630`
replaces only that table's `lines` value and writes `widestLine`; it does not
replace the outer table.  The same constructor loads `highscores.lua`,
`settings.lua` and `bi_data.lua` through `sub_10005B7D4` into three distinct
tables, falling back to fresh empty tables when the corresponding AppData
file is absent.  The Rust bootstrap and `clipText` member now preserve these
ownership and identity boundaries instead of constructing them lazily or
replacing the result object.

The platform touch bridge is likewise native rather than a mouse-only
approximation.  `sub_10005E898` reads the insertion-ordered
`framework::TouchEvent` vector at GameApp `+0x4C8`, caps publication at two
16-byte entries, emits each `x`/`y` pair into a fresh Lua `touches` table, and
formats the table key with `%d` from the signed low 32 bits of the 64-bit touch
identifier.  `touchcount` is the capped count.  IDA and Hopper agree that
`MyEAGLViewController` truncates drawable-pixel coordinates in
`touchesBegan:` (`0x100408DAC`), updates the matching entry in
`touchesMoved:` (`0x100409020`), and removes it in the ended/cancelled members
at `0x100409290`/`0x100409508`.  Only the first touch drives LBUTTON/cursor
state, and ending it does not promote another active touch.  Winit touch
events now enter the same ordered vector and reproduce that primary-touch
lifecycle.

`sub_1000293C8` owns both the per-frame zoom easing and the exact-two-touch
pinch state.  A new pinch snapshots GameApp `+0x4F8`, computes distance with
the recovered float32 `FMUL`/`FMADD`/`FSQRT` sequence, and writes
`baselineScale * currentDistance / initialDistance` to `+0x4FC`.  The initial
distance must be strictly between `FLT_MIN` and `FLT_MAX`.  `sub_10005E898`
then calls `applyUserZoom((current-previous)*0.5f)` only when `+0x4FC` differs
from `+0x51C`, and reloads current after the callback before taking the next
snapshot.  The rehost now shares one input-zoom record between pinch, wheel,
`setWorldScale` and `resetMouseWheelScale`, removing the former independent
touch accumulator.

The adjacent wheel path at `sub_100029FF8` is now implemented as well.
GameApp's constructor `sub_100026D2C` initializes base/current/previous scale
to `1.0f`, both easing clocks to `-1.0f`, and enables smooth zooming by
default.  Smooth input selects a 0.1 or 0.2 step divided by the configured
`gameWorldScale`, applies SHIFT's 0.05 fine multiplier, and creates a 0.5
second target.  Input during an active easing adds half a step to the target
and changes the duration to `1-elapsed`.  The frame member advances at most
0.1 seconds and evaluates the exact cubic ease-out as two `FMADD`s.  Direct
mode writes current scale immediately unless CONTROL is held.  Both modes
publish `cursor.wheel`/`wheelTriggered`, with the latter cleared only after the
following native update.  The desktop host forwards Winit line/pixel wheel
events and current modifier state to this recovered integer callback.

Four focused regressions cover fresh touch-table publication, signed IDs and
the two-entry cap; pinch baseline/reset and half-delta callbacks; cubic
smooth easing and in-flight retargeting; and direct SHIFT/CONTROL behavior.
All 360 workspace tests pass (43 app/audio/wgpu, 19 assets, one core and 297
script/physics), formatting and strict all-target Clippy are clean, and the
optimized executable is SHA-256
`b033def26e72a6d75b785f5730d07df8398b11d1fc9225fdc7444aa9a6694279`.
The exact fresh-AppData 16,500-frame menu/L01/result/L02/post-ability route
again reaches live L02 with 88 optional data reads, zero invoked fallbacks and
zero compatibility bindings.  Its visually checked wgpu capture is
`build/audit-native-input-l02-16500.png`, SHA-256
`f26557248c62f3c73a22baeef57082a9157066744bfb0ab03094aa5b9002b626`.
This entire recovery and validation pass used only Purple 1.1.6, its bundle,
IDA, Hopper and the Rust desktop harness; no Android device or Android build
was used.

### Native five-key bridge

The fixed key publication loop in `sub_10005E898` iterates the five numeric
codes at `unk_1009AE708`: 57, 86, 87, 82 and 83.  Reading their entries from
`off_100AA3580` identifies them as `LBUTTON`, `KEY_BACK`, `KEY_MENU`,
`VOLUME_UP` and `VOLUME_DOWN`.  This is deliberately not a generic host
keyboard scan.  `sub_1004016F4` reads the persistent hold byte at GameApp
`+1088+key`, while the GameApp virtual members at `+32` (`sub_10002A208`) and
`+40` (`sub_10002A218`) write the one-frame press and release arrays at
`+1424+key` and `+1555+key`.  The native update publishes all five entries to
`keyPressed`, `keyReleased` and `keyHold` every frame, including explicit
false values, then clears only the two edge arrays after the Lua callback.

The Rust host now preserves those three state classes and ignores repeated
host key-down events for edge generation.  Winit Escape, Context Menu, audio
volume up and audio volume down feed the four matching native names; pointer
and touch primary-button handling continues to own `LBUTTON`.  SHIFT and
CONTROL remain internal wheel modifiers (native codes 34 and 35) and are not
incorrectly added to Purple's five-key Lua publication loop.  A focused
regression checks initial false publication, press/hold, key repeat, release
and post-frame edge clearing for every fixed entry.

All 361 workspace tests pass (43 app/audio/wgpu, 19 assets, one core and 298
script/physics), formatting and strict all-target Clippy are clean, and the
optimized application is SHA-256
`c020030c2cce40c60a7fcfccb2ce498f0dfc943ff09788ef6f86bef76a15aea5`.
The new fresh-AppData 16,500-frame route again reaches the complete live L02
post-ability state as `build/audit-native-keys-l02-16500.png`, with 88
optional data reads, zero invoked fallbacks and zero compatibility bindings.
Its visually inspected capture is SHA-256
`87e39705eca9a6eb33f022cbdfbde63450322e006cf3528668bf664129a4b566`.
This batch likewise uses no Android program, screenshot or device.

### Application activation and pointer cancellation

IDA and Hopper agree that `-[MyEAGLViewController viewDidDisappear:]` first
calls the GameApp virtual at `+0x78`; its target `sub_100401964` assigns the
touch-vector end pointer from its begin pointer and therefore clears every
active touch without reallocating the vector.  It then clears
`m_singleTouch`, and only if LBUTTON code 57 is still held does it call the
release virtual at `+0x28` followed by `sub_1004016B0(..., 57, false)`.  No
other key is released by this member.  The desktop lifecycle now uses the
same ordering and conditional release, while Winit focus loss supplies the
touch-cancellation event that iOS normally delivers separately.

The surrounding display-link lifetime was recovered as a separate boundary.
`-[AppController applicationWillResignActive:]` clears `m_allowUpdate` and
calls `stopUpdate`; that member cancels pending updates, resets the view's
single-touch owner, invalidates the CADisplayLink and clears its pointer.
`applicationDidBecomeActive:` sets `m_allowUpdate`, snapshots the current
monotonic microsecond clock through `sub_1005863E8`, then calls `startUpdate`.
Both start/stop members also call App's virtual slot `+0x98`, which resolves
to `sub_100029BE8`.  That member calls `sub_100401678` to zero all 0x83
platform hold bytes and clear the native touch vector, then forwards the
active Boolean to GameLua `sub_10005D4D4`.  Once GameLua is initialized, the
latter invokes the zero-argument Lua global `gameResumed` or `gamePaused`.
The shipped pause callback stops the retained music name, records play time,
saves settings/highscores/BI data and emits its event; resume relayouts the
frames, restores the appropriate music and reports elapsed pause time.

The Rust/wgpu window host now follows the complete chain.  It performs no
fixed updates while unfocused or suspended, resets its monotonic timestamp
and pending fixed-step debt before resuming, clears all native key holds and
touches on each activation transition, and invokes the shipped lifecycle Lua
callback before restarting updates.  Time spent in the background therefore
cannot appear as catch-up physics or animation frames, and persistence/audio
side effects no longer disappear at the platform boundary.

The following App virtual at `+0xB0` resolves to `sub_100029C24`, rather than
being part of GameLua activation.  IDA and Hopper show its nested lookup of
the Boolean `settings.root.audioEnabled`, defaulting to enabled when any
table/value/type test fails.  On activation it starts an existing output only
when that flag is true and independently starts an existing input; on
deactivation it stops both unconditionally.  The output wrappers resolve to
`sub_1005795DC`/`sub_1005795F8` and ultimately
`AudioOutputImpl::startOutput`/`stopOutput`.  The host now updates the retained
native device state at this exact point and synchronizes either the physical
Rust mixer or the device-independent clock immediately, so audio cannot keep
advancing while the display link is stopped.  This ownership boundary is
isolated in `game_lua/host_lifecycle.rs`, separate from pointer injection.

The focused pointer regression verifies that disappearance publishes an empty
touch table, creates exactly one LBUTTON release edge, preserves an unrelated
held KEY_BACK, clears the release after the following update and does not
invent a second release when no primary pointer exists.  A second regression
locks activation-time hold/touch clearing before both callbacks while proving
that `sub_100401678` does not erase already pending press/release edges.  A
third covers disabled/default-enabled output, independent input state and the
native first-start master-gain sentinel.  All 364 workspace tests pass (43
app/audio/wgpu, 19 assets, one core and 301 script/physics), formatting and
strict all-target Clippy are clean.  A live desktop-window startup also
remains stable through the initial `gameResumed`, audio-device synchronization
and event-loop path.  The optimized application is SHA-256
`e63c8856f7e99cd1e0dd6bfcaadba5b2ac78c1f0cf3e94e6a31a5c0380411bd2`.
The fresh 16,500-frame full route reaches the visually complete live L02 state
as `build/audit-activation-audio-l02-16500.png`, with 88 optional data reads, zero
invoked fallbacks and zero compatibility bindings; its SHA-256 is
`d6577891c3fe86c02929dbbd14d1435fd31af1d16220ab202dab15860b61d394`.
No Android executable, capture or device contributed to this pass.

### GameApp vtable closure and live drawable theme extent

The remaining primary `GameApp` vtable at `0x100A903C0` was audited as one
closed 27-slot inventory in both IDA and Hopper.  Its touch members are the
four adjacent entries at slots 12 through 15: `sub_100401700` appends one
complete 16-byte `TouchEvent`, `sub_100401744` replaces the first event with a
matching 64-bit id, `sub_10040182C` compacts away every matching id, and
`sub_100401964` clears the vector by assigning end from begin.  Winit's touch
adapter now preserves even the otherwise-malformed duplicate-begin case, so
the observable container semantics match the native members rather than a
host map keyed by id.

Slot 23 (`sub_100028FB0`) forwards to `sub_100062518`, which returns the byte
at GameLua `+0x6AC`.  The only store is at `0x10005EB14` inside
`sub_10005E898`: it reads Lua `g_safeToQuit` through the ordinary
`lua_toboolean` path at `0x10005EAD4..0x10005EB04`, then latches the result
before the later Lua update callback.  The rehost now retains this separate
frame-latched byte and exposes it to the platform host; numeric zero remains
true under Lua semantics, and a callback-side change becomes visible on the
next frame.  Slot 25 (`sub_10002A228`) publishes its Boolean as
`g_mouseAvailable`; the iOS target initializes it false even though the
desktop adapter can translate a physical pointer into the target's touch and
LBUTTON interfaces.  Slot 26 and its secondary-base thunk both reach
`sub_1004016E4(..., 0)`, confirming that the existing `requestExit` state is a
zero-code platform exit request.  Its Lua wrapper `sub_100030750` saves the
three persistent tables only for `deviceModel == "wp8"`; the reproduced iOS
profile correctly skips that Windows Phone branch.

The resolution member at slot 7 resolves through `sub_10002A25C` to
`sub_10006E1F4`.  Before replacing `screenWidth`/`screenHeight` and invoking
Lua `resolutionChanged`, it calls `sub_10009961C` on the retained
ThemeManager.  That member indexes
`gameCamera.resolutionCorrectedCameras[FCVTZS(float(endCameraIndex))]`, reads
the selected camera's float32 `sx`, and stores it at ThemeManager `+0xA8`.
Rust now performs the same pre-callback snapshot and initialization-time
capture; it does not incorrectly read the newly recalculated camera after the
callback.

This camera audit exposed a directly visible wide-drawable error.  The
ThemeManager frame member `sub_10009B8B4` calls renderer vtable slots
`+0xD8/+0xE0` at `0x10009B94C` and `0x10009B98C`, and its draw member
`sub_10009BDB4` repeats those width/height calls at
`0x10009BF74..0x10009BF98`.  GameLua's independent ThemeSpriteData update
`sub_1000607E8` likewise reads the live width at `0x100060B28` and live height
at `0x100060BBC`; its foreground wrap thresholds are `4*width/physicsScale`,
`2.5*width/physicsScale` and `height/physicsScale`.  These paths were still
using authored `1024x768` constants in Rust.  `RenderBridge` now owns the live
drawable extent, resolution changes update it before Lua relayout, and theme
motion, culling, anchoring, repeated-row/column traversal and ThemeSpriteData
wrap all consume that state.  Authored 1024-by-768 constants remain only in
native members where the disassembly actually contains those reference-space
values.

Four new focused regressions cover duplicate touch append/first-replace/all-
erase semantics, pre-callback float32 camera-scale capture, Lua-truthy
safe-to-quit frame latency, and theme centering before and after a live
drawable resize.  All 368 workspace tests pass (44 app/audio/wgpu, 19 assets,
one core and 304 script/physics), formatting and strict all-target Clippy are
clean.  The optimized application is SHA-256
`c8aa6a362420cb11c543677ac545509e4da78f6655ec5d748913e4d1a6e586bd`.
A new empty-AppData 16,500-frame menu/L01/result/L02/post-ability route reaches
the complete live L02 state as
`build/audit-live-extent-fresh-l02-16500.png`, with 88 optional data reads,
zero invoked fallbacks and zero compatibility bindings.  The visually checked
capture is SHA-256
`238c678180a359d72296a2625599e05bed6790986771a2c45863e8c2b02d38eb`.
This recovery and validation uses only Purple 1.1.6, its iOS resources,
IDA/Hopper and the Rust/wgpu desktop harness; Android hardware remains
unnecessary.

### Particle update gates, level-limit remapping, and signed wrap bounds

The `Particles` primary vtable begins at `_ZTV9Particles` `0x100A90810`
(address point `off_100A90820`). Its update member is slot `+0x48`,
`sub_100091834`; GameLua invokes that slot at `0x1000605A4..0x1000605D0` with
`W1 = 1`, `W2 = (GameLua+0x6A8 == 0)`, renderer `GameLua+0xD0`, scaled delta
in `S0`, and raw delta in `S1`. Consequently the GameLua particle movement
branch always has scale one. The otherwise visible renderer-height/768 path
belongs to the shared `ThemeParticleSystem` caller. The update-all versus
menu/notification-only branch is controlled by the physics-lock total, not by
GameLua `+0x199`: `enableInGameParticlesNative` (`sub_10004C7F4`) changes only
the flag consulted by the two in-game draw wrappers at `sub_10004C578` and
`sub_10004C5A8`. Rust now preserves this separation and the BOOLEAN adapter's
strict slot-one ABI.

The four infinite-particle wrap values are not a fixed 1024-by-768 rectangle.
`sub_10004FFB8` writes GameLua `+0x618..+0x624` at `0x1000503D4` and
`0x1000503DC`. It reads `level*EdgePhysics` and `oldLevel*EdgePhysics`, narrows
them with `FCVTZS`, adjusts the vertical pair using renderer width/height and
the fused `FNMSUB` at `0x10005031C`, notifies Particles slot `+0x28`
(`sub_100091544`), then divides the current four float32 bounds by the fixed
0.05 physics-to-framebuffer scale and vector-`FCVTZS` stores signed integers.
`sub_100091544` rescales only lifetime `-1.0f` records about the old viewport
centre with float32 `FMADD` at `0x1000915D0/0x1000915D8`. The update member
later loads those signed fields at `0x100091A68..0x100091ABC` and
`0x100091C3C..0x100091CE8`, using strict comparisons and opposite-edge
replacement.

The rehost now models that complete chain in the focused
`world_physics_camera_registration/camera/level_limits.rs` member, retains the
signed wrap rectangle in `RenderBridge`, remaps existing infinite particles
before publishing new bounds, and consumes those bounds after integration.
Regression coverage verifies strict edges, exact-edge retention, old-centre
remapping, draw-only in-game enable state, physics-lock update gating, and the
strict generated adapter. The full workspace passes 369 tests and
`cargo clippy --workspace --all-targets -- -D warnings`; release SHA-256 is
`05efaa44e63f32a45ee8d312ccd781e4701bd51727bbf3190918891bfd77bd10`.
A fresh-AppData automated route reached L02 with 88 optional missing data
globals, zero invoked fallbacks, and zero compatibility bindings. Its
1024-by-768 screenshot is
`build/audit-particle-bounds-fresh-l02-16500.png` with SHA-256
`152c44d0b42bebe4b9c2ea6d06db4f3748790281387dd71046635b77bf8206a4`.
No Android device or screenshot was used for this closure.

### Theme-particle reachability and independent visual-scale pairs

The native `ThemeParticleSystem` audit closes the apparent second particle
path without inventing content. Its primary vtable is `_ZTV19ThemeParticleSystem`
at `0x100A90A80` (address point `0x100A90A90`); slot `+0x48` shares
`Particles::update` at `sub_100091834`, while slot `+0x60` is
`sub_100096E4C`. The latter first clears ordinary particles through
`sub_1000912D8`, then erases and reinitializes both the integer-to-spawner map
at object `+0x88..+0xB0` and the integer-to-`vector<ParticleData>` map at
`+0xB8..+0xE0`. ThemeManager constructs two 0xE8-byte instances for background
and foreground passes. Their creation path is reachable, but the only shipped
population branch is the optional layer `spawnParameters` handling inside
`sub_100099C24`. Direct inspection of every 1.1.6 `bgLayers` and `fgLayers`
entry finds no such field, so the two maps remain empty for the supplied game
data. This dormant engine subsystem is therefore not a source of missing
textures or particles in the reproduced route.

A separate immediate-offset audit found an active visual difference. Purple's
`RenderObjectData` does not keep redundant scale copies: live render scale is
at `+0xBC/+0xC0`, while persistent bounce-base scale is at `+0xCC/+0xD0`.
`sub_100040304` and object-parameter cases 5, 17 and 18 write both pairs, and
`sub_1000403E4` queries the live pair. The per-frame bounce branch in
`sub_10005E898` instead loads only the base pair at `0x10005F824` and
`0x10005F844`, performs the two float32 `FMADD` calculations, and writes only
the live pair at `0x10005F840/0x10005F85C`. The previous Rust state collapsed
the base into collision-fixture scale, so a visually scaled object bounced
about the wrong size. `SceneObject` now retains independent live, base and
fixture scales; ordinary scale setters update both visual pairs, fixture
rebuilds remain independent, and bounce updates only the live pair.

Focused regressions cover a 2-by-3 visual scale with unchanged 1-by-1 physics
fixtures, native float32 bounce phase/decay, and the pre-error scale stores of
`setPhysicsScale`. The complete workspace still passes 369 tests, strict
all-target Clippy is clean, and the optimized executable is SHA-256
`b1ec751110925c697b4373e4469fdcf481c17e365b3771bad3ff482b29a19a5b`.
This recovery used only Purple 1.1.6, its shipped data, IDA/Hopper and the
desktop Rust test harness; no Android device or capture was used.

The previously 1,085-line physical mixer source is now split along the same
native ownership boundaries: `audio/native_mixer.rs` is the 243-line
`AudioOutputImpl` configuration, six-buffer queue and synchronization facade;
`native_mixer/engine.rs` is the 255-line integer block mixer and
`AudioClipInstance` lifecycle; `native_mixer/engine/reader.rs` is the 323-line
memory/decoder/composite `AudioClip` reader family; and the 292-line regression
suite is separate. This is a structural move with unchanged PCM, cursor,
prefill and worker-poll semantics. The post-split workspace again passes all
369 tests and strict all-target Clippy; the final optimized executable is
SHA-256 `8ae8d7f366ba11244508969bdf6e972c9032003f4b01756fe4d44fc1369cbd30`.

### Strict scalar object members and L02 tutorial route closure

The generated adapter families around `sub_100086690`, `sub_10008598C`,
`sub_10008897C` and `sub_100085BB4` establish strict stack contracts for five
previously permissive Rust bindings. `setTextureScale` and
`native_setWaterDensity` each consume one exact string and one number through
`sub_1005285CC`/`sub_10052859C`; `native_setIsWater` uses the same string
reader followed by the strict Boolean reader `sub_1005281BC`;
`setSensorMinimumAndMaximumForces` consumes a string and two numbers; and
`native_resizeRadius` consumes a string and four numbers. Every numeric value
is narrowed to float32 in the adapter before the member call. Extra Lua
arguments remain irrelevant, but absent or wrongly typed required slots now
raise an error.

The members themselves are deliberately native-only stores.
`sub_10004CE38` writes RenderObjectData `+0xC4`, `sub_100059530` writes
`+0x14B`, `sub_100059554` writes `+0x150`, and `sub_100031388` writes
`+0x10C/+0x110`; none writes `objects.world`. `sub_100059488` first resolves
the object, writes its radius at `+0x98`, destroys the old fixture and creates
the replacement circle with the four adapter values. All five members use
`sub_10005DAF8`, whose missing-map branch formats `Missing object: %s` and
throws. Rust now preserves the strict types, float32 boundaries, lack of Lua
reflection and failed-lookup behavior, including the resize fixture/contact
lifecycle already recovered earlier.

The deterministic L02 tutorial comparison was also corrected without an
Android reference. The shipped `Tutorial_StellaTap.lua` targets the current
level goal, and `TutorialTapArea.lua` requires a two-second stationary hold
within `screenHeight * 0.15` of that target. The valid deterministic event is
therefore a 150-frame hold around `(580,620)`, not a tap on the airborne bird.
Together with the existing menu, level-selection and sling events, this
dismisses the tutorial and exercises Stella's ability before the camera
returns to the next ready bird.

Focused regressions now prove strict arity/types, float32 rounding, missing
object errors, native-only material/water/sensor stores and radius replacement
behavior. The complete workspace passes 370 tests (44 app/audio/wgpu, 19
assets, one core and 306 script/physics), formatting and strict all-target
Clippy are clean. The optimized executable is SHA-256
`242a8b27ec26c99b43989bd7e6e5c15024c0323cefa1f89d46e1ed5bb137b9bd`.
The 16,500-frame desktop route reaches the stable live L02 sling-side state in
`build/audit-strict-scalars-l02-16500.png`, with 85 optional data reads, zero
invoked fallbacks and zero compatibility bindings; the visually inspected
capture is SHA-256
`c2a76ea12debac013ca5a74f7c4c01671fb07b2ed9ced9b7dc8ddffa8998632e`.
No Android program, hardware or screenshot was used.

### Object mutation, DrawablePolygon snapshots and strict table-stack boundaries

The adjacent object-member audit separates native body state from Lua mirror
state at several previously conflated boundaries. `applyImpulse`
(`sub_10003F930`) and `applyForceNative` (`sub_10003F9CC`) share the strict
string-plus-four-floats adapter `sub_100085BB4`; both use the nullable body
lookup and modify only a dynamic `b2Body`, with no immediate `objects.world`
write. `setMaterial` (`sub_10004CBD0`) recognizes only `wood`, `stone` and
`glass`, stores enum values 1, 2 and 3 at RenderObjectData `+0x18`, and treats
every other string as a total no-op before lookup. This enum is independent of
the Lua collision-material string. `setTexture` (`sub_10004CC74`) performs the
throwing render-object lookup and updates only the native resource name and
pointer. Both string/string entries use `sub_100089B0C`.

The scalar body cluster now follows its individual lookup contracts rather
than one blanket policy. Restitution/friction, sleeping and activity use the
nullable body lookup; damping, fixed rotation and sensor state manually find
an object and ignore absent/non-body entries; collision enable and gravity
scale first call throwing `sub_10005DAF8`. Their generated number/Boolean
adapters are strict and every number is narrowed to float32. The direct
`setPhysicsEnabled` member at `sub_100041ABC` strictly reads its first Boolean
and only reads an optional second string when present. `addVertex` uses
`sub_100088294`, requires two numbers and appends exactly two float32 values.
`setGameRenderingDisabled` (`sub_100059D58`) reads the strict Boolean at stack
slot -1, so the topmost extra argument wins rather than the first argument.

`makeRay` is not a live collision-debug overlay. `sub_10004CE5C` copies the
float2 contour at RenderObjectData `+0x168` with `sub_100024680`, copies the
current position at `+0xA4/+0xA8`, sets four float32 color channels through
`sub_1000249E8`, and inserts the completed `DrawablePolygon` into a unique-key
red-black tree. A duplicate name retains the first record. Its constructor is
mode 1 with outline byte zero, and `sub_100024A50` submits the ear-cut triangle
list through the current object callback context. Rust now retains that
immutable contour/position/color payload, does not move or rotate it after
later body transforms, and emits the alpha triangle list without the unrelated
software triangle-fan or black outline.

The hand-written table APIs also now preserve their native stack positions and
failure behavior. `decomposePolygon` (`sub_100035FFC`) strictly wraps argument
1 and reads every indexed point's numeric `x/y`; `getIntersectingObjects`
(`sub_10005411C`) and `getRayCastedObjects` (`sub_10005464C`) wrap stack slot
-1, require all six/four fields, and operate/publish at float32 precision.
The AABB member no longer reorders malformed lower/upper edges. `createJoint`
(`sub_100037374`) and the `createJoints` loop (`sub_10003CC64`) likewise use
the stack top and require a numeric `type`; malformed batch entries are not
silently skipped. `createTrack` (`sub_10003CD0C`) deliberately differs by
using positive index 1, strictly copies point tables and block strings, and
throws when a named RenderObjectData entry is absent.

Focused regressions cover strict types and arity, top-versus-first stack
selection, float32 publication, lookup/no-op distinctions, native-only state,
duplicate ray insertion, frozen ray geometry and the no-outline triangle-list
path. The complete workspace passes 374 tests (44 app/audio/wgpu, 19 assets,
one core and 310 script/physics); formatting and strict all-target Clippy are
clean. The optimized executable SHA-256 is
`5a63570d66eee8af17a5536640bed2ba27ee01caf11da8da0ad0e00a8dfc0a71`.
The no-Android 16,500-frame L02 route remains stable in
`build/audit-native-boundaries-l02-16500.png`, reports 85 optional data reads,
zero invoked fallbacks and zero compatibility bindings, and has SHA-256
`0cefffbd6e4a2cf2e903c47ff260daf5a430583e6c62dd2f0d0669061fff2434`.

### LightBeam filtering and strict particle-table cache construction

`makeLightBeam` installs the native object implemented by `sub_10005AD4C` and
constructed at `sub_10008B01C`. Its generated `plotPath` adapter
`sub_10008C384` consumes one table after the colon receiver, while the member
`sub_10008B194` strictly reads numeric `startAngle`, a table-valued
`startPoint`, and numeric `startPoint.x/y`. IDA and Hopper agree that the
fixture callback at `sub_10008B7B0` rejects sensors and Box2D shape type 3
(`b2ChainShape`), but does not inspect the separate RenderObjectData
collision-enable flag. Rust now has the same strict table boundary and
float32 path integration, continues until the native termination conditions
rather than an invented 4,096-iteration cap, ignores nearer sensors and chain
fixtures, and still stops on a nearer collision-disabled ordinary fixture.

The particle entry `sub_10008E524` is another hand-written stack-tail member:
it wraps Lua slot -1 rather than searching the argument list. It strictly
reads `definitionName`, `x`, `y`, `w`, `h`, `angle` and `mode` before consulting
the name cache. `amount`, `z` and `themeLayerIndex` are guarded numeric fields;
`ignoreDeltaTimeMultiplier` is a guarded Boolean and defaults true only for
modes 3 and 4. The cached 0x80-byte definition record strictly reads
`gravityX/Y`, velocity bounds, angular-velocity bounds, scale bounds, emitter
angle bounds, particle angle bounds and `lifeTime`; only `amount`, `areaW/H`
and emitter-area scales have numeric defaults. `sprites` is a required table
whose sequence entries are strict strings. Missing definitions are therefore
table errors, not silent empty bursts, and a cache hit does not relax the
required per-spawn fields. The Rust parser now follows these distinctions and
preserves the native float32 narrowing, zero-amount fallback, definition cache
and optional wrong-type fallback behavior.

The same audit exposed a cross-subsystem host-language trap. AArch64 `FCVTZS`
returns signed integer-indefinite (`INT_MIN`) for NaN and overflow, whereas a
Rust `as i32` cast saturates and maps NaN to zero. The recovered conversion now
lives in the shared Lua argument layer rather than the theme module, and is
used by particle amount/mode/layer fields, object parameters, gravity masks,
decoration counts, trajectory/AimStream counts, system-font metrics, dirt's
Clipper grid and unlock-checksum selection. `LuaResources::setClipRect`
(`sub_100448958`) additionally receives four already-narrowed floats, performs
its right/bottom additions in float32, then FCVTZS-converts the edges; the
2^24-plus-one regression distinguishes that sequence from a Lua-double sum.

Focused tests cover LightBeam table/field errors, sensor and collision-disabled
filtering, particle stack-top selection, required emitter fields, malformed
definition records, optional type fallbacks and cached first-use state. The
complete workspace passes 375 tests (44 app/audio/wgpu, 19 assets, one core
and 311 script/physics); formatting and strict all-target Clippy are clean.
The optimized executable SHA-256 is
`3c3c584c047b935103492b7fea50321ff37546a6f34721b6230f2df514bab496`.
The original iOS 1.1.6 Lua bytecode completes the no-Android 16,500-frame L02
route with the stricter particle ABI, 85 optional data reads, zero invoked
fallbacks and zero compatibility bindings. The visually inspected capture is
`build/audit-no-android-fcvtzs-l02-16500.png`, SHA-256
`ebef3a6265aa831e5d88dc0b2a5f301ab624bc157dac2a8e4039f7867f695a98`.

### Composite-sprite handwritten stack ABI and entry-map mutation

IDA and Hopper agree that the three composite query members are handwritten
Lua-state functions rather than generated overload adapters.
`getCompoSpriteData` at `sub_1004493BC` strictly reads the string in positive
stack slot 1 and returns a one-based array whose entries contain only
`{name, x, y}` in numeric slots 1, 2 and 3. It immediately calls the native
part-count member on the lookup result and has no missing-resource branch; the
Rust host now reports that invalid lookup as a Lua error instead of inventing
an empty composite. `getCompoSpriteEntry` at `sub_1004497D8` and
`setCompoSpriteEntry` at `sub_100449CFC` differ: a missing composite is logged
and returns zero Lua values before later arguments are inspected. This is
observably different from returning one `nil`.

Both entry members test selector slot 2 as a number before testing it as a
string. The numeric path strictly fetches a float, narrows it to float32,
executes AArch64 `FCVTZS Wd, Sn`, and passes the resulting unsigned index to
the raw part vector. Consequently `-0.75` selects entry zero, while NaN,
negative integral and overflow values become invalid unsigned indices. A
numeric string enters the `lua_isnumber` branch but is then rejected by the
strict NUMBER-tag reader; it must not fall through to the name map. Unsupported
selector types return zero values. Native invalid vector/map entries are later
dereferenced without a safe branch, so the Rust boundary preserves failure as
a recoverable Lua error rather than returning a fabricated `nil` or risking a
host-process crash.

The get result publishes `name`, `x`, `y`, `scaleX`, `scaleY`, `flipX`,
`flipY`, `angle` and `visible`. The setter requires its third argument to be a
table only after the composite and selector branches succeed. Every field is
optional by a nil check; present scalar fields intentionally use Lua 5.1
`lua_tolstring`, `lua_tonumber` and `lua_toboolean` coercions rather than the
generated strict adapters. This includes hexadecimal numeric strings,
nonnumeric values becoming zero and ordinary Lua truthiness for flips and
visibility. Renaming retains the complete `#suffix` in the entry and name map,
strips it only for the sprite-resource lookup, erases the old key and rebuilds
the composite geometry. The Rust live part vector and wgpu asset-update queue
now follow the same mutation.

Focused regressions cover strict slot 1, colon-call rejection, zero-result
counts, resource-before-selector/table checking, numeric-string rejection,
float32 `FCVTZS`, safe invalid-entry failure, partial updates, `#suffix`
renaming and Lua 5.1 hexadecimal/falsey coercions. The complete workspace
passes 376 tests (44 app/audio/wgpu, 19 assets, one core and 312
script/physics), formatting and strict all-feature Clippy are clean. The
optimized executable SHA-256 is
`c2df62cc9d4155a7fa7e9f21f9e45e773cc158805a7a7d550f72914c6de6407d`.
A fresh-AppData, no-Android 16,500-frame run of the original iOS Lua bytecode
reaches the live L02 post-ability scene with 82 optional data reads, zero
invoked fallbacks and zero compatibility bindings. Its visually inspected
wgpu capture is `build/audit-composite-abi-l02-16500.png`, SHA-256
`81d74b10025541544ba9a474a143091580fcaf2a61d475bfa68f73dc8a68535d`.

### FilePath-keyed resource creation, replacement and symmetric release

The ResourceManager lifecycle maps are keyed by parsed `game::FilePath`
components rather than by the caller's complete string. IDA decompilation of
`sub_1004FCEDC` and Hopper's independent pseudocode establish the object
layout: `+0x00` is the normalised complete path, `+0x08` the drive, `+0x10`
the directory including its final slash, `+0x18` the filename before the last
dot, and `+0x20` the extension including the dot. The parser converts
backslashes to slashes, uppercases a lowercase drive prefix and performs its
native `./`/`../` in-place collapse. `setPath` is the generated raw-method
thunk `sub_100446E24`, tail-calling `sub_100457140`; that member first parses
the supplied string as a FilePath before assigning ResourceManager `+0x58`.
`sub_1004FD2F0` is the corresponding `(base, child)` constructor and delegates
to the same parser.

SpriteSheet create/release (`sub_100457E38`/`sub_10045ABC4`) and
CompoSpriteSet create/release (`sub_1004586AC`/`sub_10045AF5C`) parse the
caller path, copy its `+0x18` stem, parse that stem a second time, and use the
second `+0x18` value as the red-black-tree key. Thus
`images/MENU.PROFILE.dat` is keyed as `MENU`, not as the complete path or as
`MENU.PROFILE`. Bitmap-font create/release
(`sub_100459324`/`sub_10045B214`) and TextGroupSet create/release
(`sub_100459AD8`/`sub_10045B41C`) use one `+0x18` extraction. The apparent
one-argument release thunks at `sub_1004481AC`, `sub_1004481B4` and
`sub_1004481BC` preserve X1 and tail-call those members; registration at
`sub_100446570` proves that all three use the generated
`void(std::string)` raw dispatcher. Hopper assembly confirms the preserved-X1
tail branch that Hex-Rays omits from its prototype.

Every creator probes its optional replace Boolean by exact Lua type. When it
is false and the key already exists, the native returns the existing pointer
without loading the new path. A true value constructs the resource from
`FilePath(currentPath, callerPath)` and replaces the map node. SpriteSheet and
CompoSpriteSet accept only the case-sensitive `.dat` and `.json` loader
branches. The third `createSpriteSheet` Boolean independently defaults true,
but its X3 value is unused by `sub_100457E38`. Bitmap and system fonts share
the same native `IFont` map, so a successful forced replacement removes the
prior font kind rather than leaving two independently selectable entries.
`releaseSpriteSheet` alone is handwritten and strictly reads an explicit
second Boolean; the Boolean selects resource release versus map erasure after
the same double-stem lookup.

Rust now uses host-independent FilePath spelling and stem extraction, applies
the double/single key rules on both create and release, normalises `setPath`
and joined source paths, retains each map value's resolved source identity,
preserves an existing object when replace is false, and updates it when true.
Font replacement and path-form `releaseFont` now operate on the shared logical
key. Default releases clear both lifetime membership and retained source
identity, while `releaseSpriteSheet(path, true)` retains the map node and
records its cleared resource pointer until a forced replacement reconstructs
it. Focused tests cover forward/backward slashes, drive casing, native
dot-segment behavior, dot files, two-extension keys, non-replacing duplicates,
forced replacement, retained-node resource release, current-font invalidation
and create/release symmetry.

The complete workspace passes 378 tests (44 app/audio/wgpu, 19 assets, one
core and 314 script/physics); formatting and strict all-target/all-feature
Clippy are clean. The optimized executable SHA-256 is
`5fc96a6d006848fa5813d7df83b06a19b607b13a2d02bb3b253bc03b74169280`.
A fresh-AppData, no-Android 16,500-frame L02 route of the original iOS Lua
bytecode reaches the visually inspected post-ability scene with 82 optional
data reads, zero invoked fallbacks and zero compatibility bindings. The 1024
by 768 wgpu capture is `build/audit-resource-release-mode-l02-16500.png`,
SHA-256 `dfe175e69eff6fb2f7cfc4b54e18f9ea618453429df61467c7072f7f1b717604`.

### TextGroupSet locale loading and current-font query contracts

The locale bindings are stateful ResourceManager members, not a permanently
preloaded host dictionary. Registration at `sub_100446570` publishes
`loadLocale` through `sub_1004482B8`/`sub_10045B7AC`, `useLocale` through
`sub_1004482C0`/`sub_10045BBF8`, and `getString` through
`sub_100448A64`/`sub_10045C380`. IDA and Hopper independently show that a
missing TextGroupSet makes `loadLocale` a zero-result no-op. For an existing
set, `sub_1004741C0` first releases the loaded `"ALL"` selection and
`sub_1004731B0` then loads either the exact requested language or every
language for the literal `"ALL"`. The release happens before validation, so a
failed request for an absent language leaves the previously loaded groups
cleared and raises the native language-specific error. `useLocale` merely
assigns the current locale string and performs no validation.

`sub_1004743D4` distinguishes a language absent from the source data from a
present language whose TextGroup has not been loaded. `getString` returns its
key unchanged when the named TextGroupSet itself is absent. Once a language
group is loaded, `sub_1004725A8` also inserts and returns the key as the
fallback for a missing translation. Rust now keeps source-file identity with
each created TextGroupSet, parses the requested locale on `loadLocale`, clears
all cached language instances in the native order, preserves the two native
error cases, and removes cached translations on replacement or release.

The adjacent metric thunks at `sub_10044A998`, `sub_10044A9BC`,
`sub_10044A9D8`, `sub_10044A9F4`, `sub_10044AA10` and `sub_10044AA2C` cast the
integer IFont virtual results to float32 for Lua publication. Their members
`sub_10045CE44` through `sub_10045D4AC` require a current font, use virtual
slots `+0x30`, `+0x48`, `+0x50`, `+0x58` and `+0x60`, and compute height as
max-ascending plus max-descending. The existing Rust bitmap/system metric
paths and exact missing-font messages already satisfy those recovered
contracts.

Focused regression covers absent sets, present-but-unloaded languages,
invalid-load clearing, exact-language and `"ALL"` loading, missing-key
fallback, release invalidation and zero-result lifecycle ABI. The complete
workspace passes 379 tests (44 app/audio/wgpu, 19 assets, one core and 315
script/physics); formatting and strict all-target/all-feature Clippy are
clean. The optimized executable SHA-256 is
`82611aad9217f279c265df024e3b688caf06eeb17d3a0db103ec5c72bb71da1f`.
A fresh-AppData, no-Android 16,500-frame run of the original iOS Lua bytecode
again reaches the live L02 post-ability scene with 82 optional data reads,
zero invoked fallbacks and zero compatibility bindings. The visually
inspected 1024 by 768 wgpu capture is
`build/audit-locale-lifecycle-l02-16500.png`, SHA-256
`90df03a3322de8744f6ed792125e9d2475e63cb183e4e5198a612bdcd5db652e`.

### Resource file dispatch and failed-construction transaction order

IDA and Hopper independently confirm four different constructor paths rather
than one generic ResourceManager insertion rule. SpriteSheet creation at
`sub_100457E38` and CompoSpriteSet creation at `sub_1004586AC` compare the
case-sensitive FilePath extension with exactly `.dat` and `.json`. Their
binary loader virtuals are `sub_100461050` and `sub_100461B18`; their JSON
loader virtuals are `sub_100463338` and `sub_10046574C`. An unsupported suffix
leaves the native loader pointer null and faults at its indirect call. The Rust
host preserves that hard failure as a catchable Lua error at the process
boundary instead of terminating the cross-platform executable. A duplicate
with replace=false returns before this dispatch and therefore does not require
the candidate file or even a supported suffix.

Both SpriteSheet and CompoSpriteSet fully parse the candidate before touching
the old map value. Sprite replacement then unregisters the old type-1 sprite
names through `sub_1004578BC` and registers the new names through
`sub_100457C84`. Compo replacement uses the analogous type-2 paths
`sub_1004571A8` and `sub_100457570`, but only when the parsed set's native map
count at object `+0x40` is nonzero. A malformed candidate therefore preserves
the old object in both maps, and a successfully parsed empty composite is also
a no-op. Bitmap-font creation at `sub_100459324` has no extension switch:
`sub_10042A5B0` constructs and parses the binary FONT completely before the
shared IFont map's `operator[]` assignment, so its failure also preserves the
old bitmap or system font.

TextGroupSet creation at `sub_100459AD8` deliberately has the opposite
transaction boundary. It obtains or inserts the map slot, constructs a new
object through `sub_10047287C`/`sub_100472760`, assigns it and destroys the old
shared pointer, and only then invokes the TEXT parser `sub_1004729A4`. A parse
exception consequently leaves the new empty TextGroupSet installed and the
old loaded language data gone. Rust now records that constructed-but-unloaded
state explicitly, commits it before validation, and freezes every successfully
parsed LocalizationTable and BitmapFont object in ResourceRuntime so later
locale and metric calls observe the constructed object rather than reopening a
possibly changed host file.

The JSON SpriteSheet parser at `sub_1004637FC` recognizes the exact
TexturePacker app URL and the Adobe/ArtPacker families. TexturePacker's object
form of `frames` raises `Unsupported TexturePacker JSON sheet format (use JSON
Array format instead)`; unknown exporters raise `Unsupported JSON sheet
format`. The Compo parser at `sub_1004657CC` accepts only Adobe/ArtPacker,
treats an absent or empty `compo` array as an empty set, walks each sprite-part
array in reverse, and raises `Unsupported JSON composprite format` for another
family. The new split lifecycle `loading` module enforces these branches and
validates the recovered required frame, pivot, transform, scale, flip and angle
shapes.

Focused regressions cover exact extension casing, both supported loader
families, TexturePacker array rejection, missing-file duplicate short-circuit,
malformed forced replacement, empty-composite preservation, shared-font
replacement and TextGroupSet's post-commit failure. The complete workspace
passes 381 tests (44 app/audio/wgpu, 19 assets, one core and 317
script/physics); formatting and strict all-target Clippy are clean. The
optimized executable SHA-256 is
`0cc787e5874872aa19d3b6d63f66b7d3fe2ff2a7ce58746fb3193c9f96a76cd4`.
A fresh-AppData, no-Android 16,500-frame run of the original iOS Lua bytecode
again reaches the live L02 post-ability scene with 82 optional data reads,
zero invoked fallbacks and zero compatibility bindings. The visually inspected
1024 by 768 wgpu capture is
`build/audit-resource-loader-l02-16500.png`, SHA-256
`83b57f1bd6e08fd85495aec95b6fd37bde07c0245644bd2ea4df2871252ab3c9`.

### Shared sprite-name stacks, live draw dispatch and downloadable sheets

The older global `ResourceManager` does not own a second sprite database.
IDA and Hopper both show `native_createSpriteSheet` at `sub_10009470C`
calling the same `sub_100457E38(Resources, path, false, true)` member used by
`res.createSpriteSheet`; `native_releaseSpriteSheet` at `sub_100094800`
similarly reaches `sub_10045ABC4(Resources, path, false)`. Its decoded-byte
accounting remains a separate legacy map, but the actual sheet pointer and
sprite-name lifetime are shared. Rust now forwards both legacy calls through
the common FilePath-keyed lifecycle instead of creating a second full-path
sheet entry.

Every SpriteSheet registration at `sub_100457C84` appends a type-1 entry to
the name vector, while `sub_1004578BC` removes that sheet's entries. The
corresponding CompoSpriteSet members `sub_100457570` and `sub_1004571A8` use
type 2. The central lookup `sub_10045BDDC` examines only the vector's last
entry; when a requested type does not match that last entry it returns null
rather than searching backward. This makes a later same-named resource shadow
an older one and makes release expose the previous last entry. ResourceRuntime
now stores the parsed sheet/set objects and reproduces this vector order,
type gate, same-owner replacement and release fallback. Sprite bounds, pivots,
composite data/entry mutation and RenderBridge's active composite table all
read this live registry.

The same rule now reaches drawing rather than stopping at queries.
`sub_10045C144` calls `sub_10045BDDC(name, 0)` and immediately returns on a
missing name; its type-2 branch calls `sub_1004376D4`, and its type-1 branch
calls `sub_100467AF0`. The former Rust static-preload/zero-anchor fallback has
therefore been removed, and a missing resource no longer submits a command or
consumes render order. Tests that inspect render state now explicitly create a
small parsed SPRT instead of relying on fictitious names.

The four global GameLua helpers use the same live database with narrower type
rules. `drawCompoSprite` at `sub_10004DDA0` obtains a type-2 record through
`sub_10045C06C`; `drawSpriteWithoutShader` at `sub_10004E300` asks only the
atlas virtual; `drawSpriteWithShader` at `sub_10004E070` tries the atlas first
and then `sub_10045BF08`'s type-2 lookup; and `isCompoSprite` at
`sub_10004E3A0` is exactly a non-null `sub_10045BF08` test. Their already split
`direct_sprite_registration/{composite,plain,shader,lookup}.rs` leaves now
share ResourceRuntime instead of the constructor-time all-bundle scan.
Releasing the selected composite consequently changes `isCompoSprite` to
false and prevents later direct submission.

Downloadable `Assets.createSpriteSheet` is also part of this shared map.
`sub_1000AC660` constructs a sheet with the explicit descriptor and texture
paths through `sub_1004617AC`, then calls `sub_100457724(Resources, name,
sheet)`. That last member unregisters the old name's sprites, registers the new
sheet's type-1 entries and replaces the map pointer. The offline Assets adapter
now parses a cached binary/JSON descriptor completely, installs its explicit
texture path, and only then performs the same transactional replacement.
Malformed replacement therefore preserves the previous dynamic sheet, and
its internal sprite names are visible to the ordinary resource queries.

Focused regressions cover legacy forwarding, duplicate name shadowing,
release fallback, cross-type last-entry rejection, missing-resource draw
ordering, live direct-helper lookup, release-sensitive `isCompoSprite`, and
downloaded-sheet parse/failure transactions. The complete workspace passes
383 tests (44 app/audio/wgpu, 19 assets, one core and 319 script/physics);
formatting and strict workspace all-target Clippy are clean. The optimized
executable SHA-256 is
`3d62edd0e8cceec4bf3e67f6e96417aa9b187300e04d7251786bf3c9685d754c`.

A new empty-AppData, no-Android 16,500-frame run of the original iOS 1.1.6
Lua bytecode completes the menu/L01/result/L02 route with 88 optional data
reads, zero invoked fallbacks and zero compatibility bindings. The 1024 by 768
wgpu capture `build/audit-live-resource-stack-l02-16500.png` is visually
complete and has SHA-256
`08d7f484d48abfd93b4b9b3b08e180b5c8d300e4d9c4d529a6194b5e2b478df9`.
Its camera stop differs from earlier deterministic captures, reinforcing that
these PNGs are route/render completeness checks rather than a substitute for
the executable's disassembled control flow or a version-matched native frame.

### Submission-time sprite ownership and retained scene-object pointers

The iOS 1.1.6 executable is sufficient to close this ownership behavior; no
Android device or Android 1.1.5 frame was used. IDA shows that Purple's draw
paths submit resource pointers, not names for a later renderer lookup.
`sub_100043990` resolves the selected object's first image through
`sub_10045BC64`/`sub_10046B1F0`; `sub_1000343CC` and `sub_100096344` resolve
the masked image before constructing its quad; and the textured-line paths
`sub_10006DB0C`/`sub_100030EB0` consume an already resolved image. The Rust
draw adapters now copy a `SpriteCatalogRegion` into every corresponding
deferred wgpu command, including ResourceManager draws, selected/masked
objects, textured lines, rubber bands and box segments. Same-frame
draw-then-release and same-name replacement can therefore no longer rebind an
already submitted command.

The COMP loader has an even stronger lifetime boundary. `sub_100461B98`
iterates the ordered SpriteSheet map, resolves every child AtlasSprite while
loading, raises `Sprite "{0}" not loaded while loading {1}` at the first
missing child, and stores the resolved pointer in each part. It does not allow
a later sheet with the same child name to change that pointer.
`sub_1004376D4` subsequently traverses those retained records and calls
`sub_100467BE0` directly on each stored AtlasSprite. ResourceRuntime now keeps
the part and region vectors index-aligned, validates the complete COMP before
commit, and exports private catalog aliases only as an internal way to carry
pointer identity through the deferred host. These aliases are not visible to
Lua and do not alter the native name map.

Scene objects retain the same identities. `sub_10004C7FC` writes either the
CompoSprite pointer at RenderObjectData `+120` or the AtlasSprite pointer at
`+144`, then keeps a null pointer when neither lookup succeeds. Existing
objects must consequently survive source-sheet release without changing to a
new same-named sprite, while a missing assignment must not spring into view if
that name loads later. `SceneObject` now freezes the assigned atlas or ordered
composite children; an empty bound-composite vector is the host-only null
pointer sentinel that preserves native callback/command ordering while
preventing future catalog lookup. `RenderCommand.bound_composite` carries
those frozen children into wgpu, whose traversal prioritizes the retained
parts and their regions over the active catalog. ResourceManager composite
draw submissions use the same mechanism.

The growing ResourceRuntime implementation was also split along the recovered
native clusters. `resource_manager/runtime.rs` now contains the shared state
layout and constructor (261 lines),
`runtime/sprite_lifecycle.rs` contains SpriteSheet/CompoSpriteSet construction,
replacement and release (163 lines), and `runtime/sprite_catalog.rs` contains
active lookup, retained pointer access, geometry and deferred catalog export
(263 lines after this batch). This is a structural split only; focused
shadow/release tests and the complete suite guard the original behavior.

Regressions now cover immediate atlas submission, COMP load-time pointer
freezing, object construction and `native_setSprite`, null lookup retention,
active-name shadowing, release of the source sheet, and wgpu rendering from a
retained composite after the active catalog is empty. The complete workspace
passes 389 tests (46 app/audio/wgpu, 19 assets, one core and 323
script/physics); formatting and strict all-target/all-feature Clippy are
clean. The optimized executable SHA-256 is
`a8e46c28f6f832e2854d92caff04cc27f225c10c8023f0b66c7e9578161f2868`.

A fresh empty-AppData, no-Android 600-frame run of the original iOS Lua
bytecode reaches the complete main menu with 67 optional data reads, zero
invoked fallbacks and zero compatibility bindings. The visually inspected
1024 by 768 wgpu capture is
`build/audit-object-pointer-menu-600.png`, SHA-256
`470f6255b774e61e7c529a77c754a2812c9974790bbd83c224d74b1f9c7339c9`.
As with earlier captures, it is a route/render regression artifact rather than
the source of truth for native semantics.

### Remaining submission-time image pointers and special wgpu quads

This closure used the connected IDA MCP directly and did not use an Android
application, device or screenshot. Theme, particle, trajectory and decoration
descriptors retain names until their native draw members run, so the Rust
adapters now resolve and freeze atlas/composite pointers at command submission
rather than at descriptor construction or later wgpu traversal. The selected
sprite helper has two independent native operands: `sub_100043990` resolves
the atlas sprite through `sub_10045BC64`/`sub_10046B1F0` and also obtains the
second image passed to `sub_10008D428`. `RenderCommand` consequently carries a
separate retained masked-texture binding. Render objects use the same binding
for the image pointer stored by `setTexture` at RenderObjectData `+128`.

Dirt has a different, earlier lifetime boundary. The IDA decompilation of
`sub_10001F98C` shows `bgTexture` and `fgTexture` being read once, followed by
two `sub_10045BC64`/`sub_10046B1F0` chains at `0x10001FF88` through
`0x10001FFC8`. The foreground image result is stored at DirtMechanics `+232`;
the background result is immediately installed on the first
DrawablePolygon. The Dirt component now receives ResourceRuntime in its
factory, freezes both resolved texture sources when the native component is
created, and copies them into `DirtRenderCommand`. Same-name resource
shadowing and release can no longer change either polygon's image in the
deferred host.

One wgpu-only bypass was found during the catalog audit. Commands containing
`native_sprite_quad` or `explicit_quad` previously entered a special branch
that looked the sprite up again by name even when the script command already
contained its resolved atlas region. This contradicted `sub_1000343CC`, which
resolves the image before calling `sub_100096344`; that renderer stores the
passed image at its `+40` field before building the six vertices. Both wgpu
quad paths, as well as the test-only CPU reference quad, now prefer the
submission-time region and use the live catalog only for commands that did not
receive a native pointer.

Focused regressions cover theme, particles, both trajectory paths,
decorations, the selected helper's two images, object `setTexture`, Dirt's two
constructor-time images, and both special wgpu quad branches across active
catalog replacement/release. The complete workspace passes 398 tests (48
app/audio/wgpu, 19 assets, one core and 330 script/physics); formatting and
strict all-target/all-feature Clippy are clean. The optimized executable
SHA-256 is
`d41ce21d43e591ce06e56d71880094d6432813d63593a9928a434d7097eb0261`.

A fresh isolated AppData, no-Android 600-frame execution of the original iOS
1.1.6 Lua bytecode reaches the complete main menu with 67 optional data reads,
zero invoked fallbacks and zero compatibility bindings. The visually inspected
1024-by-768 wgpu capture is
`build/audit-pointer-ownership-menu-600.png`, SHA-256
`dc90aeb764a15039ccf0119db1d2cdb00adee85a002853f6fef7df98440bfc63`.
It remains a deterministic completeness regression; the iOS executable and
bundle resources are the behavioral and visual source of truth.

### Current-IFont submission ownership and the drawString3D string ABI

This pass used both connected disassemblers and deliberately did not infer
semantics from a screenshot. IDA decompilation of `sub_10045C1FC` shows the
ResourceManager reading its current `IFont *` from `+0x48`, throwing when it
is null, obtaining virtual slot `+0x10`, resolving the localized string through
`sub_10045C380`, and invoking the font immediately. Hopper independently shows
the same `r0[9]` pointer read, null branch and indirect slot call.
`sub_10045BAC4` establishes how selection is represented: it searches the
shared IFont map, copies the selected node's object pointer into `+0x48` and
stores the name separately at `+0x50`.

The BitmapFont virtual at `sub_10042B338` then walks the constructed font's
glyph tree and submits each retained AtlasSprite through `sub_100467A00` (or
the installed callback) during that call. Both IDA and Hopper show that the
font object, its glyph metrics and the resolved sprite/texture identity are
therefore consumed before a later resource-map mutation. A deferred wgpu host
cannot reproduce this by looking up only `TextRenderCommand.font` at frame
preparation time: a same-name replacement or release would incorrectly rebind
an already submitted draw.

The 3D adapter had a separate ABI error. `sub_100087BB4` reads two required
strings followed by seven float32 numeric arguments. `sub_10003457C` does not
select the second string as a font; after installing its perspective/model
state it forwards both strings directly to `sub_10045C1FC`. They are the
TextGroupSet name and localization key, and the already-selected current IFont
performs the draw. The prior Rust adapter mislabeled them as `text, font`, did
not localize the key and consequently attempted to render with a key-named
font, producing silent missing text.

`TextRenderCommand` now carries an optional `TextFontBinding`. Every native
ResourceManager, ordinary UI and 3D-text submission records either the exact
parsed BitmapFont plus its constructor-relative resolved atlas source, or the
distinct system-font kind. ResourceRuntime retains the concrete FONT
descriptor path so later filesystem or ResourceManager path changes cannot
alter that source. Both the wgpu expander and test-only CPU renderer prefer
this frozen binding; their old static font catalog is used only for explicit
legacy/test commands without a native binding. The public `getString`, 2D
drawString, ordinary UI text and 3D text now share one implementation of
`sub_10045C380`'s TextGroupSet/current-locale contract.

Focused regressions prove that the 3D strings localize as group/key, that the
current bitmap font rather than the second string is selected, that a submitted
command keeps its original glyph width and atlas path after a forced same-name
replacement and release, and that wgpu uses the frozen texture and geometry
instead of the active catalog entry. The complete workspace passes 401 tests
(49 app/audio/wgpu, 19 assets, one core and 332 script/physics); formatting and
strict all-target/all-feature Clippy are clean. The optimized executable
SHA-256 is
`30b2a8824c345d264c34f3a49790c7ee1414599f7b5c30bce213732e306bcf15`.

A fresh isolated-AppData, no-Android 600-frame headless run and a 600-frame
wgpu run both completed with 67 optional data reads, zero invoked fallbacks and
zero compatibility bindings. The latter emitted
`build/audit-text-binding-smoke-600.png`, SHA-256
`070ce1675e5fd44635114093c69b280626db562a35417c35f2a99f4d2c57942f`.
Per `docs/visual-reference-policy.md`, this image proves only that the live
wgpu/resource path completed; it was not used to accept positions, texture
choice, timing, shader behavior or visual fidelity.

### UIKit SystemFont label ownership, ARGB ABI and deferred wgpu rasterization

This pass used IDA and Hopper independently and did not inspect a rehost
screenshot to choose any visual behavior. `game::SystemFont::Impl::drawString`
at `0x100475B98` returns for an empty string, subtracts the stroke width from
both input coordinates and clips the requested substring. Its low anchor word
subtracts ascending for BASELINE (`3`), ascending plus descending for BOTTOM
(`2`), or half that sum for VCENTER (`1`); TOP (`0`) and VPIVOT (`4`) take the
unshifted default branch. Its high anchor word subtracts the complete
string width for RIGHT (`2`) or half for HCENTER (`1`). The resulting anchored
coordinates are converted to integers before the cached label texture is
drawn.

On a LabelPool miss, the same routine measures an NSString with the retained
UIFont, truncates width and height, adds twice the stroke width, and creates an
8-bit, four-channel bitmap context with format value `6` (`A8B8G8R8`,
premultiplied-last). The context is vertically flipped. A stroke of at least
one pixel translates the text origin by the stroke width, selects the stroke
color, a line width of twice the requested width and a round join, draws the
text in stroke mode, then performs the fill draw. The pool key hashes family,
text, point size, packed fill color, stroke width, packed stroke color and
style in that order. `sub_100477E38` clamps normalized color components and
packs them as `AARRGGBB`.

The constructor at `0x100477668` confirms the Impl ownership layout: family at
`+24`, size at `+32`, fill Color at `+36`, stroke width at `+52`, stroke Color
at `+56`, style at `+72`, metrics at `+76/+80/+84`, and the retained UIFont at
`+88`. The two Lua adapters, `sub_1004471B4` and `sub_100447480`, accept colors
as alpha, red, green, blue rather than RGBA. Each number narrows through
float32 and FCVTZS before packing, without an explicit per-input byte mask;
the Color constructor at `sub_100477DD0` then normalizes the resulting bytes.
The non-stroked adapter supplies zero width and opaque black for the unused
stroke color.

A runtime interception of the shipped iOS Lua initialization, immediately
before `initializeGameMenus`, recorded the actual boot call as
`SYSTEM_FONT, ArialRoundedMTBold, 40, 255, 0, 0, 0, 0`: point size 40, opaque
black fill, style zero and no stroke. The constructed metrics are ascending
37, descending 8, leading 0 and height 45; the native width of `Stella` is
110. These values, rather than a screenshot estimate, now define the default
menu label contract.

`TextFontBinding::System` now owns an immutable render binding containing the
resolved face bytes and index, family, size, fill/stroke colors, style and all
three metrics. This freezes the font selected when the command is submitted;
same-name replacement or release cannot rebind a deferred frame. The
cross-platform rasterizer is isolated in `assets/system_font.rs`, while
`gpu/frame/text/system.rs` creates and submits the generated premultiplied
label texture. Both 2D and projected 3D text use the concrete retained binding.
The test-only CPU renderer follows the same dispatch so it no longer silently
omits SystemFont commands.

A follow-up instruction audit closed two integer-boundary differences without
using a visual comparison. In both the LabelPool hit path near `0x100475E04`
and miss path near `0x1004762A0`, IDA shows `FCVTZS` of the float32 coordinate
after stroke subtraction and anchor subtraction; Hopper independently exposes
the same conversions immediately before virtual texture draw slot `+40`.
`TextRenderCommand` therefore retains the local float32 IFont origin in
addition to its transformed screen origin. The deferred renderer now performs
the two native FSUB operations, converts toward zero, and only then applies the
captured 2D matrix or 3D projection. This is observably different for
fractional and negative coordinates and cannot be reconstructed by rounding a
finished screen-space vertex.

The hash arithmetic also uses signed operands. The instruction sequence
sign-extends the point size, both packed `AARRGGBB` colors, stroke width and
style from 32 to 64 bits before wrapping additions and multiplications by 33.
The earlier Rust path zero-extended opaque colors. The shipped boot pair
`ArialRoundedMTBold`/`Stella` now produces the recovered signed-field key
`0e93feaf876257ce`, rather than the zero-extended `0e948b31876257ce`.

SystemFont also owns the LabelPool lifetime rather than leaving it alive for
the process. The successful constructor increments `dword_100C12B68` at
`0x1004777A4`. IDA decompilation of the destructor at `0x100477BC4` and Hopper's
independent pseudocode both show a decrement followed by a one-to-zero test.
On that transition Purple resets the pool count, erases its complete red-black
tree, restores the empty sentinels and clears the remaining pool state. A
same-parameter font constructed after the last instance dies therefore starts
with an empty cache even though its numeric label hash is identical.

ResourceRuntime now advances a LabelPool epoch only when removal actually
destroys its final SystemFont value. Successful same-name SystemFont
replacement does not advance it because the new Impl is constructed before
the old map value is overwritten, matching the native nonzero instance count.
The epoch is frozen into every deferred `SystemFontRenderBinding`. It is not
part of Purple's recovered numeric hash; it is appended only to the host-side
texture identity so release-before-frame-preparation can keep an old submitted
label and a newly constructed same-hash label distinct in one wgpu frame.

The metric virtuals have a second, independent rounding boundary. The byte
string wrappers at `0x100476424` and `0x100476538` convert UTF-8 to the engine's
UTF-32 string first. Their wide overloads at `0x10047692C` and `0x10047676C`
take the requested substring in Unicode-codepoint units, convert it back to
UTF-8, call `NSString sizeWithFont:`, and apply `FCVTZS` directly to the
returned width or height. Hopper independently shows the same substring,
conversion, Objective-C call and final conversion sequence. In contrast, the
constructor stores ascender, negated descender and leading only after three
separate conversions. Adding those already-truncated metric values is not the
same operation as measuring the complete NSString height.

`SystemFontRenderBinding` consequently retains a separate label line height
derived from the unrounded face metrics before their individual integer
boundaries. Cached single-line and multiline texture allocation uses this
measurement plus twice the stroke width. TOP/VCENTER/BOTTOM/BASELINE anchoring and the
public max-ascending/max-descending/height metric queries continue to use the
constructor's separate integer fields. The wgpu regression covers one-line,
two-line and stroked allocation so the two contracts cannot silently collapse
again.

Focused regressions cover the exact Lua ARGB conversion (including its
sign-extension edge), constructor argument slots, metrics and retained face
bytes, replacement/release lifetime, native stroke texture dimensions,
premultiplied fill/stroke pixels, signed LabelPool hashing, positive/negative
fractional FCVTZS behavior, non-uniform matrix ordering and RIGHT/BOTTOM
anchored wgpu geometry. A dedicated delayed-frame regression releases the
last SystemFont, recreates the same family/size/color and submits both
same-text draws together, proving that their texture identities remain
separate. The complete workspace passes 407 tests (53
app/audio/wgpu, 19 assets, one core and 334 script/physics); formatting and
strict all-target/all-feature Clippy are clean. The optimized executable SHA-256 is
`8d544f93637975ee5cc529458493e3a76281191ab1105fcf93fad4ae16f10f9d`.

A new isolated-AppData, no-Android 600-frame headless run completed with 67
optional data reads, zero invoked fallbacks and zero compatibility bindings.
A separate 120-frame wgpu probe invoked the shipped SYSTEM_FONT call on every
draw and completed with 15 optional reads, zero invoked fallbacks and zero
compatibility bindings. It emitted
`build/audit-system-font-smoke-20260820.png`, SHA-256
`a318b4699df2a40259eaaccd415e171ff978a9468e5621e1bd05a50900f930c7`.
The image was deliberately not viewed or compared. It proves only that the
live SystemFont/wgpu path executed; still images, including original-game
captures without precisely matching runtime state and time, remain
non-authoritative for layout or fidelity.

### LabelPool five-mebibyte FIFO and immediate-to-deferred texture ownership

The next pass again used no screenshot as acceptance evidence. IDA's
decompilation of `game::LabelPool::addLabel` at `0x100476AEC` and Hopper's
independent pseudocode agree on the complete capacity path. The label texture
reports its format through virtual slot `+112`, width through `+88` and height
through `+96`; `sub_1004DE110` converts those values to storage bytes. Format
`6` is the four-channel `A8B8G8R8` surface, so every cached SystemFont label is
accounted as `width * height * 4` bytes. The exact limit is `0x500000`
(5,242,880 bytes). A total equal to the limit is accepted. A total of at least
`0x500001` repeatedly evicts entries until the new label fits.

The pool is insertion-order FIFO, not LRU. Its auxiliary vector inserts every
new hash at `begin()` (using `_M_insert_aux(begin, value)` when nonempty), while
the capacity loop selects `end() - 1`; the red-black tree is then erased by
that oldest hash and the evicted texture's actual byte size is subtracted.
`drawString` performs its tree lookup directly and does not call `addLabel` on
a hit, so a hit cannot reorder the vector. The tree key remains only the
recovered 64-bit hash, preserving native collision behavior.

IDA and Hopper also expose the collision/layout ordering in
`SystemFont::Impl::drawString` at `0x100475B98`. Stroke and the requested
vertical metrics are subtracted before the pool lookup. RIGHT or HCENTER calls
`getStringWidth` for the current substring before that lookup. On a hit, the
virtual draw instead reads width and height from the cached Label object.
Consequently a hash collision anchors using the current string but draws the
first cached label's texture dimensions; neither the complete current layout
nor the complete cached layout may be reused as one unit.

`AssetCatalog` now carries this exact logical pool: one active destructor
epoch, a hash map, a newest-first deque, the native byte counter and a
monotonic host texture identity. Hits do not mutate insertion order. Misses
accept the exact 5 MiB boundary, evict from the deque tail and assign a fresh
physical key when an evicted hash is later inserted again. Entering another
last-instance epoch clears all logical entries and bytes, matching the native
SystemFont destructor.

wgpu requires one additional ownership layer because Purple consumes each GL
draw immediately, whereas this host uploads after the complete frame has been
prepared. Every prepared frame therefore retains `Arc<TextureAsset>` snapshots
for the physical label identities it references. Logical FIFO eviction marks a
texture retired but cannot remove it while an earlier draw in the same frame
still requires it. The renderer defers GPU texture and bind-group destruction
until a later frame no longer references that identity. This host-only
snapshot does not enlarge or reorder the emulated LabelPool; it prevents the
deferred backend from changing an already submitted native draw.

Regressions cover the exact byte boundary, FIFO selection after a recent hit,
non-reordering cache hits, oversized-label rejection, epoch clears,
same-hash re-insertion identity, cached dimension ownership and actual
headless-wgpu retention/removal across two frames. The complete workspace now
passes 410 tests (56 app/audio/wgpu, 19 assets, one core and 334
script/physics); formatting and strict all-target/all-feature Clippy are clean.
The optimized executable SHA-256 is
`25136eb2d8ddb84b106617d58ae1c5981187a94138f16523071df278fa305e65`.

A fresh isolated-AppData, no-Android 600-frame execution of the shipped iOS
Lua bytecode again completed with 67 optional reads, zero invoked fallbacks
and zero compatibility bindings. No screenshot was generated or inspected in
this pass. In accordance with `docs/visual-reference-policy.md`, even a
matching still would not prove these cache, collision or ownership semantics.

### System-font enumeration order and one-time cache

`game::SystemFont::Impl::getAvailableFontNames` at `0x100475950` exposes a
separate nonvisual ordering contract. IDA and Hopper independently show that
the function populates its static vector only while `begin == end`. It obtains
`+[UIFont familyNames]`, walks that NSArray in returned order, obtains
`+[UIFont fontNamesForFamilyName:]` for each family, and appends every UTF-8
PostScript name in the nested array order. There is no native sort and no
deduplication. Later calls return the already populated vector. The Lua bridge
at `0x1004482D0` copies it into a fresh table at consecutive one-based indices.

The previous host flattened all font faces, globally sorted their names and
deduplicated them. The replacement now mirrors the two-level traversal over
the platform font database: unique families retain first-returned order, each
family contributes its faces in database order, duplicate face names remain,
and the existing `OnceLock` supplies the recovered process-lifetime cache.
The platform's actual installed font inventory remains platform-dependent, as
it is for UIKit, but the binary-defined ordering and duplication semantics no
longer change on the Rust side.

The released desktop host keeps that strict behavior for arbitrary missing
font names, but the shipped scripts also force the `ios` resource profile on
every rehost platform and consequently request the iOS-only PostScript face
`ArialRoundedMTBold`. Purple's own Windows profile selects Arial instead.
When that one Apple face is absent, the cross-platform host therefore resolves
it to the platform's bold sans-serif face (Arial first on Windows) while
retaining the requested family string in the SystemFont binding and LabelPool
key. An installed `ArialRoundedMTBold` still wins, and every other unavailable
font continues to raise the recovered native error.

A deterministic helper regression uses interleaved families and a repeated
PostScript name to prove family grouping, face order and duplicate retention.
The complete workspace passes 411 tests (56 app/audio/wgpu, 19 assets, one
core and 335 script/physics); formatting and strict all-target/all-feature
Clippy are clean. The current optimized executable SHA-256 is
`ce66e73de5328978ee5ae91bbdf44bc01eb2369f331e464619c52dcb82cad228`.
A new isolated-AppData 600-frame run again reports 67 optional reads, zero
invoked fallbacks and zero compatibility bindings. No image was produced or
consulted.

### SystemFont getBounds ABI and corrected TOP/BASELINE anchoring

IDA and Hopper independently confirm the complete
`game::SystemFont::Impl::getBounds` implementation at `0x10047664C`. The
IFont vtable selects it at slot `+104`. Its packed `Anchor` argument stores the
vertical enum in the low word and the horizontal enum in the high word. The
string is measured through the same substring width and height virtuals first;
LEFT uses zero, HCENTER multiplies the already integer width by float32 `-0.5`,
and RIGHT negates it. Each of the four final edges is then converted separately
with `FCVTZS`, so an odd centered width deliberately need not retain its source
integer span after both boundaries truncate toward zero.

The anchor parser at `sub_10040A248` assigns TOP=0, VCENTER=1, BOTTOM=2,
BASELINE=3 and VPIVOT=4. The low-word comparisons in `getBounds` and
`drawString` therefore subtract the ascender for BASELINE=3, not TOP. BOTTOM
subtracts ascender plus descender, VCENTER subtracts their integer half, and
TOP/VPIVOT take the unshifted default branch. This corrects the prior rehost
interpretation, which had exchanged TOP and BASELINE and could place identical
text differently depending on whether its LabelPool entry was created or
reused.

`SystemFontRenderBinding` now owns the shared cross-platform width, height and
bounds calculations. The public font-width query, label allocation and
cache-hit anchor calculation use that same measurement boundary. Bounds ranges
are selected after UTF-8 is converted to Unicode codepoints, matching the
native UTF-32 substring: a negative start fails, a positive start beyond the
end is clamped to the end, and a negative signed count becomes a size_t-sized
suffix request. An empty source bypasses range validation entirely and both
measurement wrappers return zero.
The stored stroke width expands all four bounds before their independent
float32-to-int conversions.

Regressions cover TOP versus BASELINE on both LabelPool miss and hit, BOTTOM,
VPIVOT, an odd-width HCENTER rectangle, stroke expansion, UTF-32 indexing over
multibyte text, negative counts and invalid starts. The complete workspace now
passes 413 tests (56 app/audio/wgpu, 19 assets, one core and 337
script/physics); formatting and strict all-target/all-feature Clippy are clean.
The optimized executable SHA-256 is
`0122b8738d8b2bf8c5d26dd2b07351b2a9a6d75c9cb4ecd9a27ee80de4944490`.

A fresh isolated-AppData, no-Android 600-frame execution of the shipped iOS
Lua bytecode completed with 67 optional reads, zero invoked fallbacks and zero
compatibility bindings. No screenshot was generated or inspected. In
particular, neither a matching nor a differing still is accepted as evidence
for these enum, substring, cache or rounding contracts.

### SystemFont CGFloat metrics and byte-wrapper range normalization

A follow-up constructor audit distinguishes two conversion domains that the
earlier host had collapsed. Lua's numeric size, stroke and style slots first
narrow to float32 and use `FCVTZS S`, but `UIFont` returns its ascender,
descender and leading as ARM64 `CGFloat` doubles. IDA shows direct
`FCVTZS W8,D0` at `0x100477750`, `0x100477770` and `0x10047778C`; Hopper
independently shows the same three D-register conversions. The Rust constructor
previously narrowed those values to f32 first, which can round a value such as
36.9999999 across the integer boundary before truncation. Constructor metrics
and the retained label-height measurement now convert directly from f64, while
Lua argument conversion deliberately remains f32.

The IFont height getter at `0x1004758D4` loads ascender and descender as W
registers and uses `ADD W0,W8,W9`. `drawString` uses the same wrapping W add
before its signed, toward-zero half calculation. The public height metric and
both LabelPool hit/miss vertical-anchor branches now share that exact wrapping
addition instead of Rust debug-overflow behavior or release-only wrapping.
The constructor still calls `fontWithName:size:` before rejecting a nonzero
style, so an unavailable face is reported before an unsupported style; a
focused regression preserves this failure order.

The byte-string width and height wrappers at `0x100476424` and `0x100476538`
also contain range behavior that is absent from the wide overloads. For a
nonempty source they convert UTF-8 to UTF-32, clamp a positive start greater
than the codepoint length to the end with signed `CMP/CSEL`, and clamp count
using a wrapping 32-bit start-plus-count comparison. Negative start remains
negative and fails when sign-extended into `basic_string::substr`; negative
count becomes a large size_t request and selects the suffix. For an empty
source both wrappers return zero before conversion or range checks, so even
otherwise invalid start/count values produce a stroke-only bounds rectangle.
`native_string_bounds` now reproduces this wrapper normalization rather than
calling the wide-substring model directly.

New regressions distinguish f64 and f32 conversion on both signs, cover ARM
integer-indefinite overflow, constructor error priority, W-add overflow,
signed half-height, positive overrun clamping, negative start/count, huge
wrapping counts and empty-source BASELINE bounds. The complete workspace now
passes 418 tests (57 app/audio/wgpu, 19 assets, one core and 341
script/physics); formatting and strict all-target/all-feature Clippy are clean.
The optimized executable SHA-256 is
`6c70376d9e7c691306d4e668f0b648a7a475f74f7aefafeac3a5386aa0037619`.

A fresh isolated-AppData, no-Android 600-frame run of the shipped iOS Lua again
completed with 67 optional reads, zero invoked fallbacks and zero compatibility
bindings. No screenshot was generated or inspected, since no still image can
prove these precision, overflow, exception-order or substring contracts.

### BitmapFont v2 UTF-32 records, signed pivots and IFont wrapping arithmetic

This pass deliberately treated screenshots as non-authoritative. IDA and
Hopper independently recover the BitmapFont constructor at `0x10042A5B0`, its
loader at `0x10042A780` and the vtable installed from `0x100AA4320`. The loader
accepts FONT versions 1 and 2, not only the v1 files shipped in this bundle.
Version 1 reads a 16-bit character value and explicitly masks it with
`AND W27,W27,#0xffff`; version 2 reads the glyph key with the 32-bit reader at
`0x10042ABA4`. The remaining five atlas fields keep their signed 16-bit layout.
The common reader at `0x1004FACCC` returns with `LDRSH`, so x, y, width, height
and the vertical pivot all sign-extend. The former Rust model restricted every
key to `u16`, treated four atlas fields as unsigned and treated the pivot as
unsigned, which made the native v2 supplementary-plane path unrepresentable
and could turn negative atlas geometry or a negative baseline into a very
large positive value.

The same vtable closes the bitmap metric rules. Slot `+48` is the width virtual
at `0x10042BA8C`, slot `+56` is the per-string height virtual at
`0x10042BC54`, slots `+64` through `+96` return height, ascender, descender,
leading and tracking, and slot `+104` is `getBounds` at `0x10042BE4C`. Width
walks every requested codepoint, adds a glyph width only when that key exists,
then executes `MADD W...` with tracking times requested character count minus
one. Missing glyphs therefore still participate in the tracking count even
though they contribute neither a quad nor a cursor advance. All sums and the
final multiply-add wrap in 32-bit W lanes. String height is the largest glyph
height in the requested substring, while the public font height is the
constructor-cached ascender plus descender with the same W-register wrap.

BitmapFont anchor and bounds arithmetic is now shared by the asset parser,
script queries, test renderer and live wgpu expander. RIGHT and HCENTER use the
wrapped string width; HCENTER applies an arithmetic signed half. TOP uses the
cached ascender, VCENTER subtracts the arithmetic half of wrapped ascender plus
descender, BOTTOM negates descender, and BASELINE/VPIVOT take the zero default.
`getBounds` additionally subtracts the largest nonnegative pivot in the
selected substring and uses that substring's tallest glyph. The rendering
path continues to advance only after a found glyph, matching
`BitmapFont::draw` at `0x10042B338`.

`FontGlyph` now retains a `u32` key and signed pivot, the parser accepts both
native FONT versions, and all consumers use one codepoint lookup and wrapping
metric implementation. Regressions cover a v2 U+1F600 glyph with a negative
pivot, missing-glyph tracking, negative-pivot ascender/descender behavior,
32-bit width overflow and an actual UTF-32 glyph reaching wgpu quad
submission. The complete workspace passes 421 tests (58 app/audio/wgpu, 21
assets, one core and 341 script/physics); formatting and strict
all-target/all-feature Clippy are clean. The optimized executable SHA-256 is
`6ccc1d4f73dd6af804c766fcb19d9c94888204eb2c548e2c2812b2535369a499`.

A fresh isolated-AppData, no-Android 600-frame run of the shipped iOS Lua
completed with 67 optional reads, zero invoked fallbacks and zero compatibility
bindings. No screenshot was generated or inspected. This validation closes
only the instruction-level format, metric, anchor and submission contracts;
it makes no visual-fidelity claim from a still frame.

### Signed AtlasSprite geometry, JSON narrowing and rotated UV cache

The shared AtlasSprite audit extends the signed FONT finding to ordinary SPRT
records. IDA and Hopper independently show six consecutive calls to
`sub_1004FACCC` in the binary SPRT loader at `0x1004610F0`. The reader ends in
`LDRSH W0`, and `Sprite::Sprite` at `0x100467760` stores x, y, width, height,
pivot X and pivot Y with `STRH` at object offsets `+0x28` through `+0x32`.
The width, height and pivot getters at `0x100467E14` through `0x100467E2C`
load those members with `LDRSH`. Consequently unsigned Rust x/y/width/height
were not merely a parser interpretation; they contradicted the runtime object
ABI used by queries, bounds and draw geometry.

The JSON SpriteSheet path at `0x1004637FC` has a separate pre-constructor
boundary. `sub_10055AE64` converts parser integers with `SCVTF D0,X8` while
retaining their signed 64-bit value, and converts parser doubles to that same
integer view with `FCVTZS X8,D0`. Frame x/y/w/h are then read through
`sub_10055D93C`, which returns the low W word from the 64-bit member at map-node
offset `+0xA0`. Default pivots are calculated from those signed W values with
the native add-sign-bit and arithmetic-shift sequence, which implements
division by two toward zero.
Adobe/ArtPacker explicit pivots instead use the float accessor
`sub_10055DE7C`, add float32 0.5, execute `FRINTM`, then `FCVTZS`. Only after
those calculations does the Sprite constructor truncate the six arguments to
their signed 16-bit members. The Rust loader now preserves that order, so a
negative odd dimension produces the native signed half and a negative
explicit pivot follows the recovered floor-after-half rule.

TexturePacker's `rotated` field was previously type-checked and discarded.
The binary instead requires it through the bool accessor `sub_10055D694` and
passes it as the final argument to `sub_10046AF70`. `Sprite::Sprite` uses that
argument to cache four UV pairs at offsets `+0x34`, `+0x3C`, `+0x44` and
`+0x4C`: value 1 swaps the width/height atlas extents and rotates the corner
order, values 2 and 3 select the other two permutations, and every other value
uses the ordinary order. `SpriteRegion` now retains this constructor state and
owns one shared native-corner/UV implementation used by ordinary wgpu sprites,
native four-point quads and the test-only software sampler. The software
sampler also accepts signed reversed rectangles instead of silently drawing
nothing for negative dimensions.

Numeric regressions cover signed binary SPRT and FONT fields, all four native
UV branches, signed odd JSON dimensions, low-W truncation of a wide parser
double, required TexturePacker rotation, Adobe negative-pivot rounding and a
signed rotated atlas region reaching the wgpu vertex stream. The complete
workspace passes 422 tests (58
app/audio/wgpu, 22 assets, one core and 341 script/physics); formatting and
strict all-target/all-feature Clippy are clean. The optimized executable
SHA-256 is
`23ddb4a50782d0067b8203bab23bb2b5d48c26e9ee614f5abf246834ddb6d5f8`.

A fresh isolated-AppData, no-Android 600-frame run of the shipped iOS Lua
completed with 67 optional reads, zero invoked fallbacks and zero compatibility
bindings. No screenshot was generated or inspected. In particular, these
signed-field, truncation and UV-permutation claims are accepted only from the
two disassemblers and numeric/runtime assertions, not from apparent visual
agreement.

### Composite Entry identity, float flips and load-time radian conversion

This correction did not use screenshot pixels. It came from the connected
IDA Pro and Hopper databases plus the shipped COMP byte streams.
Both disassemblers show that the JSON composite loader is
`sub_1004657CC`, the detailed Entry constructor is `sub_1004370A4`, the
binary loader is `sub_100461B98`, and the simple KA3D Entry constructor is
`sub_100436B70`.

`sub_1004370A4` makes the runtime layout explicit. Entry `+0x18` is its map
name, `+0x20` is the retained AtlasSprite pointer, `+0x28/+0x2c` are x/y,
`+0x30/+0x34` are scale x/y, `+0x38/+0x3c` are flip x/y, `+0x40` is angle and
`+0x44` is visibility. A non-empty id is not a z order: the constructor forms
`"{name}#{id}"` for the vector/name map while retaining the AtlasSprite
resolved from the unsuffixed name. The JSON loader tests `id` with the exact
string predicate `sub_10055CE68`; an absent or non-string id is ignored.

JSON `scale` defaults to `[1,1]`, accepts either a scalar copied to both axes
or a two-number array, and is narrowed through the float accessor.
JSON `flip` also defaults to `[1,1]`, but when present it is unconditionally
indexed as a two-number array. It is neither a boolean nor an integer bit
field, and arbitrary float32 multipliers reach Entry unchanged. `angle`
defaults to zero; a present number is narrowed to float32 and multiplied by
the native degrees-to-radians constant before the constructor call. The
reverse traversal of each JSON `sprites` array remains intact.

The RVIO branch of `sub_100461B98` has the same Entry contract. Each record is
sprite string, id string, two signed 16-bit offsets, two float32 scales, a
float32 degree angle and two byte flags. Each flag is converted independently
to exactly `-1.0` or `+1.0`, and the angle is converted before storage. The
KA3D branch is selected from the container tag rather than merely from the
payload version: versions 1 and 2 use name/x/y records with unit scale/flip
and zero angle. Version 2 then consumes a counted per-composite metadata list
of string/u16/u16 records. The former Rust parser happened to accept shipped
RVIO entries only while their ids were empty, because it interpreted the
empty string length as a fictitious i16 z field.

`CompositePart` now mirrors the live Entry fields: the unused `z_order` and
raw `flags` approximations are gone, `sprite` retains `#id`, flip values remain
float32 and angle is always radians. Lua `getCompoSpriteEntry` follows the
native `flip < 0` tests at `sub_1004497D8`; the setter at `sub_100449CFC`
writes exactly `-1` or `+1` and stores the supplied angle directly. Bounds,
the reference renderer, wgpu composite expansion and the shader helper now
consume radians and flip multipliers without a second unit conversion.

Regressions cover RVIO suffix construction, byte-to-float flips, load-time
angle conversion, KA3D v2 metadata consumption, JSON scalar/array scale,
required flip arrays, non-string id fallback, reverse part order, Lua flip
mutation and radian affine composition. All 19 shipped COMP files parse under
the corrected split: eight RVIO and eleven KA3D, with zero failures. The full
workspace passes 423 tests (58 app/audio/wgpu, 22 assets, one core and 342
script/physics); formatting and strict all-target/all-feature Clippy are
clean. The optimized executable SHA-256 is
`fc72e9c63f76ecb96b8d66beb940011db452ce09f07fb77aab14a75026e561ef`.

A fresh isolated-AppData 16,500-frame no-input run of the shipped iOS Lua
completed with
67 optional reads, zero invoked fallbacks and zero compatibility bindings.
No screenshot was inspected or retained. One temporary offscreen output was
created solely to force the wgpu resource-expansion path, then deleted without
reading its pixels; that pass reported zero sprite/font/texture/quad resolution
misses. The acceptance evidence is the dual disassembly, exact data-layout
tests, complete bundle parse and runtime counters, not apparent agreement with
any still frame.

### Native physical chunk scanning, map replacement and legacy TEXT offsets

This pass again excludes screenshot pixels from its evidence. The correction
comes from IDA Pro and Hopper control flow, the shipped binary resources and
numeric parser/runtime assertions. The relevant native loaders are SPRT at
`sub_1004610F0`, FONT at `sub_10042A780`, COMP at `sub_100461B98`, the TEXT
locale pass at `sub_1004729A4` and the TEXT group pass at `sub_1004731B0`.
The common big-endian readers are `sub_1004FAC04`/`sub_1004FACCC`, strings are
read by `sub_1004FAD94`, and unknown chunks are advanced by
`sub_1004FA444`.

All four KA3D loaders compare the declared root length only with the physical
remaining-byte count. They do not require equality and do not create a child
slice bounded by that length. Their outer loops continue while any physical
byte remains, read a tag and length, parse recognized tags directly from the
same stream, and use the declared length only for an unrecognized tag. A
recognized but unsupported version consumes the version field and resumes the
same physical loop; it is not silently skipped to the declared block end. The
new shared Rust reader preserves these boundaries with checked offset
arithmetic and produces truncation errors at the same header/skip operations.
`Ka3dEnvelope` remains a bounded inspection view, now treating lengths as
upper bounds and offering tag lookup for host-side directory catalogs.

SPRT's first `u16` is version 1, not a texture count. A supported block then
reads exactly one texture string, a sprite count and the records. Every block
replaces the native sheet's current texture before constructing its sprites;
sprites retain that construction-time texture pointer, and duplicate names
replace the prior `std::map` value. `SpriteSheet` now records the per-sprite
texture index, so a multi-block file no longer routes every region to the
first texture. Composite names likewise use the last parsed map value.

FONT accepts versions 1 and 2 in every recognized block. Later blocks replace
texture, leading and tracking. Duplicate glyph keys replace lookup results,
but the constructor's cached maximum ascender/descender has already observed
the earlier record. The retained glyph history plus reverse lookup now models
both facts rather than losing either the last value or the cached metric.

KA3D TEXT is intentionally not a strict one-pass `LDAT`, `LIDS`, `TXGP`
slice parser. The locale loader scans nested chunks and replaces the locale
vector on `LDAT`. Loading a language reopens the file, rebuilds `LIDS`, skips
earlier `TXGP` payloads by their declared lengths, and reads the selected group
only after confirming that LIDS is nonempty. For a non-KA3D first word, Purple
rewinds, consumes a legacy version byte, a locale-section length, the signed
byte locale count and names; the group pass skips that section, reads IDs,
indexes a u32 forward-offset table and then reads the selected values. Both
branches are now implemented and have order, offset and missing-LIDS tests.

A complete inspection of all 114 shipped containers succeeds: 70 SPRT, 19
COMP, 24 FONT and one TEXT, with zero subtype failures. The workspace passes
431 tests (58 app/audio/wgpu, 30 assets, one core and 342 script/physics), and
formatting plus strict all-target/all-feature Clippy are clean. The optimized
`stella-app` SHA-256 is
`5a930daf86fab477d9e5afa12bfab0e9eece9c16b642f813f46cfeb408765d5a`.

A fresh isolated-AppData 600-frame no-input headless run completed with 67
optional reads, zero invoked fallbacks and zero compatibility bindings. A
separate 600-frame offscreen wgpu drive reached 103 sprite submissions without
any sprite/font/quad resolution diagnostic. Its temporary PNG was moved to
Trash without being opened or inspected. These runs establish loader and
submission health only; neither the generated image nor any existing
screenshot is accepted as proof of visual equivalence.

### UIKit shared shaping boundary and retained glyph placement

This pass follows the evidence policy prompted by the remaining differences
between screenshots and the running game: no screenshot pixel, crop or visual
similarity was used to select the implementation. IDA and Hopper independently
show that the UTF-32 width overload at `0x10047692C` and height overload at
`0x10047676C` convert the selected codepoint range back to NSString and call
`sizeWithFont:`. On a LabelPool miss, `SystemFont::Impl::drawString` at
`0x100475B98` sends the same NSString and retained UIFont to
`drawInRect:withFont:`. Measurement and drawing therefore share UIKit's text
layout result; the executable contains no per-character glyph-advance loop in
either path.

The previous cross-platform implementation violated that ownership boundary:
`native_string_width` walked Unicode scalar values with cmap, horizontal
advance and legacy `kern` tables, while the rasterizer independently repeated
another character loop through `ab_glyph`. Besides duplicated rounding, that
omitted GSUB ligatures, GPOS kerning/mark attachment and contextual shaping,
so measured geometry could disagree with the glyph sequence actually placed
in the cached label.

`SystemFontRenderBinding` now performs one Rustybuzz OpenType shaping pass per
line and retains the selected glyph IDs plus their pen-relative X/Y positions
in font units. The width path scales the shaped total in f64 and applies the
same direct `FCVTZS` boundary already recovered from the ARM64 overload. The
wgpu label rasterizer consumes those exact retained glyph IDs and positions,
including mark offsets, rather than looking characters up again. Thus the
cross-platform implementation has the same measurement/draw data dependency
as the UIKit code even though platform raster coverage remains backend-owned.

A deterministic bundle-font regression proves that OpenSans `ffi` collapses
through GSUB and that decomposed `e` plus combining acute uses one positioned
glyph; a multiline case proves width and raster lines consume the same layout.
On the current macOS host, the installed `ArialRoundedMTBold` face also retains
the previously intercepted iOS boot contract: point size 40 measures `Stella`
as 110. That assertion is conditional on the named face being installed,
matching the native `UIFont fontWithName:size:` failure dependency rather than
silently substituting a screenshot-derived face.

The workspace now carries Rustybuzz only in the script/layout layer; the app
receives a small renderer-neutral shaped-line structure, preserving the split
between native-style text semantics and wgpu texture upload. All 433 tests pass
(58 app/audio/wgpu, 30 assets, one core and 344 script/physics), formatting is
clean, and strict all-target/all-feature Clippy is warning-free. A complete
read-only bundle verification covers 2,602 files, including 1,167 encrypted
archives, 718 Lua chunks, 448 JSON documents, 114 KA3D envelopes and 72 PVR
textures. The optimized hashes are
`71699ee0611700a9657fbc5df2924f4e882084f2228c56fd57d24ca99a3ab359`
for `stella-app`,
`3b1636ea301d2c1eb2b399ab708a7372b5a6c1e7bb919e64bd92008526837aa0`
for `stella-headless`, and
`2ff97ac0f8c6cf24fe816cca590024cffd2d728438250faaf2189403339e22dc`
for `stella-tool`.

Fresh isolated-AppData headless and wgpu runs both complete 600 frames with 67
optional data reads, zero invoked fallbacks and zero compatibility bindings.
The temporary offscreen PNG existed only to force the wgpu resource path; the
containing audit directory was moved to Trash without opening or inspecting
the image. It is recoverable there, but it remains neither evidence nor an
accepted visual reference.

### Target-endian Lua 5.1 chunk ABI completion

The next cross-platform audit found one explicit host gap unrelated to visual
output. Every shipped Purple Lua chunk declares format 0, little-endian scalar
storage, four-byte `int`, `size_t`, instruction and `lua_Number`, with the
number-integral flag clear. Purple's ARM64 Lua build consumes that custom
single-precision representation, while the vendored host Lua uses its native
endianness, native `size_t` and eight-byte `lua_Number`. The existing
transcoder widened strings and numbers only for a little-endian host and
returned `big-endian runtime transcoding is not implemented` otherwise.

The transcoder now has one explicit target-ABI descriptor. It writes the Lua
header's endian and `size_t` slots for the target, widens every f32 numeric
constant to f64, and emits all scalar fields in target order: function line
ranges, counts, VM instructions, nested-prototype metadata, line tables and
local-variable PC ranges. String payload bytes and the four one-byte function
flags remain unchanged. Four- and eight-byte target `size_t` encodings are
both supported; all other target layouts are rejected before traversal.

A synthetic prototype regression uses asymmetric byte patterns for both line
fields, one instruction, one floating constant, line information and local PC
ranges, then verifies every exact offset in an eight-byte big-endian output
chunk. This closes the dormant architecture failure without relying on a
big-endian CI machine and without generating or inspecting any image. The
complete workspace now passes 434 tests (58 app/audio/wgpu, 31 assets, one
core and 344 script/physics), and strict workspace Clippy plus formatting are
clean. The release hashes are
`2fa9e6913f85d30257d1df7f500d5d8bf7e37948fdff126fc47d94682da9642f`
for `stella-app`,
`1d3c0c771f39f7cf95577384bfa6c543fb8548d31050d1c117a50ce691fd0b98`
for `stella-headless`, and
`0314c628d39540e32c143dc288cbf4d9eaa64b031688a6c606c3291b2f77bdbf`
for `stella-tool`. Read-only verification still classifies all 2,602 bundle
files successfully, and a fresh isolated-AppData 600-frame boot retains the
67 optional reads, zero invoked fallbacks and zero compatibility bindings.

### CoreGraphics vector stroke semantics for SystemFont labels

This pass again excludes screenshots from the correctness argument. IDA and
Hopper independently show the exact stroked branch of
`SystemFont::Impl::drawString` at `0x100475B98`. After creating the bitmap
context at `0x100475F68`--`0x100475F90`, Purple sets the fill color at
`0x100475FE0`, translates the CTM by `(0, height)` and scales it by `(1, -1)`
at `0x100475FF4`--`0x100476004`. A stroke width below one bypasses the branch
and draws the NSString once in fill mode at `0x100476194`--`0x1004761B8`.

For a positive integral stroke width, Purple translates by `(stroke, stroke)`
at `0x10047601C`--`0x10047602C`, installs the stroke color as both CGContext
stroke and fill color, sets the line width to exactly `2 * stroke` at
`0x1004760C0`--`0x1004760D0`, and selects line-join value one (round) at
`0x1004760D4`--`0x1004760DC`. It then selects text drawing mode one (stroke),
draws the same NSString at `0x100476118`, restores the label fill color and
text drawing mode zero, and draws the NSString again at `0x10047618C`.
Consequently the native effect is a centered vector outline rendered before
the fill, not a repeated bitmap offset or a post-raster morphological dilation.

The wgpu resource path now converts the already retained Rustybuzz glyph IDs
and positions into closed TrueType/OpenType paths, applies the same font-unit
to logical-pixel transform, fills those paths with nonzero winding, and builds
the outline with width `2 * stroke` plus round joins. Stroke coverage is
composited first and fill coverage second in premultiplied RGBA, matching the
recovered draw order. The former discrete circular dilation has been removed;
measurement and rendering still consume the same shaped-line structure, so
the stroke change introduces no second text-layout implementation.

A bundle OpenSans regression confirms that a closed `M` outline stroked with a
six-pixel line expands its tight vector bounds by the expected three pixels on
all four sides. This is a geometry assertion over the recovered graphics
contract, not a screenshot comparison. The complete workspace passes 435
tests (59 app/audio/wgpu, 31 assets, one core and 344 script/physics), and
formatting plus strict all-target/all-feature Clippy remain clean.

The optimized hashes for this pass are
`89d2fbeee46669444010a56945afbf3f343a1cc9d856dd065cda9115286d8847`
for `stella-app`,
`672b8862cb2c66e8fcbc9efa6348ec1dd0e48b9a924e16e842a3b783c5cffb7a`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for `stella-tool`. Read-only bundle verification still classifies all 2,602
files successfully. Fresh isolated-AppData headless and offscreen-wgpu runs
both complete 600 frames with the same 67 optional global reads, zero invoked
fallbacks and zero remaining compatibility bindings. The offscreen PNG was
only checked for existence, never opened; both audit directories and their new
AppData were moved to Trash and remain recoverable.

### SystemFont Cocoa line boundaries and empty-range height

The next nonvisual audit separated three height contracts that had previously
been collapsed. IDA and Hopper show the byte-string height overload at
`0x100476538` loading the source length and returning zero at
`0x100476560`/`0x1004765E4` when the original source is empty. For a nonempty
source, `0x100476574`--`0x100476590` clamps the start and count in signed
32-bit arithmetic after UTF-8-to-UTF-32 conversion, then calls the wide
overload. The wide overload at `0x10047676C` always constructs the selected
substring, including an empty selected range, sends `sizeWithFont:` at
`0x1004767F0`, and converts the returned height with `FCVTZS W0,D8` at
`0x100476810`. `getBounds` at `0x10047664C` calls these byte-range width and
height overloads independently before applying its anchor and stroke edges.

Consequently an empty source measures zero because the outer wrapper exits,
whereas a nonempty source whose normalized range is empty still enters UIKit
and retains one empty text line. The Rust bounds path now preserves that
distinction. The existing empty-source rule still ignores otherwise invalid
range arguments, while `"A"` with `(start=1,count=0)` or an overlarge positive
start returns zero width but one label-line height before stroke expansion.

The old shared shaper also recognized only LF. A local Cocoa/CoreText API probe
using the installed `ArialRoundedMTBold` face (no image output) confirms that
LF, form feed, CR, NEL, U+2028 and U+2029 each terminate a line, CRLF is one
separator, and vertical tab is not a separator. The retained shaping pass and
wgpu path now consume the same splitter, so separators never become accidental
missing-glyph advances and the measured line count cannot disagree with glyph
placement. Consecutive and trailing separators retain their empty line.

The same API probe provides an independent numeric check on the earlier iOS
runtime interception: at point size 40 the current Apple face reports
ascender `37.8515625`, descent `8.4375`, leading zero, `Stella` width
`110.05859375`, and NSString height exactly `46`. Purple's direct conversions
therefore produce the intercepted constructor values `37/8/0`, width `110`,
and the separately retained label height `46`; adding the already-truncated
constructor metrics would incorrectly yield `45`. The conditional installed
face regression now asserts all five boundaries rather than width alone.

Regressions cover every Cocoa separator, CRLF coalescing, the vertical-tab
nonseparator, empty source versus empty selected range, and wgpu allocation
for each two-line spelling. The complete workspace passes 437 tests (59
app/audio/wgpu, 31 assets, one core and 346 script/physics), and formatting
plus strict all-target/all-feature Clippy are clean. No screenshot or rendered
pixel was produced, opened or used to select these behaviors.

The optimized hashes for this pass are
`b992fde298450e290cc418df061f153fc0ee6d4a59136a6442cb57de61cb9dd2`
for `stella-app`,
`9e7aa029d35bcf53a56d074571fcad31f98d7767a52670ceb5fe17e46d4a5994`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`. Read-only verification again classifies all
2,602 bundle files. Fresh isolated-AppData headless and offscreen-wgpu drives
both complete 600 frames with 67 optional global reads, zero invoked fallbacks
and zero remaining compatibility bindings. The generated PNG was checked only
for existence and size; both temporary trees were moved to Trash without
opening it and remain recoverable.

### SystemFont UAX #9 visual runs and Unicode Script itemization

This pass again treats images as neither an oracle nor a comparison target.
IDA's Hex-Rays output and Hopper's independent assembly/pseudocode agree that
the UTF-32 width overload at `0x10047692C` constructs one NSString and sends it
directly to `sizeWithFont:` before `FCVTZS W0,D8`. The LabelPool miss path in
`SystemFont::Impl::drawString` at `0x100475B98` sends that same complete
NSString to `drawInRect:withFont:` at `0x100476118`/`0x10047618C`. Purple has no
game-owned character reversal or per-script draw loop; bidi paragraph
resolution, script itemization and OpenType shaping are therefore all owned by
the UIKit/CoreText boundary.

Rustybuzz correctly guesses properties for a single-script buffer, but it is a
font shaper rather than a paragraph-level bidi engine. The prior rehost passed
each complete line as one buffer, so a mixed LTR/RTL line inherited the script
and direction of its first strong character. A non-image CoreText probe makes
the discrepancy observable without visual judgement. With the installed
`ArialUnicodeMS` face at 40 points, `"abc אבג 123"` produces visual CTRuns in
Latin, digits, then RTL-Hebrew order, glyph IDs
`68,69,70,3,20,21,22,3,1156,1155,1154`, and width `215.5078125`, which Purple
converts to 215. `"abc مرحبا 123"` applies Arabic contextual substitutions
`6510,6514,6531,6542,6595` in its final RTL run and truncates width
`236.62109375` to 236. Parenthesized, RTL-base and number-leading probes expose
the same run ordering and mirrored-punctuation decisions through run ranges,
glyph IDs and advances alone.

`SystemFontRenderBinding` now runs Unicode Bidirectional Algorithm paragraph
resolution for every Cocoa line, consumes the returned level runs in visual
left-to-right order, and shapes each run with an explicit LTR or RTL direction.
Each directional run is further itemized by Unicode Script; Common, Inherited
and Unknown characters stay attached to a neighbouring concrete script, and
script subruns inside an RTL level run are visited in reverse visual order.
Rustybuzz still performs the actual GSUB/GPOS and bidi-mirroring work, while the
existing shared shaped-glyph payload continues to feed both measurement and
the wgpu vector-outline rasterizer.

The new paragraph itemizer is isolated in
`render_types/system_font_layout.rs` rather than enlarging the renderer-neutral
command ABI. Deterministic bundle-font tests cover byte-indexed UAX #9 ranges,
visual glyph order despite a deliberately missing Hebrew cmap, neutral/script
attachment and combining marks. A conditional Apple-font regression compares
five mixed-direction spellings to the CoreText numeric probe, including exact
glyph IDs, font-unit X positions and native-truncated widths. This closes
directional and script segmentation for retained faces. UIKit's separate
system-font fallback selection when the requested face lacks a glyph remains
an explicit next boundary; substituting the first host font with a matching
cmap would not be evidence-equivalent and was deliberately not smuggled into
this change.

The complete workspace passes 440 tests (59 app/audio/wgpu, 31 assets, one
core and 349 script/physics). Formatting and strict all-target/all-feature
Clippy are clean. Read-only bundle verification still classifies all 2,602
files: 1,167 encrypted archives, 718 Lua chunks, 448 JSON documents, 114 KA3D
envelopes and 72 PVR textures. Release SHA-256 values are
`0ceb22782b612af28f1d48a28b4ba1fbadba5ba72f7f436bcd76884cba95bd71`
for `stella-app`,
`8dec465d57a764e76533a82142b05fe21eab92ca76a4526309d5e38ff905a048`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Purple Lua math.random uses Darwin rand and float32 arithmetic

The vendored host Lua 5.1 library previously exposed the desktop libc and its
double-precision `lua_Number` implementation of `math.random`. Purple does
neither. IDA and Hopper independently recover `math.random` at
`sub_100518198`: it calls `rand` before inspecting the Lua stack, reduces the
signed result modulo `0x7FFFFFFF`, converts it with `SCVTF S`, and multiplies
by the four-byte constant at `0x1009F4128`. The constant bytes are
`00 00 00 30`, exactly `2^-31` as float32. The one-argument path multiplies
and floors through S registers before adding float32 one. The two-argument
path forms `1 - lower + upper` in a 32-bit W register, multiplies and floors in
float32, widens the offset and lower bound for one double add, then narrows the
result back to float32. Invalid intervals use `interval is empty`; every other
nonzero arity uses `wrong number of arguments`. Because `rand` precedes all
validation, even both error paths advance the shared stream.

`math.randomseed` is the adjacent `sub_1005182EC`. It applies the Lua 5.1
`luaL_checkint` conversion to argument one, calls `srand` with the resulting
32-bit word, ignores additional arguments and returns no values. Numeric
strings are therefore accepted by both wrappers. Local Darwin libc probes pin
the target recurrence rather than delegating to a platform-dependent host
implementation: the state defaults to one; a literal zero state is replaced
with `123459876`; every result is `state * 16807 mod 2147483647`. Retaining the
pre-modulo seed is observable: `srand(0x7fffffff)` returns zero once before the
zero fallback, while `srand(0xffffffff)` begins with 16807.

`game_lua/math_random.rs` now owns a portable implementation of that iOS
stream and installs both functions before any shipped bytecode is evaluated.
It keeps one logical process stream per game runtime so the original
single-VM application remains faithful without allowing parallel test or
embedded runtime instances to corrupt one another. The audit also found one
other gameplay-side libc-rand use at `0x100069004`: when a theme layer's
`sprite` is a table and `animationSpeed` is not numeric, the constructor
selects its static sprite with `rand() % count`. Numeric animation speed keeps
the first frame and consumes no libc sample. This is distinct from the CMWC
source used for positions, speeds, animation timelines and ordinary particle
construction.

Three focused regressions pin seed one, zero, `RAND_MAX` and unsigned-negative
seed edges; exact promoted float64 bit patterns for Purple's float32 results;
one- and two-bound intervals; numeric-string seeds; ignored seed arguments;
and stream advancement on interval and arity errors. The complete workspace
passes all 569 tests (82 app/audio/wgpu, 31 assets, one core and 455
script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Independent empty-AppData 600-frame headless and
real-wgpu runs report 73 optional reads, zero invoked fallbacks and zero
remaining compatibility bindings. The inspected wgpu readback SHA-256 is
`b1d3851a335a29a3c59436ee8d6cff748abe39c75e7e05c0fd8af3ded8a85ba1`;
it is execution evidence, not a visual oracle.

Current SHA-256 values are
`8105a588aba2431838499b2e8275b455ee447bb7e69eab8440c7becaceb798bf`
for `stella-app`,
`8d0c05d68a8c8a6512cebbe0c2673249e77b680c5924a9891313d6347d67b1a5`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### LEAVES skin attachment basis and the constructor physics lock

The shipped `LevelLoadTransition.lua` loads `animations/LEAVES.anim.json`,
plays `Transition_Animation` at speed 1.5, swaps the destination after its
`PLAYBACK_END`, and then plays `Transition_Animation_Backwards` at the same
speed before disposing the wrapper. Both authored actions are 0.8 seconds and
the default skin contains 16 leaf attachments; 15 of those carry a non-zero
attachment rotation.

IDA's `sub_100011BC4` and Hopper's assembly at
`0x100011E94..0x100011F14` independently show that a custom sprite skin
attachment calls `sinf(-rotation)` but `cosf(rotation)`, normalizes both
single-precision basis vectors, and only then applies the attachment's
single-precision X/Y scales and translation. This is deliberately asymmetric
with the ordinary animation rotation target in `sub_10041F4B0`, which uses
the usual positive-angle basis. Reusing the ordinary basis for skin data made
the rotated leaves point the wrong way and left holes in the supposedly
closed transition.

Rust now gives skin attachments their own inverse-rotation affine composition
path, quantizes every skin transform field to the native float32 boundary,
and uses the same path for draw submission, world bounds, and compatibility
queries. A shipped-resource regression pins the 0.8-second duration, all 16
leaf commands, descending native layer order, and a non-symmetric settled
leaf matrix so the sign cannot regress. A deterministic real-wgpu drive also
completes the original IN, destination load, OUT, and wrapper-removal sequence.

That drive exposed a separate frame-order dependency. `GameLua::GameLua`
(`sub_10002C274`) stores one at GameLua `+0x6A8`; the physics lock total does
not begin at zero. The original startup scripts later release this unnamed,
idempotent lock with `setPhysicsEnabled(true)`. Rust now constructs the same
one-reference unnamed lock, so menu/bootstrap frames skip Theme/Box2D work
instead of calling the shipped `updatePhysics` before its level tables exist.
Direct subsystem test fixtures explicitly release that constructor lock when
they intentionally enter at an already-running physics frame.

The workspace now passes 540 tests (79 app/audio/wgpu, 31 assets, one core,
and 429 script/physics); strict all-target/all-feature Clippy and the release
build are clean. The release LEAVES lifecycle reported zero invoked fallbacks
and zero remaining compatibility bindings. Current SHA-256 values are
`092d3ce15fa4c3151440d4169f600fb3e9464dfc26829464af9acd51adf008fc`
for `stella-app`,
`04e4b18729807e8b65d1883e33e527c2dbca22cc08ff715982416bd2747e7ea0`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

Fresh isolated-AppData headless and offscreen-wgpu runs both complete 600
frames with 67 optional missing globals, zero invoked fallbacks and zero
remaining compatibility bindings. The offscreen file existed only to force
the wgpu upload/readback path (587,760 bytes); it was never opened or viewed.
Both temporary trees were moved recoverably to Trash as
`stella-system-bidi-headless.Mny8Yc` and `stella-system-bidi-wgpu.FbTN6W`.

### UIKit system-font fallback faces and per-face wgpu outline scaling

The `SystemFont` call boundary recovered in the preceding passes also proves
that fallback cannot be represented as a `.notdef` glyph from the requested
face. IDA at `0x10047692C` and Hopper both show width measurement delegated to
the complete NSString plus retained UIFont, while the miss path at
`0x100475E40`--`0x10047618C` uses the same pair for allocation and drawing.
CoreText is therefore free to replace individual grapheme runs with cascade
fonts without changing the LabelPool hash, constructor metrics or public font
name. Purple itself never records those hidden faces.

A non-image CoreText run probe of `ArialRoundedMTBold` at 40 points establishes
three concrete fallback contracts on the current Apple font set. Hebrew in
`"abc אבג 123"` uses `LucidaGrande` glyphs `610,609,608`; Arabic in
`"abc مرحبا 123"` uses `GeezaPro` contextual glyphs
`241,244,261,273,345`; and Han in `"abc 漢字 123"` uses
`PingFangSC-Regular` glyphs `20344,2561`. Their NSString widths are
`228.73046875`, `241.67474287974684` and `243.3203125`, hence Purple returns
228, 241 and 243 after `FCVTZS`. Exact per-glyph X coordinates from every run
are now regression inputs as well. A separate NSString probe shows that all
three still allocate height 46: fallback ascender/descent/leading values do
not replace the base UIFont line-height contract.

The cross-platform font catalog now keeps the host `fontdb::Database` behind
one shared, pointer-stable cache. It selects a known platform cascade face by
Unicode Script where available and otherwise searches regular installed faces
for complete grapheme-cluster coverage. Positive and negative cluster results
are cached, and selected collection data is copied only once per face rather
than once per character. On macOS the font scan also includes Apple's local
MobileAsset font directory, which is where the CoreText-selected PingFang face
resides on this host; other platforms retain their ordinary fontdb system
directories and equivalent installed-family preferences. LastResort and the
currently unsupported bitmap-only Apple color-emoji face are not mistaken for
vector-outline fallbacks.

`SystemFontLayout` now retains a small face table. Every shaped glyph carries
its face slot plus baseline-relative logical-pixel X/Y coordinates, since two
fallback fonts need not share units-per-em. Bidi visual runs, script runs and
extended grapheme clusters are itemized before shaping; RTL face runs are
visited in visual order without splitting a combining sequence. Measurement
sums each face's advance using its own point-size/em ratio before the single
native `FCVTZS`. The wgpu label path opens the corresponding retained face for
each glyph and applies that face's independent outline scale, while keeping the
base UIFont baseline, line height, stroke geometry, anchors and native cache
hash unchanged.

Regressions use two bundled fonts to prove deterministic missing-cmap run
splitting and one-copy cache reuse without depending on Apple fonts. The
conditional Apple contract asserts all three fallback PostScript names, exact
glyph IDs, every CoreText X position, zero Y offsets and widths. An app-layer
test then sends the mixed Hebrew label through the real Lua SystemFont binding
and verifies that the fallback slot reaches vector rasterization with its own
em scale and produces coverage in the fallback run. These are numeric/font
table checks, not screenshot similarity checks.

The complete workspace passes 443 tests (60 app/audio/wgpu, 31 assets, one
core and 351 script/physics). Formatting and strict all-target/all-feature
Clippy are clean. Read-only verification again classifies all 2,602 shipped
files, including 1,167 archives, 718 Lua chunks, 448 JSON documents, 114 KA3D
envelopes and 72 PVR textures. Release SHA-256 values are
`26c4fd70758b884c639ecb837c175be712bf08974305255bd2f506674f5d11a3`
for `stella-app`,
`d9546bc9af563f972636ab1682f78e32fc315803a554ae742ce26585457cb80b`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

Fresh isolated-AppData headless and offscreen-wgpu runs both complete 600
frames with the same 67 optional missing globals, zero invoked fallbacks and
zero remaining compatibility bindings. The offscreen output was checked only
for existence and size (587,613 bytes), never opened or viewed. Its two audit
trees were moved recoverably to Trash as
`stella-system-fallback-headless.k3wGKl` and
`stella-system-fallback-wgpu.ThS1uY`.

### UIKit color-glyph fallback, Apple sbix optics and double text passes

Screenshots remain non-authoritative for this pass. They can reveal a missing
or misplaced glyph, but they are not used to choose a coordinate, color,
fallback face or compositing rule. The evidence chain is instead the two
disassemblers, public CoreText numeric APIs, OpenType tables and deterministic
pixel/table assertions. No generated image was opened or compared.

IDA's decompilation of `game::SystemFont::Impl::drawString` at
`0x100475B98` and Hopper's independent pseudocode agree on the complete cache
miss sequence. `sizeWithFont:` determines the bitmap dimensions, a DeviceRGB
`CGBitmapContext` is allocated and vertically flipped, and a stroke width of
at least one enters the branch at `0x100475FF4`. That branch translates by the
stroke padding, sets line width to twice the requested stroke width, selects a
round join and `kCGTextStroke`, then sends `drawInRect:withFont:`. It restores
the fill color, selects `kCGTextFill` and sends the same message again at
`0x10047618C`. With no stroke, UIKit receives only the single draw call. The
upload and `LabelPool::addLabel` still occur only after the graphics context is
popped and released. This proves that an intrinsic-color glyph is composited
once without a stroke and twice with a stroke; treating its sbix bitmap as a
monochrome outline or tinting it with the CGContext stroke color is incorrect.

A CoreText run probe of `ArialRoundedMTBold` at 40 points establishes the
fallback payload without rendering an image. `"😀"` selects
`AppleColorEmoji`, glyph 2096, advance and NSString width 40. `"A😀B"` forms
three runs: Arial glyph 36 at X `0`, Apple glyph 2096 at
`28.76953125`, and Arial glyph 37 at `68.76953125`; the precise width is
`97.5390625` and Purple truncates it to 97. The single-glyph AAT results for
`"☀️"`, the family ZWJ sequence, `"👍🏽"`, `"🇨🇳"` and `"1️⃣"` are respectively
189, 3237, 1139, 423 and 139, all with 40-point advances. By contrast plain
`"☀"` remains HiraginoSans-W3 glyph 8215; VS15 forces text presentation while
VS16, ZWJ, keycap and the default emoji ranges force the color cascade. Plain
copyright, heart and trademark characters similarly remain in their text
faces unless their presentation sequence requests emoji.

`/System/Library/Fonts/Apple Color Emoji.ttc` contains two faces and the
`sbix`, `morx` and `trak` tables but no COLR or SVG table. Its 800-unit em has
20, 26, 32, 40, 48, 52, 64, 96 and 160 ppem strikes; glyph 2096 at 40 ppem is
a 40-by-40 PNG with zero stored X/Y offsets. HarfBuzz/AAT produces the same
ligature glyph IDs as CoreText after filtering Apple's invisible, zero-position
glyph 3. CoreText also applies a private optical transform that is absent from
the sbix header. The effective raster em is `1.25 * pointSize` through 16
points, `0.5 * pointSize + 12` from 17 through 23, and the requested point size
from 24 upward. Its downward origin shift is `0.25 * pointSize`, then
`6 - 0.125 * pointSize`, then `0.125 * pointSize` across the same intervals.
The exact advances for sizes 1 through 25 are
`1,3,4,5,6,8,9,11,12,13,15,16,17,19,20,21,22,22,23,23,23,24,24,25,26`;
larger sizes use the point size. These are CTFont bounding boxes, advances and
run positions, not coordinates inferred from a capture.

The host fallback catalog now recognizes embedded `sbix`, bdat/CBDT and
EBDT bitmap faces plus COLR/CPAL faces and applies grapheme-level text/emoji
presentation before the ordinary cmap fallback. The retained shaper sets ppem
and ptem, keeps each selected face's em scale, applies Apple's measured optical
size/origin/advance contract, and preserves the one-glyph AAT ligatures for
ZWJ, modifier, flag and keycap sequences. PNG, premultiplied BGRA32 and packed
or row-padded mono/gray 1/2/4/8-bit embedded glyphs are decoded into one shared
raster model. Fractional glyph origins use bilinear coverage sampling and
premultiplied source-over, while vector outlines retain the recovered centered
round stroke.

The COLR stage is split into
`stella-app/src/assets/system_font/color_outline.rs`, keeping the large label
cache/raster facade separate in the same way Purple delegates painting to
CoreGraphics. It traverses COLRv0 and COLRv1 paint graphs, captures nested glyph
and box clips, concatenates every affine paint transform, evaluates normalized
linear, two-circle radial and clockwise sweep color lines, and maps all 28
OpenType composite modes to the CPU raster compositor. The resulting
straight-alpha intrinsic raster then enters the same recovered stroke/fill
pass ordering before wgpu upload. A local Apache-licensed COLRv1 conformance
font was used transiently to exercise all three gradients, every affine paint
form and composite paint; its external path was removed afterward. Portable
regressions retain the color-line extend, radial branch, full composite-mode,
bitmap-format, Apple sbix, AAT shaping and double-pass alpha contracts.

IDA and Hopper independently recover the final theme-repeat discrepancy in
`sub_10009BDB4`, `sub_10009C4C4`, `sub_10009CA0C`, `sub_100067A04` and
`sub_10006853C`. Purple retains the reference position and every repeated tile
step as float32 world coordinates, visits right columns before left columns and
up rows before down rows, and projects every candidate separately. The loop
bounds use the tile centre plus or minus a symmetric half-size in world space;
the final screen rejection likewise uses the projected centre and symmetric
half-size, not the atlas pivot-relative rectangle. The implementation now lives
behind the small theme-render facade in `scene_render/theme/repeat.rs`; two
asymmetric-pivot and long-repeat regressions pin the centre-culling and float32
accumulation contracts.

The original `PigAnimation.lua` and `PigEyes.lua` bytecode confirms the green
pig state thresholds directly. Linear speed above 0.5 or angular speed above 1
selects moving; a launched bird becomes visible to the pig after 0.5 seconds,
then distances greater than `1.5 * detection`, 3 and 1.5 select idle/scare0,
birdShot/scare1, birdNear/scare2 and birdCollidedNear/scare3 respectively.
Blink hides both pupils for the configured duration, while `PigEyes` exposes
the original one-frame cached-visibility lag. A continuous L01 trace observes
exact 12-frame blinks for the shipped 0.2-second duration and the expected
false/true pupil transition on the following frames. After launch the body
selects `PIG_NORMAL_SCARED_1` and the two direct post-draw pupil regions follow
with their own matrices inside the flipped pig context.

The adjacent compatible wgpu command coalescer retains one uniform per native
submission and painter order while reducing a complete L01 frame from 703
sprite commands to 30 draws. A direct L44 load contains 587 objects, two joints
and four pigs; its final frame has 1,105 uniforms, 6,630 vertices and 28 draws.
The 1,800-frame load/settle audit took 10.25 seconds wall time on this host,
about 175 simulated frames per second including load and final GPU work, with
no missing render resources, non-finite commands or fallback calls.

Cross-references from all five scene constructors into `sub_100073110` and
`sub_100073220` recover the dispatch container that precedes
`sub_10004BAB4`. The outer key is the z value narrowed to float32 and truncated
to an integer. Its value is a
`std::map<game::SpriteSheet*, std::vector<std::string>>`; constructors obtain
the retained sheet pointer from `AtlasSprite+0x18` and append the object name to
that leaf vector. The draw member walks integer z buckets, then sheet-pointer
order, then vector insertion order. It never alphabetizes object names or sorts
the original fractional z values.

`SpriteCatalogRegion` therefore retains a native sheet-allocation identity in
addition to its texture source. Reconstructing a same-named sheet advances that
identity while scene objects and composite parts retain the old value, matching
native pointer lifetime. `scene_render/index.rs` now owns a persistent
`BTreeMap<i32, BTreeMap<u64, Vec<String>>>` reconstruction of GameLua+0x310;
scene traversal groups by that identity and uses the actual leaf vector's
insertion order within a group. `scene_render/objects/model.rs`
contains only the per-frame snapshot fields consumed by `sub_10006D5B4` and
`sub_10006794C`; fixture proxies,
collision material arrays, solver state and other physics-only vectors are no
longer deep-copied for every visible object. A 9,300-frame menu-to-L01-and-shot
route retains 887 uniforms, 5,322 vertices, 30 draws and 13 textures, while its
wall time falls from the prior clean 23.61 seconds to 22.25 seconds on this
host. No render command or resource was removed to obtain that reduction.

The insertion order has its own lifetime and cannot be substituted with the
Box2D object-creation ordinal. IDA's `sub_10004C7FC` and Hopper's corresponding
pseudocode agree that `native_setSprite` erases the object's name from its old
leaf and appends it to the destination leaf only when the retained
`SpriteSheet*` changes. Conversely `changeZOrder` at `sub_1000592C4` always
erases and appends, even when both float z values truncate into the same integer
bucket. The persistent Rust leaf vectors are now the insertion-order authority;
regressions cover a same-bucket z change, a cross-sheet sprite change and a
same-named SpriteSheet reconstruction whose old object remains in the old
pointer bucket until an explicit sprite rebind.

The removal lifetime is intentionally asymmetric. In `sub_100042260`, both
disassemblers show the object z narrowed with FCVTZS, the retained direct or
first-composite sheet pointer obtained, and `sub_100073110` plus
`sub_100073220` called before `std::__find` erases the first matching name from
the leaf vector. There is no inner or outer `_Rb_tree::erase`; the two
`operator[]` helpers would even create a missing path. Empty SpriteSheet and z
nodes therefore survive. The Rust index preserves those nodes instead of
rebuilding or pruning it from the live object map.

Constructor replacement has the complementary asymmetry. At the start of
`sub_100036D38`, `sub_100070278` returns the name-map `operator[]` slot and the
new RenderObjectData pointer is stored unconditionally. A reused name therefore
overwrites the lookup pointer without erasing any older z/SpriteSheet leaf;
both old and new leaf occurrences resolve to the new object during drawing.
The constructor also allocates a fresh Lua table with `sub_100529C84` before
overwriting `objects.world[name]`, rather than mutating the old table in place.
The Rust mirror now preserves both behaviors. Finally, literal `"ground"` is a
special render-index exclusion only in the box constructor at
`0x100034D44` and non-physics constructor at `0x10003718C`; IDA reports no
corresponding string xref from circle, polygon or line construction. Those two
paths now skip their initial render leaf, while a later `changeZOrder` still
creates and appends one through the ordinary native move path.

The z-range endpoint is likewise an integer-loop contract rather than a
geometric inclusive bound. `sub_10004BAA8` stores the two FCVTZS results
unchanged. In `sub_10004BAB4`, ARM64 `CMP/B.GE` rejects an initially empty
range and the loop continues only while `bucket < maximum`; a negative stored
maximum is replaced locally by 170. Scene filtering now uses `[minimum,
maximum)` and the native 170 sentinel. Tests pin the empty `[4,4)` range, the
float32-rounded `[4,5)` selection and `[169,-1)` selecting bucket 169 but not
170.

The dispatcher does not defer every Lua z callback until the end of the frame.
`DrawCalls.lua`'s original bytecode shows `initDraw` rebuilding a sorted private
list from callbacks whose `zOrder` is truthy and setting
`hasZOrderedDraws=true`; `draw(z)` repeatedly invokes and removes the head while
`callback.zOrder <= z`, and a nil z becomes the `1e10` final sentinel. In
`sub_10004BAB4`, the captured `draw` function is called with the current
integer converted to float before the SpriteSheet leaves of every existing z
bucket, including a retained empty node. Rust now walks the persistent outer
tree and performs that callback before opening its live SpriteSheet leaves.
Visibility is checked only after the bucket callback, so both a bucket
containing solely hidden bodies and a bucket emptied by `removeObject` still
drain their Lua layer. Regressions fix both cases.

The `native_setSprite` special branch is also no longer ambiguous. Both
disassemblers show `sub_10045C06C` selecting the final Resources `+0x588`
priority entry only when its type tag is 2, then resolving it through the map at
`+0x4F8`. This is the loaded `CompoSpriteSet` path already represented by the
retained bound-composite record, not the unrelated Flash animation callback.
Its scene bucket key comes from the first composite child's `AtlasSprite` sheet
pointer; the ordinary branch uses the direct `AtlasSprite` sheet pointer.

The same dispatcher exposes one callback-state bug that had been masked by the
old whole-scene snapshot. At `0x10004BFA4` the pre callback receives the
original Lua object table and byte `+0x139`, which `sub_10006D5B4` and
parameter 8 independently identify as horizontal flip, not visibility. After
the callback returns, the dispatcher reloads alpha, sprite/composite mode,
scale, rotation and decoration fields from the live RenderObjectData before it
submits the object; the post callback reloads `+0x139` again. Rust now snapshots
no scene order at frame start: `draw_registration/scene/walk.rs` rereads the
live z tree, sheet tree and vector length/index after callbacks, while the
compact draw record itself is reacquired after Lua returns. This also preserves
the native vector-shift behavior when a pre callback moves its current object
to another z bucket: the shifted successor can be skipped and the moved object
can be reached again later in the same walk. Regressions pin both that traversal
and same-submission alpha, scale and flip mutations.

The complete workspace now passes 476 tests (69 app/audio/wgpu, 31 assets, one
core and 375 script/physics). Formatting, release compilation and strict
all-target Clippy are clean. Read-only verification classifies all 2,602 source
bundle files: 1,167 encrypted archives, 718 Lua chunks, 448 JSON documents,
114 KA3D envelopes and 72 PVR textures. Release SHA-256 values are
`f2ed110bf390df4e6399eaa12d3f2d9cf304e94b98dd4e3afa7d9bf387c3f492`
for `stella-app`,
`d08f5f1b18608f7131da87f206e8ccb8a1df9c064940b48a99c9184bd2402a2b`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

Fresh isolated-AppData headless and offscreen-wgpu drives both complete 600
frames with 70 optional missing globals, zero invoked fallbacks and zero
remaining compatibility bindings. The offscreen file existed only to force the
wgpu upload/readback path (586,869 bytes, 111 uniforms, 666 vertices, seven
draws and four textures); it was never opened or viewed. Both
CPU and GPU drives used one isolated audit tree. A fresh 9,300-frame L01
interaction route after the live-callback, exclusive-z and Lua-z-interleave
corrections retains 886 uniforms, 5,316 vertices, 31 draws and 13 textures and
completes in 23.12 seconds wall time with zero invoked fallbacks or
compatibility bindings. The extra draw is the required painter-order boundary
at an interleaved Lua layer, not an unbatched sprite regression. Its offscreen
file (703,317 bytes) was likewise never opened. The complete isolated audit
tree was moved recoverably to Trash.

### Constructor Lua-record and initial-mass ABI completion

IDA and Hopper independently expose the complete `objects.world` record built
by the five native scene constructors: box `sub_100034740`, circle
`sub_100034FB0`, polygon `sub_1000357A4`, line `sub_1000364E0` and
non-physics `sub_100036D38`. Every constructor writes `name`, `sprite`,
`type`, `x`, `y`, `angle`, `density`, `friction`, `restitution`, `mass`,
`xVel`, `yVel`, `z_order`, `animTimer`, `animFrame`,
`animThresholdTimer`, `collisionEnabled` and `alpha`. Box, polygon and line
also write the original float arguments as `width` and `height`; circle writes
the original `radius`. `angle`, both velocity fields, both timers and the
threshold timer start at zero, while `animFrame` and `alpha` start at one.

The former Rust mirror omitted the three animation fields and alpha, omitted
line dimensions, exposed a synthetic polygon/line `vertexCount`, and eagerly
published `scaleX`, `scaleY`, `visible`, `velocityX`, `velocityY`,
`angularVelocity`, `active`, `sleeping` and `sensor`. None of those latter
fields appears in any of the five native constructor field-write sequences;
they are installed only by later script/native update paths where applicable.
The mirror now emits only the recovered key set and preserves a fresh table on
same-name replacement. Hopper's `sub_100036D38` additionally confirms that a
non-physics object's density, friction, restitution and mass are all zero and
that collision is disabled, correcting the previous synthetic 0.2 friction.

The `mass` value is read from `b2Body+0x98` after fixture creation, so it is a
Box2D float32 result rather than a host-f64 geometry estimate. Constructor mass
calculation now uses the same float32 circle multiplication order and the same
reference-centred polygon area accumulation, visits compound fixtures in
native reverse-list order, and applies Box2D's unit-mass fallback to dynamic
bodies whose aggregate fixture mass is not positive. A five-kind regression
compares each exact Lua key set, raw shape dimensions, animation/alpha
defaults, non-physics coefficients and published mass against the installed
body.

The complete workspace now passes 477 tests (69 app/audio/wgpu, 31 assets, one
core and 376 script/physics); formatting, release compilation and strict
all-target/all-feature Clippy are clean. Current release SHA-256 values are
`0fc44ce8b63de3f0416530b0bb47a7525e24547c193f9ce83ea0cfe2acba9e10`
for `stella-app`,
`a13f7f85680c40055181b7b95832d5b1263181da9dc58d62e8cb229867387538`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

Fresh isolated-AppData headless and offscreen-wgpu drives both complete 600
frames with 70 optional missing globals, zero invoked fallbacks and zero
remaining compatibility bindings. The 600-frame wgpu checkpoint contains 113
uniforms, 678 vertices, seven draws and four textures. The 9,300-frame
menu-to-comic-to-L01 launch route contains 888 uniforms, 5,328 vertices, 31
draws and 13 textures and completes in 23.40 seconds wall time. The two PNGs
(587,841 and 702,909 bytes) existed only to force the wgpu path; neither was
opened or inspected, and their isolated audit directory was moved recoverably
to Trash.

### Outer draw, retry/result lifetime and trajectory timing correction

The native member at `0x10004BAB4` is specifically `drawGameNative`, not the
application's outer draw callback. IDA's references to `DrawCalls`,
`hasZOrderedDraws` and `draw`, plus Hopper's matching scene-tree walk, show
that it drains Lua z callbacks and native scene buckets only when
`GameScene:draw` calls it. The shipped `gamelogic.lua` owns the actual frame
order: root menu, `notificationsFrame`, notification particles,
`drawLoadingScreen`, subsystems and menu particles. The old host bypassed that
callback and then forced a second `DrawCalls.draw` pass.

That one boundary error caused three visible failures. Skipping
`notificationsFrame:draw` updated but never submitted the
`LevelLoadTransition` LEAVES animation. Skipping `drawLoadingScreen` left a
restart request parked at `g_drawLoadingScreen=true` without ever emitting its
`EID_CHANGE_SCENE`. Finally, forcing the native/Lua scene drain after
`GameScene:draw` had selected a full-screen `LevelCompleted` or `LevelFailed`
child leaked level bodies onto the result UI. `StellaLua::draw` now clears the
native command buffers and invokes the shipped global `draw` exactly once; it
does not initialize or drain `DrawCalls` itself. A regression pins the six
outer stages and proves there is no second host pass.

The interactive desktop loop had another native-boundary inversion. Purple's
display-link callback passes one elapsed delta to `sub_10005E898`; that member
accumulates its own fixed 1/30 Box2D steps internally. The old desktop host
instead repeated the entire Lua update, scene traversal and draw once per
missed 1/60 tick, causing an object-heavy level to enter a catch-up spiral.
Interactive wakeups are now coalesced into one callback with the whole elapsed
delta, capped at 100 ms with no residual render debt. Deterministic screenshot
execution remains fixed at 1/60. Unit tests pin both sub-tick coalescing and
stall clamping.

Trajectory reverse engineering found two independent ABI omissions. The
adapter for `clearAimingAid` at `0x100088D24` requires one number and forwards
it to `sub_10004BA70`. Before deactivation, `sub_1000082CC` upper-bounds the
particle vector by `pathParameter / (controlCount - 3)`, compacts only the
suffix beyond the supplied progress and discards the prefix. Thus
`clearAimingAid(1)` immediately removes every prediction particle; merely
switching off spawning lets the old line remain visible. Rust now reproduces
the strict argument and float32 prefix compaction.

More importantly, the LuaObject at GameLua `+0x408/+0x420` used by
`sub_100032970` and `sub_10004B8EC` is the `objects` table. GameScene bytecode
sets `objects.currentTimeStep` to float32 1/90 while aiming and 1/30 otherwise.
The predictor multiplies that value by the shipped factor three, so an aimed
simulation step is exactly 1/30; the former Rust code incorrectly read the
global render-frame delta and normally predicted at 1/20. The corrected lookup
is pinned by all predictor and AimStream tests.

A fresh numeric L01 route, without inspecting its PNG, retained 50 predicted
points and 19 real flight samples. The early real samples lie on the corrected
prediction; later differences begin only after solid-body interaction, which
the recovered one-body predictor deliberately omits. The same route then
reached the result state and clicked restart. Its `objects.world` population
changed from 285 to 292 as `Chapter01_L01` was reconstructed, the loading flag
returned false and the recovered action was
`INITIALIZE_GAME_AFTER_RELOAD_FROM_PAUSE_MENU`, confirming that the button ran
the full native/Lua reload state machine rather than only changing UI state.

After these corrections the complete workspace passes 482 tests (71
app/audio/wgpu, 31 assets, one core and 379 script/physics). Formatting,
strict all-target/all-feature Clippy and release compilation are clean. A
fresh isolated-AppData 600-frame offscreen-wgpu route reports 70 optional data
reads, zero invoked fallbacks and zero compatibility bindings; its 589,177-byte
PNG existed only to force GPU upload/render/readback and was not opened or
inspected. Release SHA-256 values are
`35f917bb2f990629c61069063968d8a388237a527203115ff68f2da673716559`
for `stella-app`,
`a99d3b49f82c512571b20b37cf59bfd5b4da4ba8a94243737788b47cc5590956`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### CADisplayLink cadence and complete level-container audit

`-[AppController startUpdate]` at `0x100404CA8` constructs a
`CADisplayLink` targeting `update`, derives the legacy frame interval from the
configured frame rate, and adds it to `NSDefaultRunLoopMode`. Its callback at
`0x100404374` has an `m_insideUpdate` re-entry guard, computes a float32
monotonic-clock delta, clamps negative time to zero and clamps a long frame to
the exact float32 value `0.1f` before calling the app. IDA and Hopper agree on
the callback/clamp flow and the display-link ownership, while direct assembly
resolves IDA's `60 / framerate` interval expression more reliably than
Hopper's decompiler rendering.

The desktop host now uses the recovered 100 ms limit instead of 250 ms. Its
winit deadline is also based on the unconsumed sub-frame accumulator rather
than `now + 1/60`: an input event that wakes the run loop early no longer moves
the next render tick later. After a rendered frame the accumulator is empty and
the next deadline is one full tick from the accounted clock instant; after an
8 ms input wakeup only the remaining portion is scheduled. This preserves the
independent display-link cadence without reintroducing multi-render catch-up.

A command-driven audit then booted the original game and passed every one of
the 153 shipped `levels/**/*.lua` containers through the native `loadLevel`
binding in one runtime. All encrypted Lua chunks transcoded, executed, passed
their filename identity check and published `loadedObjects`; the complete
sweep ended with nine optional data probes, zero invoked fallbacks and zero
remaining compatibility bindings. This is table/parser coverage rather than a
claim that every level's interactive solution has been played.

The workspace now passes 483 tests (72 app/audio/wgpu, 31 assets, one core and
379 script/physics); formatting, strict all-target/all-feature Clippy and
release compilation are clean. A fresh isolated-AppData 600-frame wgpu route
again completes with 70 optional probes, zero invoked fallbacks and zero
compatibility bindings. Its 590,056-byte PNG was not opened or inspected.
Current release SHA-256 values are
`0744e107ded36dff5ed95f3e268c18f3207dd2941257406f9f26bc183c8be4e6`
for `stella-app`,
`a99d3b49f82c512571b20b37cf59bfd5b4da4ba8a94243737788b47cc5590956`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for `stella-tool`.

### Calendar, ServerTime, scalar-adapter and locale completion

The calendar quartet registered at `0x10002EC4C..0x10002ED00` is implemented
by `sub_10005698C`, `sub_100056AB0`, `sub_100056C98` and `sub_100056D68`.
IDA and Hopper agree that `getCurrentTime` uses `time` plus `localtime`, then
publishes `year/month/day/hour/minutes/seconds` through the engine's float32
Lua bridge. The shared input converter `sub_10005D700` requires year, month
and day, defaults the three clock fields to zero, and leaves its zeroed
`tm_isdst` at standard time before `mktime`. `getTimeDifference` takes the
absolute `difftime`, converts it with `FCVTZU W` and decomposes that u32 into
days, hours, minutes and seconds. The signed-seconds member instead narrows
the double difference directly to float32.

`addDurationToTime` has a distinct contract: all six source fields and a
NUMBER duration are mandatory. It performs float32 addition into `tm_sec`,
uses `FCVTZS W` for every calendar component, sets `tm_isdst=-1`, and lets
`mktime` normalize the structure. The former Rust implementation treated a
missing hour as Lua 5.1's os.time default of noon, inferred DST where the
difference members force standard time, added an i64 duration to epoch
seconds, and returned an exact i64 difference. The replacement preserves the
recovered local-time, defaulting, DST, float32 and u32 boundaries.

The three force/time multipliers are the float members at GameLua offsets
`+0x530`, `+0x534` and `+0x554`, reached through setters
`sub_100031168`, `sub_100031178`, `sub_100031378` and the common generated
adapter `sub_100088D24`. That adapter strictly reads Lua slot one as NUMBER;
the old host scanned for the last numeric argument and silently ignored bad
calls. All setters now reject missing/wrong slot-one values and store the
float32 result. Likewise, `performBitwiseAnd/Or` are not host-u32 helpers:
`sub_100056964` and `sub_100056978` execute two `FCVTZS W` conversions,
signed 32-bit `AND/OR`, and `SCVTF S` before returning. Negative values,
fractions, integer-indefinite inputs and large-result float32 rounding are now
pinned by regression tests.

Purple's separate ServerTime service constructor `sub_1000BCEC0` registers
four methods, not one. `sub_1000BD1BC` passes the offset-adjusted epoch to
`gmtime_r`; `sub_1000BD224` passes it to `localtime_r`; `sub_1000BD28C`
returns `STATUS_OK` only for status zero; and `sub_1000BD110` starts the
optional HTTP synchronization. The implementation object initializes both
offset and status to zero. The offline rehost therefore exposes the complete
table, keeps synchronization void, reports `STATUS_OK`, and returns distinct
UTC and local float32 calendar tables without inventing network state.

Finally, locale refresh `sub_100050948` no longer feeds `g_currentLocale`
back into `setLocale`. It walks configured/system preferred languages, maps
the `ja`, `ko` and `en` prefixes to `ja_JP`, `ko_KR` and `en_EN`, chooses the
first value present in the live `TEXTS_BASIC` locale vector, and falls back to
`en_EN`. The cross-platform host obtains the preference list from its locale
environment (with `STELLA_LOCALE` as the explicit configuration analogue)
and validates it against the parsed native text-group resource.

Direct regressions now call all 243 unique GameLua globals published by the
constructor. The final gaps pin `clearParticlesNative` clear-all behavior,
strict Lua-file probing, byte-exact nested Bundle-to-AppData copying and the
string/void boundary of `playVideo` (the shipped bundle contains no video
asset, so the offline service records the request without inventing playback).

The workspace now passes 491 tests (72 app/audio/wgpu, 31 assets, one core
and 387 script/physics). Formatting, strict all-target/all-feature Clippy and
release compilation are clean. A fresh isolated-AppData 600-frame wgpu route
completed with 70 optional data probes, zero invoked fallbacks and zero
remaining compatibility bindings. Its 587,887-byte PNG existed only to force
wgpu upload/render/readback and was not opened or inspected; the audit tree
was moved recoverably to Trash. Current release SHA-256 values are
`59a44b736aa49bf902991a05a40c2cd629ebe78953f1d13f4049125bf76723ec`
for `stella-app`,
`ce04826508576c7c71740b95320c95b0f132903238a1ed840f6fbb8169bc26ee`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for `stella-tool`.

### Challenge-result edge occlusion and priority-flow revalidation

The remaining reports of level objects on a result screen belong to the
Frenemies challenge result path, not to the ordinary `LevelCompleted` and
`LevelFailed` paths. A command-level capture shows that the challenge class
continues drawing the native world first and then submits
`ENDSCREEN_BG_STRIP` at screen x coordinates 0 and 959 with unit scale. The
central `ENDSCREEN_WIN` composite is only 1008 pixels wide. Replaying those
exact pre-fix commands over a sentinel framebuffer leaves 11,888 pixels
uncovered, mostly the full-height left/right edge columns and the bottom
corners; foliage, pigs, and blocks can consequently remain visible there.

This is not a lost `drawSprite` size argument in the Rust bridge. IDA's
`sub_10045C144` and atlas branch `sub_100467AF0`, independently confirmed by
Hopper, preserve the supplied destination width and height through the final
sprite virtual call. The shipped `Image:drawSelf` supplies its current `w/h`,
which remain the strip's intrinsic 1 by 770 size. The actual omission is in
the shipped Lua class hierarchy: `FrenemiesChallengeLevelCompleted` directly
inherits `ScalableLayout` while reusing `FrenemiesLevelCompleted.layout.lua`,
but does not implement the `layout` override that calls
`setNonUniformScale(1000, scaleY)` on both edge strips. Ordinary level-win,
level-fail, and Frenemies-result classes all contain that override.

The host now installs only that missing challenge-class layout method after
the shipped game script is loaded, following the identical two-strip code in
the sibling result classes. It deliberately does not change general sprite
scaling, destination-size, composite, or painter-order behavior. Render trace
output now also includes each command's retained destination size so future
layout audits do not confuse scalar state with the independent draw-size
overload.

A boot-level regression executes the repaired method against independent
left/right children and pins the 1000x horizontal scale and inherited vertical
scale. Separate software-reference and actual headless-wgpu tests draw the
four challenge background submissions over a magenta sentinel and require
zero uncovered pixels across the complete 1024 by 768 framebuffer.

The previously corrected priority issues were also revalidated rather than
inferred from a screenshot. The display-link coalescing and 100 ms stall tests,
the single shipped outer-draw callback test, and all 14 trajectory/AimStream
tests pass. A fresh isolated-AppData 14,800-frame release route instruments
`levelLoadTransition`, completes L01, reaches a stable `levelCompleted` frame,
clicks the real restart button, and asserts that the LEAVES transition was
both created and drawn, the loading flag returned false, L01 remained the
active level, the world object population was reconstructed, and the result
frame was removed. Its PNG existed only to force final wgpu submission and
readback and was not inspected.

The complete workspace now passes 494 tests (74 app/audio/wgpu, 31 assets, one
core and 388 script/physics). Formatting, strict all-target/all-feature Clippy
and release compilation are clean. Current release SHA-256 values are
`fea71f9ee023ad3c11e1b1c720367264369234f745753227a0fca6b480ac5b22`
for `stella-app`,
`f8de6c40513586c1d733c44d180c64fcd5d19a01de429e26f2d37b610f1ae84f`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Native renderer ownership split after the priority fixes

The post-fix compatibility audit still reports zero invoked fallbacks and zero
remaining compatibility bindings. The next source-layout pass therefore keeps
behavior unchanged and separates the deferred command ABI along the native
owners visible in both connected disassemblers. IDA remains attached to
`Purple.i64` with Hex-Rays and completed auto-analysis; Hopper independently
has the `Purple` document selected.

The ResourceManager sprite dispatcher at `sub_10045C144` resolves a named
resource and branches to the composite member `sub_1004376D4` or atlas member
`sub_100467AF0`. Hopper folds the atlas continuation into a 424-byte/23-block
procedure view, while IDA preserves the dispatcher as an independent
184-byte/8-block function. `GL_Context::drawRect` at `sub_100598CC4` is a
separate 820-byte/11-block member that transforms four corners, rejects the
complete offscreen mesh, chooses the plain versus plain-alpha shader, and
submits one color stream. `game::SystemFont::Impl::drawString` at
`0x100475B98` is independently 2,008 bytes/47 blocks and owns substring
anchoring, LabelPool lookup, UIKit/CoreGraphics rasterization, and the final
label-sprite draw.

The former 1,005-line `render_types.rs` mixed all of those payloads. It is now
a 17-line facade retaining the exact same crate-root exports over dedicated
`sprite.rs`, `text.rs`, `geometry.rs`, `system_font.rs`, and the existing
`system_font_layout.rs`. No command field, public path, renderer ordering or
runtime behavior changed. Production system-font shaping is 477 lines before
its colocated regressions rather than being mixed with unrelated commands.

The wgpu-side system-font unit is split at the same recovered ownership
boundary. `game::LabelPool::addLabel` at `0x100476AEC` is 1,080 bytes and 65
basic blocks in Hopper; IDA independently shows that it owns the DJB2 field
hash, exact `0x500000`-byte capacity, tail eviction and insertion vector. Those
contracts now live in `assets/system_font/cache.rs`. The hit/miss branches of
`drawString` share their anchor conversion and signed `FCVTZS` coordinates;
that smaller contract now lives in `assets/system_font/placement.rs`. Outline,
embedded-bitmap and color-paint rasterization remain separate from both.

The complete workspace still passes 494 tests (74 app/audio/wgpu, 31 assets,
one core and 388 script/physics). Formatting, strict all-target/all-feature
Clippy and release compilation are clean. The rebuilt release is running from
the normal extracted data tree. Current SHA-256 values are
`600e50a2598454175bcc26261e3b9dc601668d0fd8ebbaef2b4f5527ec098bf6`
for `stella-app`,
`f9f53b76bd5fdbccceb02656eac2753e1bcc8226628da905629771482a991f30`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Object-heavy level stall correction at native Box2D boundaries

A deterministic 1,200-frame level matrix identified Chapter 01 Level 50 as a
real CPU outlier: the pre-fix release took about 9.26 seconds, versus 2.86 for
L01 and 4.50 for L61. Final-frame command counts were only 865 sprites and 35
wgpu draws, roughly 35 percent above L01 rather than the observed two-to-three
times CPU cost. A symbolized sample instead placed almost all of the excess in
the fixed Box2D step and in Lua-driven `setPosition`/`setAngle` calls.

IDA's `b2Body::SetTransform` at `sub_10086B794` provides the decisive
ownership boundary. It walks only the selected body's fixture list through
`sub_10086CB74`, then calls the shared broad-phase update member
`sub_10086BC24`. The old bridge synchronized every active body's proxy on
each individual transform setter, turning object-heavy Lua updates into a
world-sized repeated scan. Transform setters now synchronize only the named
body and drain the same shared move buffer. The world-wide synchronization
remains at the recovered `b2World::Solve` tail; a second non-native full-world
pass after SolveTOI was removed. A focused regression moves two unsynchronized
bodies, invokes the single-body member, and proves that only its proxy record
changes until the explicit world pass.

The solver had the same structural mismatch. `ResetMassData` at
`sub_10086B1F4` stores aggregate mass/inverse mass at body offsets `+0x98` and
`+0x9C`, local centre at `+0x1C`, inertia/inverse inertia at `+0xA0/+0xA4`,
and the updated sweep centre in the body. Rust formerly recomputed polygon
mass and inertia in every constraint use. The fixture aggregate is now cached
only when native ResetMassData runs. Likewise,
`InitializeVelocityConstraints` at `sub_100863BC4` visibly indexes compact
12-byte velocity/position arrays by the two body indices stored in each
152-byte constraint; it does not clone complete render objects. Contact
velocity and position passes now capture compact scalar body states, while
retaining the exact live Gauss-Seidel reread between constraints and points.
The pre-warm whole-scene clone and repeated full `SceneObject` point clones
are gone. TOI keeps pre-island records only for bodies that can move; static
endpoints use their unchanged live transforms.

The same isolated L50 route now takes about 7.50 seconds for 1,200 frames,
including startup and the final GPU readback, a roughly 19 percent total-time
reduction and about 6.3 ms per deterministic frame. This is not based on PNG
appearance. Existing contact order, two-point solver, continuous collision,
trajectory, retry/LEAVES and result-occlusion regressions remain unchanged.

The workspace now passes 495 tests (74 app/audio/wgpu, 31 assets, one core and
389 script/physics). Formatting, strict all-target/all-feature Clippy and the
complete release build are clean. Current SHA-256 values are
`1c41898ede710b76bb2dd607c2c9127aaa44dd8411e5f131ab48c32f467bb11e`
for `stella-app`,
`1ecb71dc8f304e9e7d10447530ea2d1ffbe07c8d0596b77363654021e14e9b64`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Texture-state revalidation and legacy resource-owner split

The remaining backend-difference audit first revisited texture filtering rather
than changing wgpu from visual guesswork. IDA and Hopper independently show
the complete 240-byte/six-block texture-state member at `sub_10018AFB8`:
`GL_TEXTURE_MAG_FILTER` is fixed to `GL_LINEAR`, MIN is resolved through
`sub_100206CA0`, and the same wrap result from `sub_100206CE0` is installed on
both S and T. This confirms the existing wgpu linear-clamp base sampler and
linear-repeat texturized-fill sampler. The shipped no-mipmap assets therefore
need no sampler mutation; changing either filter would move away from Purple.

The audit instead exposed a source-ownership mismatch in the 599-line
`resource_manager/legacy_usage.rs`. SpriteSheet construction is an independent
1,552-byte/76-block member at `sub_100457E38` in both disassemblers. Static and
streaming audio construction belong to the separate 1,008-byte/42-block
`sub_10045A1C8`; the WAV reader at `sub_100578858` is independently delimited
as well (IDA includes its exception tails, while Hopper ends the main body
earlier). The Rust source now follows those owners: the public-in-module facade
is 10 lines, SpriteSheet upload accounting is 42 lines, AudioReader/type
detection/decoder construction is 388 lines, MPEG and Vorbis frame accounting
remain independent 204- and 48-line children, and the 170-line regression suite
is no longer mixed with production decoding.

This split changes no resource key, file-type rule, RIFF boundary, decoded PCM,
duration, upload byte count or visibility outside ResourceManager. All seven
direct resource-accounting regressions pass, including all 538 shipped MP3
decodes and the native SpriteSheet PVR payload count. The full workspace still
passes 495 tests (74 app/audio/wgpu, 31 assets, one core and 389
script/physics); formatting, strict all-target/all-feature Clippy and release
compilation are clean. A fresh isolated-AppData 600-frame run again reports 70
optional data reads, zero invoked fallbacks and zero compatibility bindings.
Current release SHA-256 values are
`e98e8aca3ca5e5285d6684caddf6ba2aed75b02928b4a61f56f73804e352e147`
for `stella-app`,
`9429daf2b238766079d948da14d36dba73c82c816efc4e6da44d95e0f2b53fd4`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Native loadLevel scene-owner reset and retry duplication

A fresh isolated-AppData command route now covers the real shipped flow rather
than treating a screenshot as truth: 650 frames to the island map, the first
`BTN_PLAY` at game coordinate `(173,247)`, the automatic Chapter 01 map
transition and introductory comic, `Chapter01_L01`, the pause button at
`(56,53)`, and `BTN_RESTART` at `(102,364)`. The button entered the complete
reload state machine, but the pre-fix stable frame grew from 643 sprite
submissions to 940. Frequency analysis showed nearly every level object twice;
Lua still held only 292 live `objects.world` entries and did not call the
registered `removeObject` once during restart. It instead replaced the entire
`objects.world` table.

IDA identifies the missing native lifetime inside the shared bundle/AppData
loader `sub_100065D3C`. Before opening the requested `.lua`, it destroys the
old RenderObjectData owners, erases the adjacent name maps at
`0x100065FDC/0x100065FFC/0x1000661E0`, and at `0x100066200` invokes the full
`std::_Rb_tree::_M_erase` for GameLua's
`map<int, map<SpriteSheet*, vector<string>>>` at `+0x310`. This is deliberately
stronger than `removeObject` at `sub_100042260`, which erases only the first
matching vector entry and retains empty map nodes. Hopper independently reports
the same `0x100066200` cross-reference to the render-index tree eraser; it also
finds the corresponding destructor use from `sub_10005BDB8`.

Both Rust `loadLevel` adapters now retire the complete native scene and draw
callback owner before file lookup, so even a failed load has Purple's destructive
ordering. Constructor-time `objects.world` identity tracking covers direct Lua
owner replacement without changing the native same-name duplicate-constructor
behavior inside one owner. Ordinary object removal still preserves empty z and
sheet nodes; full level loading resets the entire tree. Regressions separately
pin failed-load cleanup and same-name reconstruction after a world-table change.

The final 11,000-frame route settles at 641 sprites and 34 geometry commands
after restart, with no missing wgpu sprite, zero invoked fallbacks and zero
compatibility bindings. The two-command difference from the 643-submission
pre-restart sample is the live animation/particle frame, not duplicated level
content. The workspace passes 497 tests (74 app/audio/wgpu, 31 assets, one core
and 391 script/physics); formatting, strict all-target/all-feature Clippy and
release compilation are clean. Current release SHA-256 values are
`ffd8c7ed1ff5e83917ac6318bd01ee32e6a496cd5b01dd85c1dc4f935472f287`
for `stella-app`,
`da2818362cac02dfdea728bb92c12bbc24d406115eeabf963aa77900fda8ba62`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Comic timeline validation and native playback completion

The first Chapter 01 comic was re-audited as a timeline rather than a set of
screenshots. A fresh first-run route enters page one at about frame 2,000,
advances its three panels, advances all four page-two panels, returns to the
chapter map at about frame 3,800 and starts L01 at about frame 5,000. The seven
active `Animation` actions have resource durations 4.2332997, 5.6333, 4.7666,
3.8, 2.7666, 2.9333 and 3.6 seconds. Their sequential total is 27.7331 seconds;
the per-page `Disappear` actions run in parallel and contribute at most 0.8
and 0.6666 seconds. This accounts for the measured comic interval and rules
out a host playback-rate correction. The unused page-one panel 4--6 assets are
not part of `episode_1_cutscene_1_intro` and are deliberately not played.

The same audit exposed a separate completion-boundary difference. IDA and
Hopper independently delimit `AnimationWrapper::start` at `sub_100012F18`
(1,148 bytes/51 blocks). It starts an ordinary action, stores the wrapper mode
at the AnimationWrapper component, and installs `sub_100016C50` as the action
completion callback. The latter is 276 bytes/13 blocks in both tools. An empty
mode or literal `repeat` emits `PLAYBACK_REPEAT` and calls `sub_10040E798` with
exactly `0.0f`; literal `once` emits `PLAYBACK_END`; any other string still
queues the callback record but leaves its event name empty. `sub_10040E798`
writes the float time at action offset `+0x2c` and marks its animation owner
dirty. Thus repeat completion discards a large frame's overshoot instead of
applying modulo arithmetic or traversing several synthetic cycles in one
host update.

Rust now uses that exact completion classifier for completed action ends.
Empty-mode repeats remain at time zero and reset their time-zero-event boundary
for the following update; once and unknown modes retain the action-duration
endpoint after their callback. Timeline events are evaluated only through the
one-shot action endpoint before the queued completion event. Focused tests pin
all four mode classes and a 3.25-second update over a 2-second action, including
the next-cycle time-zero event. The workspace passes 499 tests (74
app/audio/wgpu, 31 assets, one core and 393 script/physics).

Formatting, strict all-target/all-feature Clippy and the complete release build
are clean. A fresh isolated-AppData 5,200-frame route clicks the real first-run
play button at frame 650, completes the same seven-panel comic, asserts
`currentLevelName == "Chapter01_L01"`, and finishes its wgpu readback with 85
optional data probes, zero invoked fallbacks and zero compatibility bindings.
The PNG was not inspected. Current release SHA-256 values are
`1e9a41e4fb8810226e86ef8edc5dfe0cb000394683770057a731b844486b6720`
for `stella-app`,
`00737329055137cce41c949d89890762e7a54ea32090be8455059ed5e4c26af0`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Zero-duration controls and exact state-3 advancement

The apparent `0.00001f` argument at the `sub_100012F18` call to
`sub_10040A998` is not a playback-time bias. IDA shows that the 80-byte/four-
block callee ignores that floating argument and only activates the animation
owner/control state. The real static-action rule is recovered one layer below.
IDA and Hopper independently delimit action construction at `sub_100410A18`
as 448 bytes/24 blocks. For ordinary one-shot state 3 it takes the maximum
result of `sub_10040CDAC` over the action controls and stores that float
verbatim; an empty control vector or tracks whose last keys are all at zero
therefore produce an exact `0.0f` duration, with no minimum-frame clamp.

`Animation::Update` at `sub_100411230` is 224 bytes/seven blocks in Hopper
(102 instructions in IDA). Its state-3 branch performs float32 arithmetic on
duration `+0x28`, current time `+0x2c`, per-control speed `+0x24` and the owner
delta. A positive remaining interval completes only when the scaled delta
reaches that interval. An already exact endpoint takes neither completion nor
advancement branch. Consequently a zero-duration action started at zero stays
active indefinitely and emits no completion callback. If a seek placed the
control beyond its duration, small positive deltas move it back toward the
endpoint and a delta covering the overshoot completes it. Negative speed can
move an ordinary one-shot below zero; it does not synthesize a lower-bound
completion. This latter detail supersedes the provisional reverse-end handling
described in the immediately preceding milestone.

An asset-wide structural query over all 216 shipped `.anim.json` files finds
97 zero-duration actions. None contains a non-empty `spineEvent` at time zero.
Rust previously promoted each of these actions to `1/60` second, so repeat-mode
static art could continually queue `PLAYBACK_REPEAT`, seek and re-arm itself.
`AnimationWrapperNative.start` now preserves exact zero and quantizes nonzero
durations to the native float32 representation. Update uses the recovered
state-3 branches and float32 multiply/add/subtract rules, including the exact-
endpoint freeze, overshoot recovery and negative-speed behavior. Focused tests
pin duration construction, all boundary branches and two consecutive updates
of a zero-duration repeat action with no callback.

The workspace passes 502 tests (74 app/audio/wgpu, 31 assets, one core and 396
script/physics). Formatting, strict all-target/all-feature Clippy and the full
release build are clean. A fresh isolated-AppData 5,200-frame route again
crosses the real first-run map/comic flow, asserts
`currentLevelName == "Chapter01_L01"`, reports 85 optional data probes, zero
invoked fallbacks and zero compatibility bindings, and completes its final
wgpu readback in about 5.14 seconds. The PNG was not used as verification.
Current release SHA-256 values are
`8390f27043c89880e46d1219eb2c8794e528047c11927c34e228603cf37cca10`
for `stella-app`,
`6e60419ccaef5c4bb592d53043400124cac9515b862151a6c52c32f09543200b`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Retained completion state and distinct animation stop paths

The surrounding AnimationWrapper state API exposes a second endpoint rule
which the completion callback alone does not show. IDA's `sub_10001384C` and
Hopper's matching 288-byte/12-block procedure implement `isPlaying(tag)` by
testing `(control.state & ~1) == 2`; both state 2 and one-shot state 3 are
reported as playing, while state 1 is not. Natural completion at
`sub_1004110A0` is 260 bytes/15 blocks in Hopper. When a callback is installed,
it invokes the callback without changing state 3 to state 4. Wrapper callback
`sub_100016C50` only queues `PLAYBACK_END` for mode `once`; it does not stop the
control. A completed once/unknown action therefore remains playing at its
duration endpoint, and subsequent updates freeze there without sending the
completion again.

Pause and resume have the same direct state ownership. `sub_100013A98` is 260
bytes/10 blocks and writes state 1 to the retained tag control.
`sub_100013B9C` is 364 bytes/16 blocks and unconditionally restores state 3,
recomputing its exact maximum control duration. Resume does this even if that
retained control was previously stopped.

Stopping by action and stopping all actions in a scene are deliberately not
equivalent. The 252-byte/15-block wrapper member `sub_100013720` sends a
non-empty action to `sub_10041017C`, whose `sub_100410EA0` path removes the
named control from Animation's active vector, writes state 1, seeks to zero and
releases its live attachment. An empty action instead calls the 100-byte/four-
block `sub_10041103C`, which writes state 1 and seeks every active control to
zero without removing them. A later resume can therefore advance an empty-
stop control; after a named stop it can make `isPlaying` true again through the
retained wrapper pointer, but the detached control is no longer updated.

Rust now retains this distinction explicitly in `AnimationPlayback`: state and
active-vector attachment are separate. Natural once/unknown completion keeps
the playing state, empty stop and global stop-all retain attachment, named stop
detaches, resume always restores the reported playing state, and frame update
only advances attached controls. A focused end-to-end native-table regression
pins natural completion, pause/resume, both stop forms and the intentionally
non-advancing resumed detached control.

The workspace passes 503 tests (74 app/audio/wgpu, 31 assets, one core and 397
script/physics). Formatting, strict all-target/all-feature Clippy and release
compilation are clean. A fresh isolated-AppData 5,200-frame first-run route
again reaches and asserts `Chapter01_L01` in about 5.13 seconds, with 85
optional data probes, zero invoked fallbacks and zero compatibility bindings;
the PNG was not inspected. Current release SHA-256 values are
`23ba7d16ca19609042e4c26c09a661f23ce0e12c2347ffd58c2c951fe626759e`
for `stella-app`,
`4c1117fef6ff3e37d08eba5d773a3dbc695f0ed6444929bd25c92017905ecd33`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Retry routes, trajectory ownership and gameplay-frame stalls

Real pointer routes now cover all three shipped restart entry points instead
of exercising a host-only command: pause-menu restart, completed-level restart
and failed-level restart each replace the Lua `objects.world`, retain the
native `g_restartType` value (`INGAME`, `COMPLETED` or `FAILED`) and finish with
the loading screen cleared. The completed and failed result screens were also
audited at their settled frames. They submit only result UI sprites and text;
no level geometry or world objects reach the renderer. Stella's result
animation remains intentionally present as UI.

IDA and Hopper agree that `sub_10006D9C0` draws two independent flight-trail
records in fixed order and divides their points by the GameLua physics/game
scale. The routine has no age, fade or timeout branch. Those records are
double-buffered and persist until later `startNewTrajectory` calls overwrite
them. They are distinct from the live AimStream used by the slingshot preview.
The recovered level-load path at `sub_1000086CC` clears and deactivates that
AimStream while leaving the flight buffers intact. Rust now applies the same
ownership boundary. Predictor dispatch also returns immediately when the
physics world is locked, preserving both the selected bird and the previous
prediction, rather than partially replacing one side of the preview state.

The shipped `LevelLoadTransition.lua` starts `Transition_Animation` at speed
1.5, performs the load after `PLAYBACK_END`, then starts
`Transition_Animation_Backwards` and disposes itself after the second
completion. The native event-track implementation now retains empty reset keys
and queues seek/start events with float32 timing. A direct transition route
observes both playback ends, removal of the transition child and all 16
`TRANSITION_LEAF_1/2/3` wgpu sprite submissions. The rendered PNG is only used
to force the GPU path; it is not treated as the behavioral oracle.

The most expensive avoidable host work in dense levels was structural rather
than a different physics rule. Continuous collision previously cloned each
complete `SceneObject`, including render-owned vectors and resources, just to
retain a sweep start. It now stores only center and angle. Contact-island DFS
previously rescanned every sorted contact and joint for every body; ordered
adjacency is now built once while retaining native creation order. Contact
velocity iterations now use compact body-indexed float32 velocity arrays and
commit at the same contact-pass boundaries, matching the native solver layout
without changing joint/contact interleaving.

Sprite-sheet group publication previously installed region metadata but left
PVR decoding to the first frame that happened to draw each texture. Native
SpriteSheet group loading constructs the corresponding GL textures during the
load transition. Active region and masked-texture sources are now sorted,
deduplicated and decoded when a catalog revision is applied. This deliberately
moves some total work into the covered loading phase; it is intended to remove
first-appearance gameplay spikes, not to claim a lower end-to-end route time.

All pause/completed/failed restart routes and the LEAVES route pass with the
current release code. The workspace passes 506 tests (75 app/audio/wgpu, 31
assets, one core and 399 script/physics). Formatting and strict
all-target/all-feature Clippy are clean. A 1,200-frame offscreen-wgpu audit of
the dense Chapter 1 level 50 route completes with 77 optional data probes,
zero invoked fallbacks and zero compatibility bindings; the PNG was not
inspected. Current release SHA-256 values are
`1e40996a48cff1c676568961cde2bba62a52151d609a6210b51945ee500bebfb`
for `stella-app`,
`6ea9679f3ed869873c0bb5741d78fd45ff64bed1c8944dc570275b1e25bb2f47`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Active animation controls, target precedence and the hidden start tick

A broader IDA/Hopper pass corrects the remaining single-control assumption in
the Rust animation host. `sub_100410A18` searches the Animation owner's active
control vector by action name. A missing action allocates and appends a new
control; an existing action is reset in place without changing vector order or
its retained speed. `sub_100410EA0` removes a named control with the native
swap-with-last vector idiom, detaches it from its targets, changes it to state
1 and seeks it to zero. Empty stop remains the distinct path that resets all
attached controls without removing them.

The EntityTarget vtable at `off_100AA4060` and its apply member
`sub_10041E41C` make vector order observable. Each target owns an ordered state
vector for each usage--translation, scale, rotation, alpha, sprite and zOrder--
and applies only the last state for that usage. A later action can therefore
override rotation while translation and sprite continue to come from an older
active action. `sub_1004111A4` visits every EntityTarget once and then copies
each active control's current time at `+0x2c` to its previous-time field at
`+0x30`. Shipped-script tracing confirms the multi-control case directly:
`Stella_1` starts `Stella_Idle` and then `Stella_idle_02` without an intervening
stop. Replacing the first playback with the second necessarily discarded
native per-property fallback and explained several partial bird/pig pose
mismatches.

This pass also supersedes the earlier claim in “Zero-duration controls and
exact state-3 advancement” that the `0.00001f` argument was ignored.
`sub_100012F18` passes that value in ARM64 `S0` to `sub_10040A998`; the apparent
argument disappears only from the decompiler's recovered integer signature.
The callee forwards it to `sub_100411230`, so start performs a real float32
0.00001-second Animation update followed by a mode-4 forced target apply.
Only after those two operations does the wrapper replace its shared action and
mode fields and install `sub_100016C50` as the selected control's completion
callback. Completion callbacks belonging to every active control read that
same latest wrapper mode rather than a per-control copy.

The two continuous timeline implementations at `0x100417A84` (float) and
`0x100417B0C` (float2) use float32 time/value arithmetic and fused multiply-add.
Their callers return the left key when the float32 span is at most `0.0001f`;
otherwise progress is `(time-left)/span`. An inventory of all 216 shipped
animation files finds only LinearFloat, LinearFloat2, DiscreteString and
DiscreteInt tracks, so these recovered paths cover every bundled timeline.

Rust now models a scene as the native ordered active-control vector plus the
wrapper's retained current pointer and shared mode. Rendering and entity
queries resolve each property independently from the last control that supplies
that track; named stop swap-removes it, empty/global stop retain the vector,
and detached-current pause/resume/seek behavior remains distinct. Start now
uses the native hidden tick/application/write/callback order, and all parsed and
sampled continuous timeline values follow the recovered float32 threshold and
FMA rules. Regressions pin stable control order, retained speed, per-property
fallback, named-stop fallback, the hidden initial tick, shared completion mode,
float32 parse quantization and the interpolation threshold.

The workspace passes 510 tests (75 app/audio/wgpu, 31 assets, one core and 403
script/physics). Formatting, strict all-target/all-feature Clippy and the full
release build are clean. A 1,200-frame offscreen-wgpu Chapter 1 level 50 audit
reports 77 optional data probes, zero invoked fallbacks and zero compatibility
bindings. A separate isolated-AppData 5,200-frame route clicks the real play
button, completes the seven-panel comic, asserts `Chapter01_L01`, and reports
85 optional probes, zero fallbacks and zero compatibility bindings. Both PNGs
were used only to force GPU readback, not as behavioral or visual oracles.
Current release SHA-256 values are
`792cb2b887c33f1a1427db4c65cf730cf4b0764828533e5d4e95c36cc756058b`
for `stella-app`,
`088c3066ba73af4e0215d572ec58422385785ee8a0d3b5f068238c6453fe864d`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Persistent EntityTarget component values after state detachment

IDA and Hopper expose an additional consequence of the target-state vectors.
EntityTarget's virtual member at vtable offset `+0x20`, `sub_10041E0C0`,
removes every state owned by a detached control. When a usage vector becomes
empty it removes that 48-byte ApplyCallback entry as well. The apply member
`sub_10041E41C` tests `states.begin == states.end` and simply skips the
callback; it does not invoke a default-value branch.

The named-stop order at `sub_100410EA0` is decisive. It first swap-removes the
control from Animation's active vector, then calls virtual `+0x20` on every
target, writes state 1, seeks the now-detached control to zero and applies the
remaining animation state. A property with another active state receives that
state's current value. A property whose final state was just removed receives
no setter call, so its Transform/Sprite component retains the last value that
was actually applied before stop. The EntityTarget constructor at
`sub_10041D82C` initializes its callback/state vectors empty, confirming that
an asset merely loaded but never started does not implicitly sample the first
action at time zero.

Rust now stores the last applied translation, scale, rotation, alpha, sprite
and zOrder per target. Each native apply pass updates these latches from the
last active state for each usage. Rendering and entity queries use an active
state when present and otherwise use the retained component value; they no
longer fabricate a first-action/time-zero fallback. A focused lifecycle test
loads an animation without starting it (identity transform and no sprite),
seeks a running control halfway, removes its final states, and verifies that
the halfway transform and discrete sprite remain latched.

The workspace passes 511 tests (75 app/audio/wgpu, 31 assets, one core and 404
script/physics). Formatting, strict all-target/all-feature Clippy and release
compilation are clean. The 1,200-frame Chapter 1 level 50 route still reports
77 optional probes, zero fallbacks and zero compatibility bindings; an
isolated-AppData 5,200-frame first-run route reaches and asserts
`Chapter01_L01` with 85 optional probes and the same two zero counts. PNGs only
forced the offscreen wgpu path. Current release SHA-256 values are
`3001d68ef0e9d82c5b0167ec2a526350ce159b972f960b98dc8f1d6110b6d196`
for `stella-app`,
`8f5ec7f4fb2dbc5faba36d709539e2de90e58769b3013ebd31463e25b0ea0199`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Flight-trail load lifetime and persistent wgpu streams

The earlier conclusion that level loading deliberately retained GameLua's two
flight-trail records was incomplete and is superseded here. The load member
`sub_100065D3C` calls `sub_1000675C0` at `0x100066228`, after erasing the old
scene indexes and before executing the requested level chunk. The callee
destroys the contents of the vector at GameLua `+0x558`, reconstructs exactly
two default 0x38-byte `TrajectoryData` records, and stores zero at `+0x588`.
This is independent from the later `AimStream::reset` boundary. The Rust load
path now resets both flight buffers, their normal/special sprite names and the
selected slot before opening either a bundle or AppData level. A loader
regression that previously asserted trail survival now pins the recovered
empty two-slot/index-zero state instead.

The draw member `sub_10006D9C0` remains unchanged: while a GameScene is drawn,
it submits both non-empty records without an age or fade branch. The fix is
therefore intentionally limited to the native level-load lifetime; it does not
invent a per-frame timeout. A fresh 16,000-frame shipped menu/L01/level-end
route clicks the real retry button, completes the LEAVES transition, returns to
`Chapter01_L01`, and finishes with no normal/special flight-trail submission.
The final level contains 641 sprite commands rather than a duplicate scene.

The user's visually reported preview/flight mismatch was also checked as
physics data rather than from a screenshot. The original Lua prediction table
and the live `flyingBird` positions from the same L01 shot are an exact
one-sample shift: actual sample 1 is the launch position, actual sample 2 is
prediction sample 1, actual sample 3 is prediction sample 2, and so on. This
matches the native predictor beginning after its first custom body step, so no
trajectory force, time-step or camera correction is applied.

Dense levels exposed a separate wgpu host cost. `render_game` cloned the full
1,088-byte-per-draw uniform vector, created a new storage buffer and bind group,
and created a new vertex buffer on every presented frame. Those objects are now
persistent streams initialized to 2 MiB of draw storage and 1 MiB of vertices.
Each frame uploads only current bytes with `Queue::write_buffer`; either stream
grows to the next power of two only when required, and only storage growth
rebuilds its bind group. A focused regression pins reuse, growth and the
no-shrink rule. This changes no draw order, shader record, pipeline, texture
binding, scissor or framebuffer operation.

Result-page command tracing at a stable ordinary L01 completion contains only
37 result UI/animation sprites and no native level object. The shipped
`LevelCompleted:isFullScreen` transition deliberately leaves gameplay under
the first 0.5-second clipped reveal and then suppresses it. The separate
Frenemies challenge background occlusion regressions remain passing, so no
blanket early scene clear was added. A direct original `LevelLoadTransition`
audit submits all 16 `TRANSITION_LEAF_1/2/3` components and retains an active
IN/LOAD/OUT phase at the sampled frame.

The workspace now passes 512 tests (76 app/audio/wgpu, 31 assets, one core and
404 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. The dense Chapter 01 L50 1,200-frame route completes
in 6.63 seconds including startup and final wgpu readback, with zero invoked
fallbacks and zero compatibility bindings. Current SHA-256 values are
`15e35fa52ddb893887630ed2522bf1953bf93ddffd51238d25a1553db382f1a7`
for `stella-app`,
`6d35e356cde7536ece831a4e1801bdc1ad1b73a37af091f41266e92abfd2351b`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Native screenshot capture/share lifecycle

The platform screenshot entry was the next locally implementable host gap.
The registration constructor stores `sub_10005ADD4` beside
`native_shareScreenShot` at `0x10002F2D4..0x10002F2FC`. IDA preserves the
member as 0x374 bytes and Hopper independently reports 884 bytes/38 basic
blocks. Both show the same order: increment the process-global 32-bit counter
at `0x100C0FF8C`, obtain `NSTemporaryDirectory()` through `sub_10053FEC4`,
format `Stella_Screenshot<counter>.png`, invoke the renderer virtual member at
`+0x1F0` with image format value six, and pass the resulting path plus the Lua
title string to `pf::ShareImpl::shareImageFile` through `sub_10053FF0C`.

The completion closure is not decorative. Its invoke member
`sub_10006E288` forwards the captured path to `sub_100502C8C`, which unlinks a
regular file, recursively removes a directory, and throws when the path is
missing or removal fails. The shipped `ScreenShotSharing.lua` confirms the
frame boundary: `share(title)` emits `EID_WILL_CAPTURE_SCREEN_NEXT_FRAME`,
then `process()` calls the native member once and clears its trigger/title.

Rust previously validated the title and set a Boolean that no host code ever
consumed. The script bridge now owns the wrapping native counter and queues
ordered `ScreenshotShareRequest` records with the exact basename and title.
The desktop and deterministic hosts drain those requests after the matching
Lua draw. The wgpu renderer exposes readback of the already-rendered game
target, so the shared PNG is the presented frame and does not traverse Lua or
submit the scene twice. The host writes the original basename into the system
temporary directory. Because a portable pure-Rust desktop host has no native
system share-sheet completion callback, it leaves the staged file pending for
the user or platform integration rather than falsely firing Purple's deletion
closure before anyone can consume it.

An isolated-`TMPDIR` real wgpu route invokes `native_shareScreenShot` from the
live shipped runtime and creates `Stella_Screenshot1.png` as a 1024x768 RGBA
PNG with SHA-256
`66a0de2d5696a9debddeac09903abf864e7d5168b3112fc60dbc060b7b72948c`.
The image was not used as a visual oracle; the audit establishes only the
native name, one-shot request consumption, GPU readback and file format. A
fresh 1,200-frame Chapter 01 L50 release route completes in 6.77 seconds and
still reports 82 optional probes, zero invoked fallbacks and zero remaining
compatibility bindings.

The workspace now passes 515 tests (78 app/audio/wgpu, 31 assets, one core and
405 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Current SHA-256 values are
`ce09fa882de3f4026bc318ac8de9595712477b8a3eaacaaacca64b57c408b91d`
for `stella-app`,
`a902ea84144151e5a0d7c25784ef6ab76bb3e7000bf8868bd1abe46c985fdee6`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Gamelogic load-completion and activation gate

The native GameLua loader `sub_10005CD58` does more than evaluate
`commonScriptPath .. "/gamelogic.lua"`. IDA shows a strict global call to
`updateValues` at `0x10005CEE8`, followed by the byte store to GameLua
`+0x513` at `0x10005CEF0`. Hopper independently confirms the same success
ordering. If lookup or execution of `updateValues` fails, the loaded byte is
therefore never published. This post-load call is separate from the call made
by shipped `initializeGameCommon`; preserving both is intentional.

Rust now models that byte explicitly. Standard boot evaluates the common
gamelogic, strictly resolves and calls `updateValues`, and only then marks the
runtime loaded. A failed callback propagates its Lua error and leaves the flag
clear. The shipped constants recovered from `game.lua`, including
`birdCollisionSoundForceThreshold = 40` and
`hardLimitSimultaneousParticles = 150`, are consequently established at the
same native boundary rather than only as an incidental later side effect.

The activation member `sub_10005D4D4` stores active state at GameLua `+0x510`
and reads the loaded byte at `0x10005D4F4`. Before that byte is set it suppresses
the native notification work and the Lua `gameResumed`/`gamePaused` callbacks.
The Rust lifecycle path now uses the same gate. Host input and hold-state
clearing still occurs before it, matching the surrounding application
lifecycle rather than leaving pre-load input latched.

Focused regressions pin successful call-before-publish ordering, failure
atomicity, shipped boot state, and pre-load/post-load activation behavior. A
fresh isolated-AppData release audit ran 600 frames through the real wgpu
readback path, asserted both recovered constants and a live menu root, and
reported zero invoked fallbacks and zero remaining compatibility bindings. The
PNG was used only to force and verify the render/readback route, not as a visual
oracle.

The workspace now passes 517 tests (78 app/audio/wgpu, 31 assets, one core and
407 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Current SHA-256 values are
`06d71613e0ae50b27e963b9ddf2ec60608be7c8d33fa3426152a2c3232202c9d`
for `stella-app`,
`e53200218d94944e9b3f273c5b946a328ccaf872ba2865bba14e18b211a142df`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Startup audio channel limits

The earlier startup reconstruction stopped too early inside
`sub_10005D44C`. IDA shows the strict `createStartUpAssets` call at
`0x10005D464`, but the function then resolves the AudioManager and calls
`sub_1005796C8` five times at `0x10005D478`, `0x10005D48C`,
`0x10005D4A0`, `0x10005D4B4` and `0x10005D4D0`. Hopper independently
decompiles the same track/limit pairs: `(1,4)`, `(2,6)`, `(3,3)`, `(4,5)`
and `(5,5)`.

`sub_1005796C8` forwards to `sub_100572E10`, whose store at AudioManager
`+0x40 + track*4` is the same eight-entry signed channel-limit array exposed
through `setChannelCountLimit`. These are audio concurrency limits, not asset
cache sizes or animation tracks. Slots zero, six and seven retain their
constructor value `-1`.

Rust previously called only the Lua startup callback, leaving every channel
unlimited. The host now writes the five native constants directly after the
callback returns. This ordering matters because the shipped callback creates
the audio output, which reconstructs AudioManager and resets all limits; it
also means a callback error must skip every native post-write. Focused tests
pin both callback-before-overwrite behavior and the failure boundary.

An isolated-AppData release audit loaded the shipped
`levels/Chapter01/Chapter01_L50.lua`, ran 1,200 deterministic frames through
the real wgpu upload/readback path, asserted the loaded filename, and reported
zero invoked fallbacks and zero remaining compatibility bindings. The final
1024x768 RGBA PNG was used only to force the renderer and was not treated as a
visual oracle.

The workspace now passes 519 tests (78 app/audio/wgpu, 31 assets, one core and
409 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Current SHA-256 values are
`3d5d05fa575dcbc283818f92542bde70c5f7aecf538149abefe4d61a6bc4827b`
for `stella-app`,
`15771cc86b74b909ca3dc6d563a7423b4246b3b81b858314c690185d8105d1bb`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Joint-removal notifications and break particles

The joint descriptor eraser was missing the observable callback boundary at
`sub_10006800C`. IDA shows a call to `lua_onBeforeJointRemove`, followed by a
fresh lookup of `objects.joints[name].isDrawn` and Lua 5.1 truth conversion;
only a truthy result calls `lua_addParticlesToJoint`. Hopper independently
decompiles the same order and reports exactly three callers:
`sub_100042260` (`removeObject`), `sub_1000442D8`
(`removeJointsFromObject`) and `sub_10007BA50` (collision-force breakage).

The surrounding order differs at those call sites and is now preserved.
Object destruction notifies and erases the descriptor before body teardown
destroys attached Box2D joints. Direct object-joint detachment notifies at
`0x100044600`, destroys the Box2D joint at `0x10004461C`, then erases its
descriptor at `0x100044640`. Automatic breakage copies the jointData record
to GameLua's pending vector at `0x10007BC94`, invokes the helper at
`0x10007BCB0`, compacts the Lua table at `0x10007BCEC`, and only destroys the
native joint in the later frame-tail pass. Explicit `destroyJoint` does not
call the helper; its separate `sub_10003E668` path erases the descriptor at
`0x10003E7D4` before the Box2D destruction at `0x10003E7F0`.

Rust now dispatches the two shipped callbacks while the Lua descriptor is
still live and re-reads `isDrawn` after the first callback, so component
notifications can alter whether break particles are queued. The object-removal
bridge consumes one object at a time, allowing zero-delay and delayed type-five
destruction links to cross the same callback boundary rather than recursively
deleting every native record under one mutex. `removeJointsFromObject` also
removes the Lua descriptors it previously leaked.

Regressions cover collision breakage, post-callback `isDrawn` mutation,
visible/hidden particle gating, direct object-joint detachment, `removeObject`
and a five-second destruction-link cascade. A 600-frame isolated-AppData
release audit created and removed a real native test joint inside the shipped
runtime, asserted the exact two-callback sequence and descriptor/object
retirement, and completed the wgpu readback path with zero invoked fallbacks
and zero remaining compatibility bindings. Its PNG was not used as a visual
oracle.

The workspace now passes 520 tests (78 app/audio/wgpu, 31 assets, one core and
410 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Current SHA-256 values are
`ff2238c2d3e6b16952c3a41e22d5f0c27bf9d8583dce0762ed25cce4274316db`
for `stella-app`,
`c2e44ccf2bc551f3e517aa9c4cff60c4e7b68a225a1f6aa9a3c3b4cef9e70577`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Disabled challenge replay and retained native contact-solver arrays

The remaining non-working restart route was specific to Frenemies challenge
screens. The shipped `FrenemiesChallengeLevelCompleted:onPointerEvent` does
not call `startChallengeLevel` directly. It first calls
`GameServerConnection.replayCompetitionLevel`, and only the asynchronous
success callback passes the returned `{levelId, players}` payload to
`IslandEventManager:startChallengeLevel`. The challenge pause screen has the
same dependency before it queues an ordinary restart.

IDA locates the GameServer constructor at `sub_1000D25D8`. It registers
`native_getAsync` and `native_postAsync`, publishes `_G.GameServerConnection`,
loads `commonScriptPath .. "/network/GameServerConnection.lua"`, and retains
the stage endpoint `https://stella-stage.appspot.com/api/v1`. Decompiling the
exact bundled Lua file resolves the apparent contradiction: every request
wrapper immediately executes `assert(false, "GAMESERVER-DISABLED")`. Thus an
exact copy of the shipped 1.1.6 networking path cannot restart a challenge,
even if the old endpoint still existed.

The offline host now installs a narrowly scoped script-side facade after the
original scripts load. Replay retains the original zero-delay asynchronous
callback boundary and synthesizes only the payload the shipped island manager
requires: the active challenge level id, the retained players, a current
player and an offline `gameSessionId`. No result-screen handler was replaced.
Focused regressions execute the shipped challenge result handler, verify that
the callback is not synchronous, and observe the original
`IslandEventManager:startChallengeLevel` entry with the preserved event and
payload. The other disabled legacy request members return benign asynchronous
offline results so their screens cannot dead-end on an intentionally disabled
backend.

The dense-level stall profile also exposed avoidable divergence inside the
contact velocity iterations. IDA and Hopper agree that `sub_100863FAC` and
`sub_1008640D0` retain two signed body indices into one compact array of
12-byte `(vx, vy, angularVelocity)` records for the whole island solve. Rust
already used compact records inside one pass, but reconstructed name/index
state and allocated a `BTreeMap<ContactKey, impulse>` on every iteration.
The island now retains one body-index table for the complete solve, refreshes
only its three live float32 values between joint/track passes, threads the
resolved index pair through tangent and normal impulses, and constructs the
keyed impulse map once after all iterations. Native joint/contact ordering and
commit boundaries are unchanged.

On the same 1,200-frame Chapter 01 L50 route, user CPU fell from 5.20 seconds
to 4.87--4.89 seconds, retired instructions from 90.03 billion to 85.82
billion, and cycles from 23.27 billion to 22.10 billion. Wall time remains
subject to shader, filesystem and scheduler noise. A final isolated-AppData
release audit directly loaded the shipped L50 file, ran all 1,200 deterministic
frames through wgpu upload/readback, asserted its filename, and reported zero
invoked fallbacks and zero remaining compatibility bindings.

Trajectory cleanup remains deliberately split at the recovered ownership
boundaries. `clearAimingAid(1)` removes every preview particle after launch,
whereas `sub_10006D9C0` draws the two most recent non-empty flight records with
no age, fade or timeout branch. A level load resets both records through
`sub_1000675C0`. The predicted and actual L01 coordinates remain identical
after accounting for the native one-sample launch-position shift, so no visual
offset or artificial lifetime was added. The LEAVES transition regressions
still submit all 16 components and complete IN/LOAD/OUT disposal, while stable
ordinary result pages submit only result UI and challenge result backgrounds
occlude the entire wgpu framebuffer. These checks use lifecycle state and draw
commands as their oracle rather than screenshot resemblance.

The workspace now passes 522 tests (78 app/audio/wgpu, 31 assets, one core and
412 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Current SHA-256 values are
`5eaa47cf6249da653db164d2612abece4c38cb84ba2282b6179d28eab5f04ac0`
for `stella-app`,
`d5cd5f0db7bce63c9bc2e9d5c5df1149a2fa68ecbdae1c46b690663b21607d45`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### LUA_GLOBALSINDEX directory stub and whole-bundle level coverage

The platform constructor `sub_10002C274` registers
`getDirectoryFileList` at `0x10002F214` through adapter `sub_100082F9C`
and member thunk `sub_10005A298`. IDA and Hopper independently recover the
same member call into `sub_100529C84`, whose second argument is the signed
constant `-10000` (`0xFFFFD8F0`). This is Lua 5.1's `LUA_GLOBALSINDEX`.
Consequently, the bundled implementation validates argument one as a strict
string, ignores that path, and returns a reference to `_G`; it does not list
files and does not create an empty table. The Rust binding now preserves that
exact identity, with regressions for both the strict argument check and
path-independent `_G` result.

The adjacent directory APIs remain literal stubs rather than speculative host
filesystem features. `createDirectory` reaches `nullsub_12`, while
`checkDirectory` reaches `sub_10004BAA0`, which returns zero. The existing
validated-string no-op and `false` implementations therefore already match
the executable. The same tail audit confirmed the no-op or false behavior of
`print`, `printWithTag`, `linkSensor`, `goToTaskSwitcherLua`, `sendTweet` and
`isTwitterSupported`.

A permanent whole-bundle regression now boots the shipped `scripts/game.lua`,
activates the original `INGAME` sprite group set, and enumerates all 149
shipped gameplay level files below `BirdRun`, `Chapter01`, `Chapter02` and
`minigames`. Every file is loaded through the original `loadLevel` entry,
advanced by one deterministic 60 Hz update and submitted through
`drawGameNative`. Each non-empty native sprite binding must resolve through an
atlas or composite, and every route must end with zero invoked fallbacks and
zero remaining compatibility bindings.

Final release audits additionally drove Chapter 02 L61 and BirdRun L08 for
600 frames apiece through real offscreen wgpu upload and readback. Both
asserted the requested shipped filename and retained zero invoked fallbacks
and zero compatibility bindings. The generated PNGs were used only to force
the GPU path, not as a visual oracle.

The workspace now passes 523 tests (78 app/audio/wgpu, 31 assets, one core and
413 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Current SHA-256 values are
`4ab0dbb7c7af4c3309c21db8dc6e82d3eb596255abdfcd397070e0c3c7d0bbed`
for `stella-app`,
`6bf915daef025cfea832a4e4f0d61261da332c8bcfeb689965214b471d5ec5e1`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### IOSOSInterface fullscreen state and device-orientation bridge

The platform tail of `GameLua::GameLua` still contained two host constants
that did not match Purple. The generated `isInFullScreenMode` adapter reaches
member `sub_1000310E4`, which loads `GameLua+0xD8`, follows the retained
`GameApp` and its `IOSOSInterface`, then dispatches virtual slot `+0x58`.
Purple's concrete iOS vtable `off_100AA3AB0` resolves that slot to
`sub_100405434`. IDA decompiles it as an unconditional return of one, and
Hopper independently reports `return 0x1`. The Rust binding now returns Lua
`true` instead of the previous speculative `false`.

The neighboring `native_getDeviceOrientation` binding was also not a literal
zero stub. IDA recovers member `sub_100051298` calling the retained
`gr::Context` virtual slot `+0x120`, rejecting enum values above three with
`-1`, and indexing `dword_1009AF090`. The table bytes are exactly the four
little-endian signed values `{0, 90, 180, 270}`. The concrete GL context member
`sub_100599458` returns the orientation enum stored at context `+0x310`, which
is initialized from the application configuration's UIKit orientation.

Rust now retains that native enum in the render/context bridge and performs
the exact Lua-facing table lookup, including the invalid-value result. A
desktop drawable has no UIKit physical-orientation sensor, so the host chooses
the first supported landscape enum for a landscape surface and the canonical
portrait enum for a portrait surface, updating it with drawable rotation.
This deterministic host policy is kept separate from the recovered native
enum-to-degree ABI. Regressions pin all four table values, the `-1` boundary,
landscape/portrait changes, the fullscreen boolean, and permissive generated
adapter argument handling.

Final release audits drove the shipped Chapter 02 L61 and BirdRun L08 levels
for 600 frames each through real offscreen wgpu upload and readback, asserted
their requested filenames and the landscape orientation result, and retained
zero invoked fallbacks and zero compatibility bindings. Their PNG outputs were
used only to force the GPU readback route and were not used as visual oracles.

The workspace now passes 524 tests (78 app/audio/wgpu, 31 assets, one core and
414 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. Current SHA-256 values are
`5d13599b5611c5fb6eec610376789fb6f23fb144143dc767451668d3e45e5494`
for `stella-app`,
`9d95721753b74ed5b2a80e4a7b510ef8c45d830ff31e3f57a63e789c38d3f54d`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Type-five destruction expiry and Lua component disposal

The reported `Missing object: pig_medium_right_3` draw failure exposed an
incorrect lifetime shortcut in the Rust type-five destruction-link bridge.
The strict `getScale` lookup remains intentional; `sub_1000403E4` still enters
the throwing name-map lookup `sub_10005DAF8`, so returning a default scale
would only conceal a dangling Lua draw component.

IDA's complete `sub_100042260` object-removal path shows that a metadata-only
type-five link does not recursively remove its target. At `0x1000432D0` it
looks up the target RenderObjectData, stores one at byte `+0x146`, copies the
descriptor's float `destroyTimer` into `+0xD8`, removes the link descriptor and
returns. In the native frame member `sub_10005E898`, this target is visited
only after the fixed Box2D loop, its shipped `removeBlocks` callback and
`clearLuaForceFunctions`. Addresses `0x10005F178..0x10005F190` test the byte,
subtract the scaled render delta from `+0xD8` and branch while the timer is
positive. On expiry, `0x10005F194..0x10005F1F4` resolves
`objects.world[name]`, writes a float zero to `strength`, and stores the
retained object table in the native-owned `deadBlocks[name]` table. It clears
the byte at `0x10005F1F8` without deleting the RenderObjectData. Hopper's
independent assembly reports the same offsets, floating subtraction, table
lookups, `strength` write and final flag clear.

The next fixed physics step consequently lets the original Lua `removeBlocks`
path dispatch `EID_DESTROY_BLOCK`, dispose block components and remove their
`DrawCalls` entries before it invokes native `removeObject`. Rust now follows
that two-stage boundary for both delayed and zero-delay links. Direct
`removeJointsFromObject` and ordinary object removal only arm linked targets;
the frame-expiry pass publishes them through the same `deadBlocks` helper used
by collision damage. Native scene objects, Lua world records and strict
transform queries remain live throughout the intervening draw.

Regressions cover zero-delay arming, five-second expiry, joint-callback order
across the delayed Lua removal, and the exact reported
`pig_medium_right_3 -> callback.func -> getScale` shape. The latter proves that
the callback can draw on the expiry frame and that the following
`removeBlocks` pass disposes it before object retirement. The workspace now
passes 525 tests (78 app/audio/wgpu, 31 assets, one core and 415
script/physics). Formatting, strict all-target Clippy and the release build are
clean. Fresh isolated-AppData release drives of the original BirdRun L01 and
Chapter 01 L05 bytecode each completed 3,600 offscreen-wgpu frames with zero
invoked fallbacks and zero remaining compatibility bindings. Their PNGs were
used only to force upload/readback and were not viewed as correctness oracles.

Current SHA-256 values are
`a966171993de9a10ec1be9b89e4150bd27cd5cee176fba60185bdef45021ad04`
for `stella-app`,
`199db5d4d1be6ca93dec3c5586499666e9f0fa6eeccc650a66349a6cc143c850`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Held Poppy/Luca power motion and fixed-step display sampling

The remaining apparent character-only frame loss during a held power was not
an animation-timeline stall. The shipped `PoppyAbility.lua` selects the exact
Flash action `Poppy_Power` and sets the aim multiplier to `0.1`; the shipped
`LucaAbility.lua` selects `Luca_ability` and sets it to `0.05`. IDA's
`sub_10005E898` confirms that the scaled delta feeds a fixed `1/30`-second
Box2D accumulator before the later Lua update and native scene draw. On a
60 Hz host, a direct Flash-root binding therefore repeats Poppy's last solved
body pose for about twenty display frames and Luca's for about forty while the
unscaled UI and other draw work continue normally.

The render bridge now retains velocity in its compact scene draw snapshot and,
only for those two exact held-power actions, evaluates a display pose at the
unsolved accumulator time with float32 fused multiply-adds. This is a
visual-only host sample: the Box2D transform, Lua `objects.world` position,
collision queries, forces and fixed-step ordering are unchanged. Ordinary
Poppy/Luca actions and every other Flash animation still use the solved native
pose. A regression drives both authored action names through the real
same-named Flash scene route, proves that both receive distinct intermediate
display coordinates, and proves that the underlying scene positions remain
untouched; the ordinary Flash-transform regression remains unchanged.

The workspace now passes 530 tests (79 app/audio/wgpu, 31 assets, one core
and 419 script/physics). Formatting, strict all-target/all-feature Clippy and
the release build are clean. Current SHA-256 values are
`822d15eb1f71f9afe37f64ebc3be90cc4f87f4f794831f8865d4f33fa548b59d`
for `stella-app`,
`acbf2ecd63897e9ee3280a1635b9c9a8f76e08956d88e4e4d5a2878e3175a17f`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Float32 display delta and repeated fixed-step subtraction

The fixed 60 Hz display-link result does not make the frame-time path double
precision. IDA shows `-[AppController update]` converting the signed
microsecond difference directly with `SCVTF S0` and `FMUL S0` at
`0x1004043CC..0x1004043D8`. Its long-frame branch writes immediate
`0x3DCCCCCD` at `0x100404598`, and the application virtual call retains the
value in the low single-precision lane. Hopper independently reports the same
`S`-register conversion, comparison and clamp sequence.

The callee `sub_10005E898` closes the rest of the numeric ABI. It copies the
incoming lane to `S8` at `0x10005E8C8`, saves the raw value at
`0x10005EC7C`, loads `deltaTimeMultiplier` from GameLua `+0x554`, performs one
single-precision `FMUL` at `0x10005EC84`, and later calls the typed
`lua::LuaObject::call<float,float>` member at `0x1000605A0`. The first Lua
number is therefore the widened result of the float32 multiply and the second
is the widened original float, not values recomputed in double precision.

The adjacent Box2D clock is also a float field, at GameLua `+0x51C`. Both
disassemblers recover immediate `0x3D088889` for the fixed step, the `FADD`
into that field at `0x10005ED6C`, the per-step comparison at `0x10005EDA0`,
and one addition of the negative step at `0x10005EDE4`. It is not equivalent
to dividing a double accumulator, flooring the quotient and subtracting one
multiplied total. In particular, an incoming `0.1f` executes two native
physics steps and leaves accumulator bits `0x3D088887`, just below a third
step; the former Rust path executed three immediately.

Rust now narrows the host clock once on entry, stores the multiplier and
physics accumulator as `f32`, performs the recovered float multiply, forwards
the two exact widened float values to Lua, and counts physics steps by the
native repeated comparison/subtraction. The Poppy/Luca held-power display
sample consumes this same float accumulator, so its visual extrapolation no
longer observes a higher-precision remainder than Box2D. Focused regressions
pin raw/scaled Lua values, the `0x3D088889` step, the two-step `0.1f` boundary,
and the residual bit pattern.

The workspace now passes 531 tests (79 app/audio/wgpu, 31 assets, one core and
420 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. A fresh isolated-AppData release drive ran the
original menu/comic/gameplay route for 16,500 frames, including the first
shot, next-level transition and held-power input, through real wgpu upload and
readback. It ended with 93 optional data probes, zero invoked fallbacks and
zero remaining compatibility bindings; its 750,523-byte PNG was used only to
force the GPU path and was not inspected as a visual oracle. Current SHA-256
values are
`56f23477eba799a30f6f2d394754753adf1f6460bdfc35643b8886a685407381`
for `stella-app`,
`efb09706e0103a851e12e413879e0c02e10e751eb38836cad98f3d48c44a16f2`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Native rolling material loops and per-frame joint endpoint export

The remainder of `GameLua::update` contained two shipped gameplay paths that
were still absent from the Rust host. IDA recovers the rolling-level pass at
`0x10005F688..0x10005F738`. It excludes controllable RenderObjectData
(`+0x140`) and every fixture without the circle flag at `+0x147`, reads the
absolute b2Body angular velocity, and performs four separate single-precision
operations: `angularSpeed * radius * 0.0025f * mass`, followed by `FMIN` with
one. The literal gain has bits `0x3B23D70A`. Material values one, two and three
select wood, rock and light respectively, and the pass retains the strict
per-material maximum. Hopper independently reports the same body offsets,
float instruction sequence, material switch and comparisons.

Addresses `0x10005FAA0..0x10005FD58` then maintain `wood_rolling`,
`rock_rolling` and `light_rolling` after the scene/joint export but before the
Lua `update` callback. A non-positive level stops every instance by resource
name without clearing GameLua's cached integer handle. A positive level first
checks playback by resource name; if already playing, volume is updated
through the cached handle, including the native stale-handle no-op. Otherwise
the resource starts looping on channel two and its new handle is cached.
GameLua stores rock, wood and light handles at `+0x2C8`, `+0x2CC` and `+0x2D0`.
The physics-locked branch supplies zero levels and therefore stops all three
loops. Rust now implements the same name-level ownership, persistent handle
lifetime and exact f32 formula against the shipped MP3 resources. A directly
constructed, unbooted host VM remains inert until AudioOutput exists; the
shipped boot creates and starts that output before its first normal frame.

The adjacent native joint walk is independently visible in both disassemblers
at `0x10005F944..0x10005FA98`. GameLua's `+0x3C0/+0x3C8` fields delimit an
insertion-order vector of 48-byte records. For every physical record it calls
the b2Joint virtual slots zero and eight to obtain both world anchors in
`S0/S1`, looks up the named descriptor in `objects.joints`, and then compares
the record field at `+0x20` with two. The creation path `sub_100037374` proves
that field is `coordType`, not joint `type`: it reads the `coordType` number at
`0x1000375B4`, rounds it through `FADD/FRINTM/FCVTZS`, stores it in the record's
`+0x20` slot, and appends that record to `GameLua+0x3C0`. Body-local
`coordType == 2` descriptors deliberately retain their authored coordinates
but still perform the strict descriptor-table lookup. Other physical
coordinate modes receive `x1`, `y1`, `x2`, `y2` in that order. Rust now stores
the coordinate mode in every native joint record, collects anchors through
the float32 b2Body transform in native creation order, excludes metadata-only
type five, preserves the body-local exception and skips the complete export
while physics is locked.

This distinction fixes the three drawn wheel joints in shipped
`Chapter02_L11`. They are revolute (`type == 3`) but body-local
(`coordType == 2`), with their first anchors at the centers of
`BLOCK_LIGHT_ROUND_4X4_1_1` through `_1_3`. Treating `+0x20` as joint type
overwrote those local fields with world coordinates every frame; the shipped
Lua drawing helper then transformed them a second time, leaving one axle
floating at the right edge and the other two offscreen. Retaining their
authored local fields keeps all three axle sprites centered in their circular
glass wheels.

Focused regressions pin the rolling maximum and literal bit pattern, all three
resource/handle branches, stale-handle behavior, locked-frame stop, ordinary
joint anchor widening, body-local retention, metadata exclusion, locked-frame
retention and the body-local missing-table error. The workspace now passes
642 tests (83 app/audio/wgpu, 31 assets, one core and 527 script/physics),
with one long-duration audit ignored by default. Formatting, strict all-target
Clippy and the release build are clean. A 180-frame release
drive used the shipped resource catalog and real offscreen wgpu path, started
the native `wood_rolling` loop, observed an ordinary joint descriptor update,
preserved the weld descriptor, and ended with 17 optional data probes, zero
invoked fallbacks and zero remaining compatibility bindings. Its PNG existed
only to force GPU upload/readback and was not inspected as a visual oracle.

Current SHA-256 values are
`287d3f08954830f24b95cc8f9aaca1de48252fbc3b1bba4b3ad3babecea120f9`
for `stella-app`,
`454a350cb7fd7a1ec264c266c84f26bf31a71838ed360df8eac11e83dfa87544`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Deferred native destruction of collision-broken joints

The collision RemovePredicate does not immediately call
`b2World::DestroyJoint`. IDA identifies `sub_10007384C` as the generic
`std::vector<GameLua::jointData>` append helper: it copy-constructs the three
strings, copies the two 64-bit fields at record `+0x18/+0x20`, copies the
native joint pointer at `+0x28`, and advances the end pointer by exactly
`0x30`. At `0x10007BC94`, the predicate passes its GameLua `+0x3F0` pending
vector to this helper. Hopper independently shows the same three string copy
constructors and the final 48-byte increment. The main jointData vector is
then compacted by `std::remove_if`, while the queued b2Joint remains linked to
the Box2D world.

The actual destruction occurs at `0x1000605D4..0x10006064C`, after Lua
`update(scaled, raw)` and the Particles virtual update. Both disassemblers show
GameLua reading `+0x3F0/+0x3F8`, deriving the 48-byte record count, walking
from the last entry to the first, erasing each queued record before loading
its `+0x18` b2Joint pointer, and finally calling `sub_10086E27C`
(`b2World::DestroyJoint`). The next instruction reloads the physics lock for
AimStream, so delayed joint destruction is strictly between particles and
the aiming-stream update. `sub_100062474` is the complementary destruction
listener: a body or explicit native teardown removes a matching b2Joint
pointer from the pending vector so the frame tail cannot destroy it twice.

Rust now keeps collision-broken joints native-solver-visible across every
remaining fixed step in the same display frame, while excluding them from
GameLua's logical attached-joint lookups and per-frame endpoint export. The
predicate scan and Lua removal callbacks follow native joint creation order;
the frame-tail native teardown reverses that queue, wakes both endpoints and
marks formerly suppressed contacts for filtering only at the real
DestroyJoint boundary. Body destruction also cancels queued entries through
the same ownership rule. Focused regressions pin logical invisibility,
continued solver ownership, export exclusion, no early wake, reverse teardown
and final wake behavior.

The workspace now passes 545 tests (79 app/audio/wgpu, 31 assets, one core and
434 script/physics). Formatting, strict all-target Clippy and the release
build are clean. A fresh isolated-AppData 180-frame release smoke test
completed real offscreen wgpu upload/render/readback with 17 optional data
probes, zero invoked fallbacks and zero remaining compatibility bindings. Its
21,470-byte PNG was used only to force the GPU route and was not inspected as
a visual oracle.

Current SHA-256 values are
`0e2c52335f2e4a7c751614926d6b574e691a5a26f994162d160da9c880681302`
for `stella-app`,
`55e99c12cabd8c614c7ccef245f7e7bedc7ecf0500113c7e5f31166f083c774f`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Independent additional trajectory gravity and native constructor colors

A constructor-field audit found a decimal/hexadecimal offset collision in the
former Rust model. IDA decompiles `sub_1000311AC` as one float store to byte
offset 548, which is hexadecimal `+0x224`; Hopper shows the literal
`STR S0, [X0,#0x224]`. The four-argument water-color member
`sub_100031198` instead stores to `+0x540`, `+0x544`, `+0x548` and `+0x54C`.
These fields do not overlap. The former implementation incorrectly treated
decimal 548 as hexadecimal `0x548`, coupling additional bird gravity to the
blue water-color component in both setter directions.

The use site at `sub_100032970` confirms the semantic effect. Before each
specialized BirdSimulation step, `0x100032B8C` loads GameLua `+0x224`, applies
the branch only when it is strictly positive, multiplies it by the simulated
body mass in float32 and adds the resulting vertical force. Water color is
never part of that path. The GameLua constructor stores `0xBF800000` at
`+0x224`, so the native default is `-1.0` and the force is disabled until the
explicit gravity setter enables it. The same constructor stores four `1.0f`
values in the independent water-color vector. It also initializes the packed
background color at `+0x238` to `0xFFFFFFFF`; the boot/theme scripts choose
their later scene color, so the pre-script Rust default is now white rather
than a host-selected sky blue.

Rust now stores the two settings independently and uses the exact `-1.0`
additional-gravity default. A regression sets additional gravity to `-1`,
then writes a blue water value of `9`, and proves that the zero-world-gravity
trajectory remains vertically stationary while both native fields retain
their separate values. Another pins all constructor defaults. All shipped
levels still construct, update and reach native draw with this corrected
state.

The workspace now passes 547 tests (79 app/audio/wgpu, 31 assets, one core and
436 script/physics). Formatting, strict all-target Clippy and the release
build are clean. A fresh isolated-AppData 180-frame release smoke test again
completed real offscreen wgpu upload/render/readback with 17 optional probes,
zero invoked fallbacks and zero remaining compatibility bindings; its
21,470-byte PNG was not inspected as a visual oracle.

Current SHA-256 values are
`3847b004d692e6754b12062391c3297f96fe2820bc2a6f07021837606f1d7c4d`
for `stella-app`,
`b804f297453198773e2870fe52203c183430f730f206bb8d6d65761a4169df56`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Two-stage physics-to-world scale initialization

The adjacent constructor audit also distinguishes GameLua's native default
from the value installed by shipped Lua. IDA shows `MOV W8,#0x3F800000`
followed by `STR W8,[X19,#0x50C]` at `0x10002C558..0x10002C55C`, so a newly
constructed GameLua starts with a physics-to-world scale of exactly `1.0f`.
Hopper reports the same literal store. The one-number
`setPhysicsSimulationScale` member later writes this field directly with no
range check.

The shipped `gamelogic.lua` `initParams` callback then assigns
`physicsToWorld = 20`, derives `physicsScale = 1 / physicsToWorld`, and calls
`setPhysicsSimulationScale(physicsToWorld)` before `initializeGame`. Thus the
normal level value remains 20, but the interval between native construction
and script initialization is observably 1 rather than a host-preselected 20.
Rust now preserves those two phases. One regression pins the bare constructor
value and the whole-bundle boot regression pins the post-`initParams` value
before constructing all 149 shipped level files.

The same field sweep identifies GameLua `+0x524` as the immutable `10.0f`
collision-force divisor used by all three branches of `sub_100062520`; the
existing Rust collision paths already use that exact float32 divisor. It also
closes a tempting but incorrect host interpretation of `setGameOn`:
`sub_1000504A8` forwards `enabled ^ 1` through IOSOSInterface virtual slot
`+0x30`, and the concrete iOS slot `sub_100405390` only calls
`UIApplication.setIdleTimerDisabled`. It does not gate GameLua update or
Box2D stepping, matching the rehost's separation between `setGameOn` and the
real physics lock.

After this two-stage correction, the workspace still passes all 547 tests
(79 app/audio/wgpu, 31 assets, one core and 436 script/physics). Formatting,
strict all-target Clippy and the release build are clean. A fresh isolated
AppData 180-frame release smoke test completed real offscreen wgpu
upload/render/readback with process exit 0, 17 optional probes, zero invoked
fallbacks and zero remaining compatibility bindings. Its 21,470-byte PNG was
only used to force the readback path and was not inspected as a visual oracle.

Current SHA-256 values are
`2bc85e3831c81846216c97ef0d671bcbdb854b6082a134ae946e859435120bcc`
for `stella-app`,
`f2c65700b3fd6f4ba6a96330b5ce7ee4830b7dfb4fe8ca9bc5268134903d0188`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### LEAVES hierarchy and final-vertex float32 arithmetic

The remaining transition drift was below the asset/timeline layer. IDA renders
each row of `sub_10001E440` as float32 products and a sum, but that decompiler
expression does not preserve ARM64 contraction. Hopper's instruction listing
resolves the exact order: Purple rounds the second product with `FMUL`, feeds
that value to `FMADD` as the addend of the first product, then adds the parent
translation with a separate `FADD`. The former Rust path retained parsed
float32 values in an f64 matrix and did the hierarchy composition in f64; an
intermediate correction that rounded both products separately was also not
instruction-accurate. Rust now reproduces the mixed `FMUL`/`FMADD`/`FADD`
sequence in float32.

The final sprite path uses the same ordering. At `sub_100467BE8`, each of the
four vertices first computes the second matrix term with `FMUL`, combines the
first term with `FMADD`, and only then adds float32 translation with `FADD`.
This matters for cancellation operands: Purple retains exactly `2^-46` where
rounding both products first produces zero. Ordinary animation construction
continues to use `sincosf`-equivalent f32 inputs and f32 scale products;
parent/child, skin, composite and final wgpu vertex transforms now share the
native mixed fused order instead of f64 or all-non-fused approximations.

The wrapper scene setters were corrected at the same boundary. IDA shows
`setTranslation` (`sub_1000145FC`), `setRotation` (`sub_1000147B8`) and
`setScale` (`sub_100014978`) resolving an existing scene before mutating its
float matrix; an unknown tag only warns and never creates a transform.
Rotation installs a new unit basis, discarding prior scale, while scale
normalizes and rescales the existing basis. Rust now quantizes all setter
arguments to float32, ignores unknown scenes, resets scale on rotation and
retains orientation when scaling. The shipped LEAVES path calls translation
then scale, so its screen-centred root now follows the native mutation and
precision order exactly. The final `world_space` audit also confirms that the
animation root already carries screen coordinates and must not pass through
the gameplay camera a second time.

The workspace passes 550 tests (81 app/audio/wgpu, 31 assets, one core and
437 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. A fresh isolated-AppData 180-frame release smoke
completed the real offscreen wgpu upload/render/readback path with process
exit zero, 17 optional data probes, zero invoked fallbacks and zero remaining
compatibility bindings. Its 21,470-byte PNG was retained only to force the GPU
route and was not inspected as a visual oracle.

Current SHA-256 values are
`2ff30db80d3c956b7512d54cfa218cabe0e31876810ff4a91766147f0f550602`
for `stella-app`,
`229d3c4fb7e090a25c34f38239af9ca4a35caf61369f426718c529d478265418`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Fixed 60 Hz display link, not adaptive refresh

The original iOS host does not implement adaptive frame rate. IDA decompiles
the `Configuration` constructor at `sub_100401398` with a literal 60 stored in
its `framerate` member at `+0x48`. The title-specific configuration callback
`sub_10002A278` only supplies orientations, the renderer, and bundle directory
names; it does not replace that value. Hopper independently shows
`ORR W9,WZR,#0x3C` followed by `STR W9,[X0,#0x48]` in the constructor.

On activation, `-[AppController startUpdate]` at `0x100404CA8` constructs one
`CADisplayLink`, computes integer `60 / configuration.framerate`, and passes
the result to the legacy `setFrameInterval:` selector. With the shipped value
60, the interval is exactly one display refresh. IDA and Hopper both show the
`SDIV` by the stored configuration member and the subsequent selector call.
There are no `preferredFramesPerSecond`, `preferredFrameRateRange`, or
`maximumFramesPerSecond` selectors in the executable, and no runtime branch
changes the interval according to load, device refresh capability, or frame
time. Resuming the application destroys and recreates the same fixed-interval
display link.

`-[AppController update]` measures the real monotonic interval for every
callback, narrows it to float32, and clamps only negative values to zero and
stalls above 0.1 seconds to 0.1. That variable delta drives animation and Lua;
the separate GameLua accumulator continues to step Box2D at 30 Hz. This is
variable-delta simulation on a fixed 60 Hz callback, not adaptive refresh.
The Rust desktop host's 16,666,667 ns display-link deadline and measured,
0.1-second-clamped frame delta therefore remain the faithful default. Driving
presentation at 120 Hz is technically possible as an optional rehost feature,
but enabling it as native behavior would diverge from Purple 1.1.6 and is not
done here.

### Authoritative wrapper matrices and scene-relative query round trips

The wrapper scene state is now retained as Purple's six float32 matrix rather
than reconstructed from host-side translation, angle and scale values during
draw. IDA and Hopper agree that `setTranslation` (`sub_1000145FC`) copies the
current matrix and overwrites only its two translation members;
`setRotation` (`sub_1000147B8`) replaces the complete linear basis with the
raw `__sincosf_stret` result and therefore discards any prior scale; and
`setScale` (`sub_100014978`) normalizes the two current basis columns through
`sub_10057B644` before multiplying them by the requested float32 scales. That
normalizer computes its squared length with `FMUL` followed by `FMADD`, uses
`FSQRT`, rejects lengths below `FLT_MIN`, then applies `FDIV`/`FMUL` in
float32. The Rust wrapper now mutates an authoritative matrix in exactly this
order. A parallel scalar record is retained only for compatibility render
metadata where a signed reflection such as horizontal scale `-1` is
observable; it no longer determines sprite geometry.

The entity-query path also performs a real matrix round trip. In
`sub_10000F46C`, Purple inverts the root scene matrix, composes an entity world
matrix, then evaluates `inverse(scene) * entityWorld` through
`sub_10001E440`. The inverse determinant and translated terms preserve their
ARM64 rounded-product/fused-add ordering. Position comes directly from the
resulting float matrix, scale uses `FMUL`/`FMADD`/`FSQRT` column magnitudes,
and angle is `atan2f(m10, m00)`. This is observably different from returning
the authored scalar values: a synthetic root at `(500, 600)`, scale
`(10, 20)` and angle `0.25f` round-trips a child to x
`19.999996185302734` (`0x419ffffe`), y `41.0`, scale x
`0.9999999403953552` (`0x3f7fffff`), scale y
`5.999999523162842` (`0x40bfffff`) and angle
`4.968269795568858e-9` (`0x31aab55d`). Regressions now pin these residues.

`sub_100015000` uses the same float32 magnitude decomposition for local scale.
`sub_1000152BC` derives world bounds from the scene-relative matrix, reads
sprite width and height as signed 16-bit integers through
`sub_100467E14`/`sub_100467E1C`, multiplies each converted dimension by
`0.5f` before applying scale, and finishes its bounds with float32
`FADD`/`FSUB`. Rust now follows that instruction order as well. Together with
the previously corrected final-vertex `FMUL`/`FMADD`/`FADD` path, animation
hierarchies such as LEAVES no longer mix a native float32 draw matrix with
idealized host-side query values.

The workspace passes 551 tests (81 app/audio/wgpu, 31 assets, one core and
438 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. A fresh isolated-AppData 180-frame release smoke
completed real offscreen wgpu upload/render/readback with process exit zero,
17 optional probes, zero invoked fallbacks and zero remaining compatibility
bindings. Its 21,470-byte PNG was used only to exercise GPU readback and was
not treated as a visual oracle.

Current SHA-256 values are
`fa98307577aabba1f91b16cbdeb5fda781c92331d81a5aa7668a07c78d1ede6f`
for `stella-app`,
`47332da72b7c2f1dc3d093b60421a08ddb81bb65f7c8b182bfe576e3892f77e1`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Recursive animation world matrices and mirrored-parent inheritance

The remaining LEAVES motion drift came from collapsing an authored hierarchy
to one host-side transform before drawing it. Purple never performs that
collapse. `AnimationWrapper::draw` at `sub_1000144F4` forwards its scene and
camera to `sub_10042F53C`; the latter refreshes the scene renderable array,
then submits its entries in the order produced by `sub_10042F3F0`.
IDA and Hopper both show a stable sort whose comparator at `sub_10042F4F4`
returns `left.z > right.z`. The existing descending LEAVES order was therefore
retained, while every `root -> LEAF_n -> SLOT_TRANSITION_LEAF_n` matrix is now
composed separately with the recovered float32 `FMUL`/`FMADD`/`FADD` ordering.
The screen-centred wrapper matrix is the first parent rather than a transform
reapplied after the authored chain. This changes the actual IN and OUT paths,
rotations and squash matrices, rather than applying a screenshot-derived
offset.

The same traversal also closes Purple's reflected-parent rule. At
`sub_10043C758`, `Transform::GetWorldMatrix` compares the sign of the parent's
world determinant with a retained descendant-reflection byte. On a mismatch
it measures the local basis, obtains its angle with `atan2f`, applies a signed
twice-angle correction between inverse-scale and restored-scale matrices, and
only then composes the parent world matrix. `sub_100010E2C` marks the exact
`SLOT_` prefix through `sub_10043CAF0`; those attachment nodes skip the
correction. `AnimationWrapper::setScale` at `sub_100014978` propagates the
float32 `(scaleX * scaleY) < 0` state recursively, whereas `setRotation` does
not clear it. Rust now retains that state per live scene and applies it at each
non-slot hierarchy step. This is immediately relevant to the 38 shipped
animation assets containing negative scale keys, without disturbing LEAVES'
intentional negative skin-attachment scales.

The shipped LEAVES regression now covers both `Transition_Animation` and
`Transition_Animation_Backwards` at their 0.4-second midpoints. It pins all 16
leaf submissions and exact float32 positions/matrices for representative back,
middle and front layers, in addition to the existing duration, settled matrix
and layer-order assertions. The workspace passes 554 tests (81
app/audio/wgpu, 31 assets, one core and 441 script/physics). Formatting,
strict all-target/all-feature Clippy and the release build are clean.

Fresh isolated-AppData real-wgpu runs exercised ordinary startup plus the
original LEAVES IN and OUT paths. Both transition captures completed with zero
invoked fallbacks and zero remaining compatibility bindings; their PNGs were
used only to force texture upload, rendering and readback, not as the
behavioral oracle. Current SHA-256 values are
`f177687492dd19b939c9cf8da79c86b082632a112220c3960101593b547ce6ae`
for `stella-app`,
`9466a57d3f616ab9c96591b4e6001856cd4497c7cd6aae3225149434b0e061de`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Discrete native sprite rebinding without host geometry morphs

The post-LEAVES approximation audit found one remaining host-only animation
patch in the ordinary scene-object path: `POPPY_POWER_1` through
`POPPY_POWER_6` retained their authored sprite timeline, but the Rust host
inserted one display frame whose width, height and pivot were the arithmetic
mean of the current and following atlas regions. This attempted to smooth a
30 Hz texture sequence, yet it changed the character outline and had no
corresponding native state.

IDA's complete decompilation of `sub_10004C7FC` and Hopper's independent
40-block pseudocode agree on the actual `native_setSprite` member. It resolves
the requested composite or atlas sprite, writes the live resource pointer,
removes the object name from the previous `(integer z, sheet)` vector when the
sheet changes, inserts it into the new vector and assigns the requested string
to RenderObjectData `+0x68`. It never derives a following sprite name, reads
two sets of dimensions or pivots, maintains a one-frame age, or interpolates
geometry.

The host-only override fields and frame-age pass have therefore been removed.
Poppy's power frames now change as discrete native resource rebindings, while
the separately recovered unsolved-Box2D display-position sample remains in
place for held Poppy and Luca powers; position continuity is preserved without
inventing a morph frame. A regression holds `POPPY_POWER_1` across a display
frame and then switches to `POPPY_POWER_2`, proving that both submissions use
their bound atlas geometry with no draw-size or pivot override.

The workspace passes all 554 tests (81 app/audio/wgpu, 31 assets, one core and
441 script/physics). The full run also exposed a test-only collision between
parallel temporary sprite-sheet files; their names now combine the process ID
with an atomic sequence. Formatting, strict all-target/all-feature Clippy and
the release build are clean. A fresh 180-frame real-wgpu upload/render/readback
smoke test completed at 1024x768 with process exit zero, 17 optional data
probes, zero invoked fallbacks and zero remaining compatibility bindings. Its
PNG was used only to exercise the GPU route and was not treated as a visual
oracle.

Current SHA-256 values are
`381bc31c64a00d515dd4348f38e765d33b221ccf90619545428a64dcc9fa5684`
for `stella-app`,
`dbb6ca474dd4dbd0f11349f1d33cc700c1be6a81287636c0db0fc766d7f12bc6`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Uniform atlas-sprite matrices and removal of the tutorial-name shear

The next render audit found one resource-name special case below the Lua and
native resource layers. `render_command_transform` recognized
`TUTORIAL_TARGET` and post-multiplied its ordinary transform by a hard-coded
local shear of `0.1316`. This had been introduced from a visual inspection to
level one edge of the authored triangle, but it also distorted the internal
pixel art and had no counterpart in Purple.

The shipped script and resource evidence is complete. Constructing
`ui.TutorialTapArea` from the original Lua bytecode creates `arrowTop`,
`arrowBot`, `arrowLeft` and `arrowRight` with angles exactly `0`, `pi`,
`-pi/2` and `pi/2`. All four use the same `TUTORIAL_TARGET` token. The
original `INGAME_BLOCKS_MISC_2.dat` describes it as an ordinary 147x164 atlas
region with pivot `(73,82)` and contains no transform metadata.

IDA has no `TUTORIAL_TARGET` string in the executable. Its complete
decompilation of Sprite's four-vertex member `sub_100467BE8` applies the same
incoming six-float transform to every vertex and forwards the resulting
triangle strip; the only branch selects the generic matrix helper when the
matrix flag at `+0x30` is set. Hopper independently shows the four identical
`FMUL`/`FMADD`/`FADD` blocks and only the generic transform/submission callees
`sub_10057B748`, `sub_10057B8A0` and `sub_10046B1F0`. Neither implementation
can inspect an atlas-region name at this stage.

The name check, shear constant and correction function have therefore been
removed. A regression now submits identical state under `TUTORIAL_TARGET` and
an arbitrary atlas name and pins all seven resulting transform scalars as
bit-identical. The source pixels, native pivot and cardinal Lua rotations are
again the only inputs to the tutorial triangles.

The workspace passes all 554 tests (81 app/audio/wgpu, 31 assets, one core and
441 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. A fresh 180-frame real-wgpu upload/render/readback
smoke completed at 1024x768 with process exit zero, 17 optional data probes,
zero invoked fallbacks and zero remaining compatibility bindings. Its PNG was
used only to force the GPU route, not as a behavioral oracle.

Current SHA-256 values are
`b9997c406d39260a65ddeec820a120e04196ad9e423fff1033a60edf65407dae`
for `stella-app`,
`dbb6ca474dd4dbd0f11349f1d33cc700c1be6a81287636c0db0fc766d7f12bc6`
for the unchanged `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Native submission of near-degenerate atlas matrices

The generic atlas path still contained one host-side visibility threshold:
`append_gpu_region` discarded a command whenever the absolute determinant of
its float32 linear matrix was below `f32::EPSILON`. Two non-zero scales of
`0.0001` produce determinant `1e-8`, so a large sprite animated through a
small scale could disappear for host-selected frames instead of shrinking
continuously.

The threshold is absent from the complete native call chain. IDA reports five
callers of Sprite's four-vertex member `sub_100467BE8`:
`sub_10004DB90`, `sub_10006C838`, `sub_100095A4C`, the forwarding thunk
`sub_100467BE0`, and `sub_100469138`. Their decompilations construct or obtain
the incoming transform and call the member directly; their only early tests
check required resource pointers. `sub_10006C838` builds the direct-sprite
matrix and all four local corners before submission, while the two animation
component members obtain the live world matrix and submit it unchanged.

Hopper independently shows the same caller set and no determinant magnitude
comparison. Inside `sub_100467BE8`, each of the four vertices executes the
recovered float32 `FMUL`/`FMADD`/`FADD` sequence and is forwarded to the
renderer. The sole per-vertex branch selects `sub_10057B8A0` for the generic
matrix flag; it is unrelated to scale magnitude. Purple therefore leaves
near-degenerate triangles to the GPU rasterizer.

The determinant epsilon cull has been removed from the wgpu atlas builder. A
new GPU-boundary regression submits a 1000x1000 sprite at scale
`(0.0001,0.0001)`, pins its 0.1x0.1-pixel transformed corners, and verifies
that the prepared frame still contains six vertices and one draw. This closes
a generic source of intermittent missing sprites during small-scale animation
without inventing a minimum visible size.

The workspace now passes all 555 tests (82 app/audio/wgpu, 31 assets, one core
and 441 script/physics). Formatting, strict all-target/all-feature Clippy and
the release build are clean. A fresh 180-frame real-wgpu upload/render/readback
smoke completed at 1024x768 with process exit zero, 17 optional data probes,
zero invoked fallbacks and zero remaining compatibility bindings. Its PNG was
used only to execute the GPU route.

Current SHA-256 values are
`7b3da0bffccd614041f9973a7b5a6fcbe1a781f4bfb722bd54f45944a4637aa6`
for `stella-app`,
`dbb6ca474dd4dbd0f11349f1d33cc700c1be6a81287636c0db0fc766d7f12bc6`
for the unchanged `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Float32 scene-object transform boundary

The next ordinary-object pass compared `sub_10006D5B4` and
`sub_10006C838` instruction-for-instruction in both IDA and Hopper. The
former loads all camera, object and game-scale values into scalar `S`
registers. The latter computes the submitted origin as
`((position * 20.0f) - topLeft) * worldScale`, using a separate `FMUL`,
`FSUB`, then `FMUL`. Its X basis scale is `(flip * worldScale) * objectScaleX`
and its Y basis scale is `worldScale * objectScaleY`. The old bridge retained
the float-backed values as `f64` and collapsed these expressions before the
wgpu boundary, which changed the last bits of screen origins and small object
scales.

The call at `0x10006D8F4` also settles the two object-angle fields. The
ordinary sprite matrix receives only `RenderObjectData+0xAC`; the separate
`+0xB0` written by `setSpriteRotation` is not added to that explicit matrix.
The surrounding live callback context is different: `0x10004C0E8` installs
`+0xAC + +0xB0` with one float32 `FADD` when the object is not mirrored, while
the mirror branch at `0x10006D640` replaces it with `FNEG(+0xAC)` and drops
`+0xB0`. Rust now preserves both paths instead of adding the visual rotation
to every ordinary sprite and computing the callback basis in host double
precision.

This correction also exposed an outdated ray expectation. `makeRay` retains
its captured fixture polygon, but `drawGameNative` takes the distinct
DrawablePolygon branch at `0x10004C14C`; `sub_10008D428` reads the live context
basis, so a later `setRotation` rotates that retained polygon. Its regression
now pins the float32 rotated bounds rather than an axis-aligned host result.

A new adversarial circle regression covers float32 camera subtraction,
physics-to-world multiplication, independent scale order, ordinary-object
angle selection and the non-mirrored callback angle. The workspace now passes
all 556 tests (82 app/audio/wgpu, 31 assets, one core and 442 script/physics).
Formatting, strict all-target/all-feature Clippy and the release build are
clean. A fresh 180-frame real-wgpu upload/render/readback smoke completed at
1024x768 with process exit zero, 17 optional data probes, zero invoked
fallbacks and zero remaining compatibility bindings. The PNG was used only
to execute the GPU route.

Current SHA-256 values are
`da993c09ed54ef1b9b5ae4fe4e39e2810899859103eecec01f85746c0749e05f`
for `stella-app`,
`3cbcc410ddef3e82484ca36fab960f41e6ebab64bb7b22bdee5dc0f93713cb39`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Native scene callback pivots and CompoSprite integer bounds

IDA and Hopper independently confirm that `sub_10004BAB4` installs a live
rotation pivot before entering an object's draw callback, and that its two
resource branches are deliberately different. At `0x10004C070`, the ordinary
atlas branch reads the signed 16-bit Sprite pivot at `+0x30/+0x32`, converts
each component to float32 and adds `RenderObjectData+0xB4/+0xB8`. The
CompoSprite branch at `0x10004C020` instead reads the signed 32-bit pivot at
`+0x68/+0x6C` and never adds those object offsets. The pair is written to the
GLES2 context at `+0x30`; `sub_100598CC4` later consumes it through the
float32 `(I-R) * pivot` translation terms. Keeping the Rust callback pivot at
zero therefore rotated callback attachments around the wrong point.

The composite values are not static authoring metadata. Both disassemblers
show `sub_100436D40` visiting every visible atlas part, normalizing the two
rotation basis vectors through `sub_10057B644`, composing translation,
rotation, scale, flip and the negative atlas pivot through `sub_10001E440`,
then transforming all four raw sprite corners. Every transformed coordinate
is truncated with `FCVTZS`; the member stores `max-min` as size and `-min` as
its integer pivot. The Rust bridge now reproduces the same float32
`FMUL`/`FMADD`/`FADD` staging, normalization guard, signed truncation and
wrapping integer arithmetic in a separate reverse-aligned `pivot` module.

Two regressions pin the branch distinction: an atlas callback combines a
resource pivot with its object offset, while a transformed/flipped composite
derives `(4, 7)` from integer bounds and ignores a deliberately large object
offset. The workspace now passes all 557 tests (82 app/audio/wgpu, 31 assets,
one core and 443 script/physics). Formatting, strict all-target/all-feature
Clippy and the release build are clean. A fresh 180-frame real-wgpu
upload/render/readback smoke completed at 1024x768 with process exit zero, 17
optional data probes, zero invoked fallbacks and zero remaining compatibility
bindings. The PNG was used only to execute the GPU route.

Current SHA-256 values are
`cb734f0b52dd8d2d8548ff08e3aae1d0e19e3a5c3479ce891f3cf95ffd3cd01c`
for `stella-app`,
`f4c9ec98a5bc054c980a2ad14d4d9c81d544280eaf57e1a104e7e55f6ab9aa93`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### SpriteComponentCustom atlas-pivot anchor and LEAVES quad repair

The LEAVES audit exposed a conflict between an earlier screenshot-derived
center-pivot inference and the component implementation. IDA's complete
SpriteComponent constructor at `sub_100467F5C` resolves it directly. At
`0x1004681A8/0x1004681B0` the default `game::Anchor` passed to the registered
`anchor` property is `{4,3}`. `sub_100095C24`, the SpriteComponentCustom
constructor used by AnimationWrapper slots, calls that base constructor and
only installs its derived vtables plus the `1.0f` member at `+0x200`; it never
changes the anchor.

Both disassemblers independently show how the default is consumed.
`sub_1004686FC` rebuilds the component's four local vertices after
`sub_100469030` changes its Sprite pointer. Vertical mode 4 reaches
`0x100468740` and uses `sub_100467E2C` (signed SPRT pivot Y), while horizontal
mode 3 reaches `0x100468928` and uses `sub_100467E24` (signed SPRT pivot X).
`sub_100467E14/E1C` supply the signed width/height for the opposite edges.
The AnimationWrapper sprite/skin callback `sub_100011BC4` calls
`sub_100469030` and installs the recovered attachment matrix, but contains no
anchor write or center conversion. Hopper reports the same constructor
literal, switch cases and caller set.

The shipped `MENU_ELEMENTS_1.dat` records are deliberately not exact
half-size pivots: `TRANSITION_LEAF_1` is 439x272 at `(220,136)`, leaf 2 is
371x327 at `(186,163)`, and leaf 3 is 315x254 at `(158,126)`. The former Rust
override instead used `(219.5,136)`, `(185.5,163.5)` and `(157.5,127)`. Skin
rotation and non-uniform scale amplified those offsets into moving seams and
incorrect overlap during the transition.

Animation render commands leave `sprite_pivot` unset, so the retained SPRT
record supplies the same signed pivot as the base SpriteComponent. This is one
stage of the native path; the derived draw member recovered below supersedes
the earlier conclusion that this stage alone determined the final quad.

The workspace still passes all 557 tests (82 app/audio/wgpu, 31 assets, one
core and 443 script/physics). Formatting, strict all-target/all-feature Clippy
and the release build are clean. A real-wgpu 180-frame audit preloads the
shipped MENU group, runs the original LevelLoad IN -> LOAD -> OUT state
machine with a no-op load callback, asserts that the transition child disposes
at completion, and reports zero invoked fallbacks and zero remaining
compatibility bindings. Its final readback SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
the PNG is only a GPU/lifecycle audit, not the behavioral oracle.

Current SHA-256 values are
`e2c4a8a686de32f553ae983bf138bf09c575859d27a16163229bbf9f1c138c4d`
for `stella-app`,
`fbadfda36e37fde9690442c13d8353c8a809b87a576b46987946c390e51f0c45`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### SpriteComponentCustom derived centering and atlas-only pointer binding

The preceding constructor/rebuild audit was incomplete because it stopped
before the derived virtual draw. IDA and Hopper both recover
`sub_100095A4C` as a 400-byte, 100-instruction member. It copies the Transform
world matrix from `+0x70`, updates renderer alpha from the component's `+0x200`
member only when the absolute difference exceeds the native approximately
`1e-5` literal, then computes local X/Y as `pivot + (-size) * 0.5` with the
original `FMADD` staging. It composes that translation after the world matrix
with `sub_10001E440` and submits the base SpriteComponent vertices through
`sub_100467BE8`.

This means both apparently conflicting observations are native: the base
vertices retain the signed atlas pivot, while SpriteComponentCustom appends
`pivot - size/2`. They algebraically centre the raw quad, but must remain two
float32 stages because rotation, shear and composition rounding make the
intermediate representation observable. Rust now snapshots exact integer
width/height/pivot metrics with each animation attachment and post-composes
the recovered translation without replacing the retained atlas-pivot vertices.
The real LEAVES IN/OUT midpoint regressions pin the resulting float bits and
all 16 layer submissions.

An audit of all 216 shipped `.anim.json` assets against every loaded
`*COMPOSPRITES.dat` name found one animation attachment that is not an atlas
sprite: `GENERAL_UI_BG` in `TELEPOD_PAGE_LEAVES.anim.json`. A deeper caller and
type audit reverses the earlier inference that the component could retain that
CompoSprite. IDA and Hopper both show `sub_100469030` storing its input directly
at SpriteComponent `+0x188`; `sub_100011BC4` supplies only the resolved
DiscreteSprite/skin AtlasSprite pointer and passes zero on every failed lookup.
In `sub_100095A4C`, the load from `+0x188` is followed immediately by `CBZ` to
the epilogue. Its non-null path calls only the AtlasSprite width, height and
pivot getters before `sub_100467BE8`. There is no CompoSprite type branch or
call to the separate composite renderer `sub_1004376D4`.

Rust animation assets therefore retain only concrete atlas regions. Geometry
and integer metrics are derived from that same captured region, rather than a
second type-neutral catalog lookup. A missing atlas or same-named CompoSprite
leaves the component null and emits no render command, preventing the wgpu
catalog from dynamically drawing an object the native component never bound.
The shipped TELEPOD regression loads all relevant sheets plus
`MENU_COMPOSPRITES.dat`, releases the composite set, and verifies that
`GENERAL_UI_BG` remains absent while ordinary `UI_BG_LEAF_1` atlas attachments
still draw.

The workspace passes all 558 tests (82 app/audio/wgpu, 31 assets, one core and
444 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. A fresh isolated-AppData 180-frame real-wgpu audit
preloads the shipped MENU group, drives the original LevelLoad IN -> LOAD ->
OUT state machine with a no-op destination, and asserts both the load callback
and transition-child disposal. It reports 19 optional data probes, zero
invoked fallbacks and zero remaining compatibility bindings. The final
readback SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
the PNG exists only to execute upload/render/readback and is not a visual
oracle.

Current SHA-256 values are
`0ab91e224a01569938bea7e84c4abe082c52e0b76492f1ba6327450eb09b90a2`
for `stella-app`,
`2c32619527cd6d1a35e4b0417ddcac76b1466bf4c3e45d3c034dcbcab104b8d0`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Stella main-loop cadence and absence of adaptive frame rate

The binary contains two CADisplayLink owners, so the presence of
`SceneGraphViewController.animationFrameInterval = 2` is not evidence for the
game cadence. That controller belongs to the embedded SceneGraph/Zappar path.
The Stella application enters through `AppController` instead. IDA and Hopper
both recover `sub_100401398`, the `framework::Configuration` constructor, as a
28-instruction leaf that writes integer 60 at configuration `+0x48`
(`framerate`). The game-specific configuration hook `sub_10002A278` adds the
two orientations and renderer, but does not change that field.

`-[AppController startUpdate]` at `0x100404CA8` loads the same `+0x48` value.
For positive values it performs signed integer `60 / framerate`, passes the
result to legacy `-[CADisplayLink setFrameInterval:]`, and installs `update` in
the default run-loop mode. The shipped value is therefore interval 1 on the
60 Hz iOS display path. The non-positive fallback is also interval 1. There is
no load/thermal/elapsed-time decision in this function, no setter on the
Stella configuration path, and no `preferredFramesPerSecond` or
`preferredFrameRateRange` selector in the binary.

`-[AppController update]` does measure the real monotonic elapsed time and
clamps it to `[0, 0.1]` before calling the engine. That protects simulation
after a stall; it does not adapt the display cadence. Consequently Stella
1.1.6 is fixed at a requested 60 FPS, has no enabled adaptive-frame-rate mode,
and has no native 120 FPS route. Rust already schedules its desktop display
callback at 16,666,667 ns and forwards the coalesced real delta with the same
100 ms clamp. The constant is now named `DISPLAY_LINK_STEP` and documents the
two recovered native functions; behavior is unchanged.

### DiscreteString skin binding at EntityTarget application time

The next LEAVES pass separates two native timeline types that the previous
resource snapshot treated alike. IDA and Hopper both recover
`sub_100412E10` as the `DiscreteSprite` JSON keyframe loader. It extracts the
keyframe string, invokes the resource callback stored in its first argument at
`0x100413024`, and passes the returned concrete `Sprite*` directly to
`TimelineDiscrete<Sprite*>::addKeyframe` at `0x100413030`. A null lookup is
therefore frozen into that particular direct keyframe at animation-load time.

`mockup::storeDiscreteKeyframes<TimelineDiscrete<std::string>>` at
`0x1004175E8` is intentionally different. It copies each JSON `value` into a
`std::string` keyframe and never calls the sprite resolver. The apply callback
`sub_100011BC4` later distinguishes the two state types. Its direct branch
for `DiscreteSprite` forwards the stored pointer to `sub_100469030`; its
string branch resolves the current AnimationSkins record, calls the live
resource provider, applies the skin attachment matrix and then stores the
resulting pointer (or null) in the same SpriteComponent member. The Lua
`setSkin` route at `sub_100013E18 -> sub_10000C100` only changes the selected
skin pointer; it does not force that sprite setter to run.

This is not an unused editor distinction. A complete audit of all 216 shipped
`.anim.json` files finds 6,239 sprite tracks, every one of them
`DiscreteString`; none is `DiscreteSprite`. LEAVES therefore must resolve its
16 default-skin attachments when EntityTarget applies `start`, `seek` or a
changed discrete state, not while `LEAVES.anim.json` is parsed.

Rust now records the JSON track kind explicitly. Only genuine
`DiscreteSprite` keyframes enter the load-time atlas snapshot.
`DiscreteString` application resolves the alias and selected skin against the
then-live ResourceManager, then retains the concrete atlas region, native
integer metrics and skin transform in the component's latched target. Draw and
world-bounds queries consume that retained binding; they do not re-query the
resource catalog or react immediately to a later `setSkin`. An applied null is
also represented separately from an artificial playback fixture that has not
passed through EntityTarget.

The state virtuals also pin the apply-mode boundary for both shipped discrete
types. For `TimelineDiscrete<std::string>`, `sub_100421E90`,
`sub_100421F1C` and `sub_100421FB0` respectively force a sample, seek while
returning whether the key index changed, and advance while returning that same
predicate. The matching `TimelineDiscrete<int>` members at `0x100423448`,
`0x1004234A4` and `0x100423518` have identical index-change behavior. Hopper
independently recovers the same state writes and boolean comparison. In
`sub_10041E41C`, mode 3 invokes the property's ApplyHandler only when virtual
offset `+0x58` returns true, while other modes invoke it unconditionally.

This gating still belongs to the last attached state for the usage. An
unchanged newer action therefore claims sprite or zOrder and blocks an older
action whose discrete key happens to change in the same update; the older
state must not fall through and run the setter. Rust now carries explicit
per-usage claim sentinels for both discrete properties. Focused two-control
regressions pin this otherwise subtle ownership rule for `DiscreteString` and
`DiscreteInt`.

The missing-skin branch is another pointer lifetime boundary. IDA recovers
`sub_100013E18` as a scene-map lookup followed by `sub_10000C100`. The latter
stores the matching skin record at AnimationSkins `+0x58`; when the name is
missing it emits `AnimationSkins -- Missing skin: %s` and explicitly writes
zero to that current-skin pointer. Hopper independently shows the same write
to member `0xb`. Neither path reapplies an EntityTarget, so already-bound
SpriteComponents remain unchanged. On the next forced apply,
`sub_100011BC4 -> sub_100016490 -> sub_10000C2F0` first tries the current skin
and, when that pointer is null, falls through to the default-skin pointer at
AnimationSkins `+0x50`.

Rust `setSkin` now removes the selected-skin entry when a loaded scene receives
an unknown name instead of retaining its previous selection. A two-skin
end-to-end regression proves all four observable stages: default binding,
valid selection without immediate rebinding, valid binding after seek,
missing selection retaining that concrete pointer until seek, and default
fallback on the following forced apply.

The new shipped LEAVES lifecycle regression loads the animation before its
sheet and verifies that the component remains null, loads the MENU sheet and
forces a seek to bind all 16 leaves, releases the sheet and verifies that the
already-bound components still draw, then forces another seek and verifies
that the live failed lookups replace those pointers with null. This covers
both sides of the resource lifetime boundary without using a screenshot as an
oracle.

The workspace passes all 562 tests (82 app/audio/wgpu, 31 assets, one core and
448 script/physics). Formatting, strict all-target/all-feature Clippy and the
release build are clean. A fresh isolated-AppData real-wgpu run drives the
original LevelLoadTransition IN -> no-op destination load -> OUT sequence,
asserts the destination callback, INGAME group request and final transition
child removal, and reports 19 optional data probes, zero invoked fallbacks and
zero remaining compatibility bindings. Its final readback SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
the PNG exists only to execute upload/render/readback.

Current SHA-256 values are
`8c724de1f9d7b743d9ee586b007ed656ae4dc4b151af54286ff49ac3db551c50`
for `stella-app`,
`7df218932e00a19dc5745e08a1ed57c283f9c37ac754f6b2992df5af8333cb76`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Loaded animation scenes have no implicit current action

The follow-up `getActions` audit first rules out an apparent ordering gap.
IDA `sub_10000F840` and Hopper both show the wrapper locating the first
`game::Animation` component and copying its action-pointer vector to Lua
indices 1..N. The JSON loader `sub_100414BD4` obtains the `actions` object as a
`std::_Rb_tree`, walks it with `std::_Rb_tree_increment`, and calls
`sub_10040FD78` in that order to construct the action vector. Consequently the
native list is lexicographically ordered, not source-JSON insertion ordered;
Rust's existing `BTreeMap` iteration is the exact representation and was
retained.

The same audit did expose a separate lifetime error after loading. The shared
bundle/AppData scene builder `sub_100010340` creates the scene entity,
AnimationSystemComponent, AnimationSkins and SpriteComponentCustom objects,
then deserializes the hierarchy. It never selects `idle`, selects the first
action, starts a control, or inserts a current-control record in the wrapper
map at `AnimationWrapper+0x30`. The latter record appears only through the
start path. `isPlaying` (`sub_10001384C`), `seek` (`sub_10001396C`), pause,
resume and `setSpeed` (`sub_100013D08`) all begin by looking up that map and
become no-ops when no current control exists.

The previous rehost instead installed a stopped synthetic `idle` or first
action immediately after load. Although its sprite pointers were null, entity
queries sampled that action's time-zero translation/scale/rotation, and a
pre-start seek or speed call mutated the invented control. The runtime now
represents a loaded scene with an empty control vector, no current action and
no detached control. Its hierarchy remains queryable at identity/base
transforms, every constructed SpriteComponent reports a null sprite pointer,
draw emits no animation sprites, and the first `start` alone creates and
applies the native control.

The expanded lifecycle regression proves the complete boundary through the
Lua ABI: before start the known joint remains at `(0,0)`, a known slot returns
the six-value transform shape with `hasSprite=false`, `isPlaying=false`, and
setSpeed/seek/pause/resume leave that state unchanged; after start the same
joint receives its authored `(5,7)` target and the slot binds its sprite. All
562 workspace tests pass, including construction/update/draw of every shipped
level. Formatting, strict all-target/all-feature Clippy and the release build
are clean. A shipped-LEAVES headless probe additionally verifies the native
alphabetical action order and the pre-start no-op boundary with zero invoked
fallbacks and zero compatibility bindings. The 180-frame real-wgpu IN -> LOAD
-> OUT audit also completes with zero invoked fallbacks and zero compatibility
bindings; its final readback remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`
and is used only as upload/render/readback evidence.

Current SHA-256 values are
`474377a302828bc996b2674ba1ecaf004256ad817cdc309a18f77712e4503ee2`
for `stella-app`,
`144c392d33c78695c52d43d4151e4a2612ca71df48af3a0bec8f629e46320bad`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### LEAVES startup cadence and float32 Lua UI arithmetic

The remaining startup-LEAVES timing difference came from the script VM rather
than the animation asset or wgpu geometry. Both shipped chunks begin with the
Lua 5.1 header bytes `1B 4C 75 61 51 00 01 04 04 04 04 00`: the fourth size
field is `sizeof(lua_Number) == 4`, with the following integral flag clear.
Purple therefore rounds every ordinary Lua arithmetic result to float32. The
transcoded mlua VM intentionally uses double numbers, so a value that stays
inside Lua can otherwise cross a frame boundary differently even though every
native API narrows its arguments correctly.

Unluac recovery of `scripts_common/ui_components/Frame.lua` shows that
`Frame:update` subtracts `gamelua.g_realDt or scaledDelta` directly from each
`delayedCall.timeLeft`, invokes the callback when the result is `<= 0`, then
removes the same live-list index. `Frame:doDelayed` stores the authored delay
without any other conversion. With Purple's float32 subtraction, the startup
sequence's first `0.8`-second delay retains `0x33D80000`
(`1.0058284e-7`) after 48 display ticks and fires on tick 49. Double
subtraction fired it on tick 48. The callback-created `0.5` and `0.3` delays
then fire on absolute ticks 79 and 97, respectively.

The same chunk-ABI boundary is observable in the surrounding startup effect.
`TweenSubsystem.lua` advances `timer = timer + rawDelta`; after 60 float32
ticks its one-second black-cover Tween retains `0x3F7FFFFB` and is not marked
done until tick 61. `tweenEaseCubicInOut` also rounds each divide, multiply and
add in the recovered source order. Finally, `game_init.lua` accumulates the
three LEAVES scales with `delta * 0.4`, `delta * 0.2`, and `delta * 0.1` after
updating Frame delays.

`game_lua/ui_float_precision.rs` now owns these UI-number boundaries instead
of enlarging the startup orchestrator. It preconditions the shipped Frame and
Tween updates so their existing add/subtract produces the exact float32
stored result, implements the recovered cubic-in/out operation order in f32,
and corrects only the uniquely identified three-scale startup LEAVES frame
after its shipped update. The original Lua callbacks, list mutation order,
draw order, delays, speeds and assets remain unchanged.

Focused Lua-ABI regression coverage pins the 49/79/97 callback ticks, the
48-tick residual, the 60/61-tick Tween completion boundary and cubic-easing
bits. A Rust operation-order regression pins all three accumulated scales.
The complete workspace passes all 564 tests (82 app/audio/wgpu, 31 assets, one
core and 450 script/physics); formatting, strict all-target/all-feature
Clippy, and the release build are clean. A fresh isolated-AppData 600-frame
headless run and two real-wgpu release runs complete with zero invoked
fallbacks and zero remaining compatibility bindings. The motion run asserts
that the special LEAVES scale frame received its native-number adapter; the
480-frame run asserts final startup-transition completion and disposal. Their
readback SHA-256 values are
`5b4e577a342f7e1510aab0c759626c3b131af01a08d07a0dd428c9281c538295`
and
`3c73dd420212bb12ef741571452065963e4b2e3a4402eb498794db368fb53678`;
the captures are execution smoke tests rather than visual oracles.

Current SHA-256 values are
`004704ca571a700ed4a189dd8a8cf4ac63f9cafcbc051e651917278e7862af2d`
for `stella-app`,
`236eb64dfe0856f3abe1f0e2e91d9bc99db913781b3a6460a1daddfbbf1da51b`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Complete shipped tween suite at Purple's float32 boundary

The startup LEAVES correction exposed a wider instance of the same VM-number
boundary. Unluac recovery of `scripts_common/tween.lua` contains ten unique
global curves: linear, cubic in/out/in-out, quadratic in/out, bounce-out and
sine in/out/in-out. The cubic definitions are repeated later in the chunk but
the second definitions have identical arithmetic. Every intermediate divide,
multiply, add and subtract is stored as a four-byte `lua_Number`; narrowing
only the arguments and final return is not equivalent. Bounce additionally
depends on the exact float32 constants and comparisons at `0.36363637`,
`0.72727275` and `0.90909094`.

The sine curves have a second native boundary that cannot be inferred from a
screenshot. IDA decompiles the Lua math wrappers at `0x100517D38` and
`0x100518348` as float argument extraction followed by the double-precision
`cos` and `sin` imports and a float result push. Hopper independently exposes
the exact ARM64 sequence in both wrappers: `fcvt d0, s0`, call the double libm
stub, then `fcvt s0, d0`. Rust therefore performs each angle operation in f32,
converts that angle to f64 for `sin`/`cos`, and narrows the result back to f32
before the remaining Lua arithmetic. Using `sinf`/`cosf` directly would model
a different native call path.

`game_lua/ui_float_precision.rs` now installs all ten recovered functions into
the shared game environment before `createStartUpAssets`. Each curve follows
the source statement order without reassociation. The native adapters also
retain Lua 5.1 arithmetic coercion for numeric strings; this matters because a
plain typed Rust callback would otherwise be stricter than the shipped Lua
functions. The existing Frame-delay, Tween-timer and three LEAVES-scale
corrections remain in the same focused module, so the startup orchestrator is
not enlarged.

Unit regressions pin the exact result bits for all non-bounce curves and one
sample in each of bounce's four branches. The booted-Lua integration regression
calls every published global, verifies numeric-string coercion, verifies that
`gamelua.tween*` and bare-global lookups resolve to the same functions, and
retains the 49/79/97 LEAVES delay plus 60/61 Tween-completion assertions.

The complete workspace passes all 565 tests (82 app/audio/wgpu, 31 assets, one
core and 451 script/physics); formatting, strict all-target Clippy and the
release build are clean. Independent empty-AppData release runs complete 600
headless frames and the real-wgpu LEAVES motion/final paths with 73 optional
data probes, zero invoked fallbacks and zero remaining compatibility bindings.
The motion capture remains byte-identical at SHA-256
`5b4e577a342f7e1510aab0c759626c3b131af01a08d07a0dd428c9281c538295`;
the independently booted final capture is
`11c67582f89c70a35dd500a28d14490a049c704392997b2746cffcaec677ed82`.
These captures execute upload, animation, rendering and readback; the reverse
engineered arithmetic and regressions, rather than screenshot appearance, are
the behavioral oracle.

Current SHA-256 values are
`ee630548bd2a971f7795ccf3b55e6ddcf8f016fbf382e180da826b7b799818cd`
for `stella-app`,
`4402a28335fa239d6625e987b9e797325a2d6d5a3bbf906a9da0ece34d59a172`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Luca tutorial support removal wakes the sleeping cage

The Chapter 01 level 23 failure was not authored into the level or Luca's
ability. The shipped level places `BLOCK_LIGHT_1X8_1_8` and
`BLOCK_LIGHT_1X8_1_7` below the horizontal cage member, and the recovered
`BlockComponents/LucaAbility.lua` intentionally handles pass-through glass by
putting each hit object in `deadBlocks` and setting `strength = 0`. That branch
does not apply an impulse. `removeBlocks` later calls `removeObject`, so the
upper cage depends on the native contact-destruction wake transition rather
than an ability-specific force.

The earlier contact audit stopped one call too shallow. ContactManager destroy
at `sub_10086B9B8` does not write either body's awake flag itself, but IDA and
Hopper both show its virtual listener call at
`0x10086B9EC..0x10086B9F8`, before it unlinks either contact edge. The installed
Purple EndContact listener is `sub_1000653AC`. Its instructions at
`0x1000653DC..0x100065410` set the awake bit and clear the float sleep timer on
both bodies before calling `exitTriggerCollision` or `exitCollision`. Omitting
that listener side effect left the surviving cage asleep after its two support
bodies disappeared, producing the visible floating structure.

The contact invalidation paths now wake every endpoint of each touching
contact before erasing the cached contact. This applies equally to DestroyBody
and DestroyFixture, matching their shared ContactManager listener route. The
ordinary callback order, contact-list-head order, sensor cleanup, Lua object
lifetime and destruction timing are unchanged. The corrected fixture-resize
regression also reflects that an old touching fixture wakes through EndContact
even though `native_resizeRadius` contains no direct wake call.

A focused Chapter01_L23-shaped regression uses both shipped support names and
the upper cage-member name. It establishes two contacts, puts the cage island
to sleep, removes both supports through `removeObject`, checks the immediate
awake/sleep-time state and verifies downward movement in the next fixed
1/30-second step without applying an impulse. The complete workspace passes
all 566 tests (82 app/audio/wgpu, 31 assets, one core and 452 script/physics);
formatting, strict all-target/all-feature Clippy and the release build are
clean. Fresh isolated-AppData 600-frame headless and real-wgpu runs report 73
optional data probes, zero invoked fallbacks and zero remaining compatibility
bindings. The wgpu readback SHA-256 is
`67c16e99a44987c2eb124dfc7fb2b0cda4a36e8b2bd12ea94813c312ebec4185` and is
used only as execution evidence, not as a visual oracle.

Current SHA-256 values are
`7b36cb68301b633bd216a7f167b59b8c8d84b7f7896497a81a9b687ffd9ffb55`
for `stella-app`,
`f12a824c00c60c14d2640849369091aa8b289fbdad46ab42998518a9abc9a535`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### ThemeParticleSystem per-layer spawners and draw interleave

The ordinary `Particles` vector is not the complete theme-particle path.
Purple's `ThemeParticleSystem` constructor at `sub_100096A48` derives from the
same base but adds a `std::map<int, Ptr<Spawner>>` at +0x88 and a
`std::map<int, vector<ParticleData>>` at +0xB8. IDA and Hopper agree that
`sub_100096B74` replaces a copied-query Spawner by source layer index,
`sub_1000973CC` routes each generated 0x68-byte record by
`themeLayerIndex`, and `sub_100096E4C` clears both derived maps after the base
particle member.

ThemeManager's setup walk `sub_100099168` constructs the query from a layer's
`particles` string and `spawnInterval`: X/Y/W/H/angle/amount are zero,
background mode is two, foreground mode is one, Z is `zDistance`, and
`themeLayerIndex` is the one-based source definition index at layer+0x88.
Spawn-expanded layer records deliberately reuse that index, so the final
Spawner replaces its predecessors in the map. A missing interval is -1.0,
which disables automatic emission while retaining the force-spawn query.

The per-layer update calls at `0x10009BA30` and `0x10009BA4C` precede the
layer's animation and motion update. `sub_100097090` subtracts delta from a
nonnegative Spawner timer, emits at most once when it reaches zero, then resets
the timer to the authored interval. Every particle uses `(1-z)` for gravity,
velocity displacement and the begin/end scale interpolation term; angular
velocity is not parallax-scaled. Finite records are erased before integration
when elapsed exceeds lifetime, and lifetime animation keeps the existing
one-based `ceil(progress * frameCount)` rule.

The draw member `sub_100096E9C` is called at `0x10009C17C` or
`0x10009C1A8` before the layer image. It selects only the current layer bucket,
adds the layer's world offset to each local particle coordinate, applies the
live GameLua top-left/world scale, and restores the renderer matrix after the
bucket. Rust now mirrors that exact interleave with separate background and
foreground systems rather than flattening these records into the ordinary
game-particle vector. The level loader's optional
`updateThemeParticlesNative` pre-roll advances only these systems; it does not
age theme animation, layer velocity, or ThemeSpriteData. The unrelated native
reset at `sub_1000984C0` is also no longer a compatibility no-op: it clears the
lazy camera-reference flag and the paired camera-effect floats while
preserving layers, sprites and particle maps.

The separate `ThemeSystem` constructor at `sub_10008AD30` publishes a Lua
object named `themeSystem` and registers `spawnBGLayerParticles` and
`spawnFGLayerParticles`. Their members at `sub_10009D008` and
`sub_10009D16C` read the final Lua argument as float32, truncate it with
`FCVTZS`, scan the expanded pass vector for the first matching `spawnerId` at
layer+0xF8, and use that record's one-based vector position to look up the
corresponding foreground or background Spawner. This two-stage mapping is
distinct from automatic updates, which pass the source definition index at
layer+0x88 directly. Rust now publishes both object members, retains the
previously omitted `spawnerId` field, and preserves the original first-match,
foreground/background separation and no-match return behavior.

The adjacent frame-chain audit recovered the previously disconnected producer
for ThemeManager's +0x54/+0x58 camera-effect pair. `sub_10004C524` starts or
stops the supported platform accelerometer, stores the effective active byte
at GameLua+0x2A0, and clears both filtered floats at +0x2A4/+0x2A8 on every
activation call. When active, `0x10005EC94..0x10005ECD8` widens each raw
float, multiplies by the exact double 0.2 constant, narrows it, then computes
`previous * 0.8 + sampleTerm` with float32 FMADD. Both ThemeManager passes
receive the same filtered pair. The host bridge now retains raw and filtered
sensor pairs, reproduces that reset and operation order, and naturally keeps
desktop/no-sensor execution at zero rather than bypassing the downstream
background transform.

The `setTheme` wrapper at `sub_10004D348` also revealed a state/application
split that the earlier host had flattened. It stores raw float32 `skyColor` at
GameLua+0x250..+0x258 and optional `groundColor` at +0x25C..+0x264; a missing
ground table explicitly zeroes its three fields. There are no 1.1.6 reads of
the ground triple. Sky color is not applied by selection: the background-only
branch at `0x10009BE6C..0x10009BE88` forwards it to `sub_100030C60` during the
later ThemeManager draw, where each channel uses `FMAX(0)`, integer truncation
and an explicit 255 ceiling. Rust now retains both native triples and changes
the renderer color at that same background-draw boundary, so foreground draws
and the interval between `setTheme` and `drawBackgroundNative` preserve the
previous framebuffer color.

One further lifecycle distinction comes from `native_refreshThemeSystem` at
`0x1000989D4..0x100098A0C`. `setTheme` parses and replaces the theme layer and
color state but does not construct either ThemeParticleSystem. Refresh first
clears the background and foreground systems, then invokes the setup walker
for query mode two and mode one respectively. Consequently, particles and
Spawner timers from the old theme remain observable between `setTheme` and the
next refresh; that refresh discards them and installs fresh Spawners for the
new expanded layers. Rust now performs construction only at this refresh
boundary instead of eagerly rebuilding during theme selection.

Focused regressions pin the first-frame immediate emission, one-burst
long-delta rule, `(1-z)` numeric fields, pre-roll isolation, expanded-index
replacement, explicit ThemeSystem routing, particle-before-layer wgpu command
order, accelerometer reset/filter handoff and the setTheme/refresh lifecycle.
Existing ordinary particle and theme tests remain unchanged. The delayed
sky/ground color contract has its own regression. The complete workspace
passes all 580 tests (82 app/audio/wgpu, 31 assets, one core and 466
script/physics);
formatting, strict all-target/all-feature Clippy and the release build are
clean. Fresh isolated-AppData 600-frame headless and real-wgpu runs report 73
optional data probes, zero invoked fallbacks and zero remaining compatibility
bindings. The wgpu readback SHA-256 is
`bd5b8848b77e54871fb3a8323633a8277dff4115ab9215595cc618f13585d4e7`
and is execution evidence rather than a screenshot oracle.

Current SHA-256 values are
`4057e7b077d6ea7b1d2169b64acdb709ba8b75ed80fb4036789ad7a236904786`
for `stella-app`,
`53de7aaedece93f35462b2076845dbda4eb2578b0f188dba48ded953a6a887dc`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### ParticleData retains its resolved sprite pointers

The earlier deferred-wgpu particle path still differed from Purple's native
resource ownership: it kept only the particle's sprite string and performed a
fresh active ResourceManager lookup on every draw. That made an existing
ordinary or theme particle disappear, switch to a same-named replacement, or
reappear when SPRT/COMP resources changed while the particle was alive.

IDA and Hopper independently show that `sub_10008E524` resolves the concrete
resource before the virtual add call. At `0x1000908F8..0x100090928` it invokes
the ResourceManager atlas lookup twice and stores the retained AtlasSprite at
ParticleData+0x20. Only when that result is null does
`0x100090930..0x100090948` call `sub_10045BF08` and store the CompoSprite at
+0x28. The update member `sub_100091834` repeats this lookup only after a
`lifeTime` animation selects a different sprite string: its normal branch is
`0x100091A10..0x100091A4C`, and the physics-disabled modes 3/4 branch duplicates
it at `0x100091BE4..0x100091C20`. No resource pointer is rebound on an ordinary
integration frame.

The two pointer fields also have an observable asymmetric overwrite rule. A
frame change always overwrites +0x20, but it overwrites +0x28 only if the new
atlas lookup is null. A composite-to-atlas transition therefore retains the
old lower-priority CompoSprite pointer. Both the ordinary draw member
`sub_100091D90` and ThemeParticleSystem draw member `sub_100096E9C` test +0x20
first and consult +0x28 only when it is null, so the new atlas remains the
visible resource while the old composite stays owned.

Rust ParticleData now carries separate frozen atlas and composite bindings.
All real construction routes bind at emission time: ordinary Lua particles,
theme automatic intervals, ThemeSystem force-spawn helpers, and level-load
theme-particle pre-roll. Both ordinary and theme `lifeTime` update paths rebind
only when their one-based frame changes. The draw members no longer access the
active resource catalog; they submit the retained binding and encode a null
pointer pair explicitly so a missing sprite cannot bind to a resource loaded
later. Atlas draw priority is preserved even when the retained composite slot
is still populated.

Focused regressions replace and release a same-named atlas after ordinary
particle creation, replace and release a `lifeTime` target atlas after its
frame transition, retain a composite across a later atlas frame while drawing
the atlas first, and repeat the release/shadow case through the real automatic
ThemeParticleSystem update/draw chain. The complete workspace passes all 583
tests (82 app/audio/wgpu, 31 assets, one core and 469 script/physics);
formatting, strict all-target/all-feature Clippy and the release build are
clean. Fresh isolated-AppData 600-frame headless and real-wgpu runs report 73
optional data probes, zero invoked fallbacks and zero remaining compatibility
bindings. The wgpu readback SHA-256 is
`6a5bbd8568fdac98483d921d7726ef408a6c080826326606653e4f2be70f90dd`
and is execution evidence rather than a screenshot oracle.

Current SHA-256 values are
`6fb194598726a386d65e90fa55fa9c1e1d6ffa04191e733064e29124c7f88150`
for `stella-app`,
`06750cdeb946e0f9685d8c8ee850dfe3e478ccb994335cc2e5c02e998d8a8872`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Particle draw-state ownership and native coordinate staging

The adjacent draw audit found that ordinary Particles and
ThemeParticleSystem deliberately have opposite renderer-state ownership.
IDA and Hopper show `sub_100091D90` constructing a default 0x9C-byte context
record and copying it into the live renderer at
`0x100091DE0..0x100091E54`, before it even checks whether the vector is empty.
Its common return path constructs another default record and copies that over
the live renderer at `0x100092024..0x10009208C`. It never saves the caller's
state. Consequently an entered ordinary foreground/background/menu/
notification particle pass resets translation, scale, angle, pivot, alpha and
clip state even when it contains no matching particles. A disabled in-game
pass is different because its GameLua wrapper never invokes this member.

ThemeParticleSystem draw `sub_100096E9C` instead copies the caller's current
0x9C bytes to its stack at `0x100096EE8..0x100096F04`, edits only the transform
fields needed by each particle, and invokes the renderer's restore-state
virtual at `0x100097054..0x100097064`. Its particles therefore inherit caller
alpha, clip and pivot while using their own translation, scale, rotation and
sprite dimensions; the complete caller state is live again after the layer
bucket returns. Rust previously left caller state live for ordinary particles
and built theme-particle commands from a fresh default, reversing both native
contracts.

The same procedures pin the coordinate operation order. Modes one/two store
`-topLeft / particleScale`, store `worldScale * particleScale`, and pass
`particlePosition / particleScale` to AtlasSprite/CompoSprite. Modes three/four
store `particleScale * Particles.scale` and pass position divided by that
product. Theme particles first FADD their local position and layer-world
offset, then FDIV by particle scale. The GL context later performs the separate
translation FADD and scale FMUL. Collapsing those operations to
`(position-topLeft)*worldScale` in host double precision is not bit-equivalent;
the focused ARM-float fixture differs by one complete output pixel.

Deferred particle commands now retain the native pre-scale position and live
context fields, and the shared wgpu transform boundary performs the same
float32 add/multiply sequence. Ordinary draw leaves the bridge at default;
theme draw snapshots the live state, overrides only its native transform
fields on each command and otherwise preserves it. Regressions cover the
empty-vector reset, disabled-pass non-entry, exact world/menu float boundary,
theme alpha/clip/pivot inheritance, post-bucket restoration and the existing
particle-before-layer ordering.

The complete workspace passes all 586 tests (82 app/audio/wgpu, 31 assets, one
core and 472 script/physics); formatting, strict all-target/all-feature Clippy
and the release build are clean. Fresh isolated-AppData 600-frame headless and
real-wgpu runs report 73 optional data probes, zero invoked fallbacks and zero
remaining compatibility bindings. The wgpu readback SHA-256 is
`dfe6723ce877f19c892e56a41f8e778014b4e6e85b7de57d3b5ee057654b5a41`
and is execution evidence rather than a screenshot oracle.

Current SHA-256 values are
`31168f6751c4478280bcf6d7c3caafb39168616e38234c2ca901079d632c55d3`
for `stella-app`,
`61f707ba8c09dba1e1c093803773a3f9c46c890538646cd1d878ae24e992d4ed`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Particle clear retains the base definition cache

The adjacent lifecycle audit found a refresh-time cache distinction that was
lost when Rust replaced each complete `NativeThemeParticles` value. IDA and
Hopper independently show the no-argument GameLua wrapper `sub_10004C630`
dispatching vtable slot `+0x60`. For base `Particles` that slot is
`sub_1000912D8`: it walks only the packed 0x68-byte vector at `+0x40..+0x48`,
destroys the two strings in every live ParticleData, and finally stores the
begin pointer into `+0x48`. It never erases the definition red-black tree
rooted at `+0x60`.

The derived slot `sub_100096E4C` first calls that base clear, then erases only
the Spawner tree at `+0x88` and the per-layer ParticleData tree at `+0xB8`.
The definition tree inherited from `Particles` therefore survives
`ThemeParticleSystem::clear`. The caller confirms the lifecycle order:
`native_refreshThemeSystem` clears the existing objects at ThemeManager
`+0xC0` and `+0xC8` at `0x1000989D4..0x1000989F0`, then invokes
`sub_100099168` for mode one and mode zero at
`0x1000989F4..0x100098A08`. Newly allocated managers begin with an empty
definition tree, but every later refresh keeps the definitions first resolved
by that same manager.

The ordinary tagged clear path was already behaviorally aligned. The wrapper
`sub_10004C640` maps `INGAME_BACKGROUND`, `INGAME_FOREGROUND` and `MENU` to
the global integers 2, 1 and 3, while `ALL` uses slot `+0x60`; unknown strings
only take the diagnostic path. Base member `sub_1000913BC` performs a stable
remove of every ParticleData whose mode at record `+0x60` matches the supplied
integer. Notification mode four consequently survives `MENU`, just as the
existing Rust regression pins.

Rust now clears both existing theme particle managers in native order, retains
their independent definition maps, and rebuilds the background and foreground
Spawner trees in place. The focused regression resolves a one-particle
`FIRST_PARTICLE` definition, changes the Lua table to three
`SECOND_PARTICLE` records, refreshes, and proves that the same native manager
still emits exactly one `FIRST_PARTICLE` from its first-use cache.

The complete workspace passes all 587 tests (82 app/audio/wgpu, 31 assets, one
core and 473 script/physics); formatting, strict all-target/all-feature Clippy
and the release build are clean. Fresh isolated-AppData 600-frame headless and
real-wgpu runs report 73 optional data probes, zero invoked fallbacks and zero
remaining compatibility bindings. The wgpu readback SHA-256 is
`52ca583acb554fac40dc6b6f24950318fefdf3073f5196fb62af9545eea8265d`
and is execution evidence rather than a screenshot oracle.

Current SHA-256 values are
`a2bd9629626e17ee2cecc69dcee8051a44649204f38763d270ecc0e54c6f417a`
for `stella-app`,
`d5e0ba1028e31d2527792994ef759a686fdaa40cfd0cb6feb562f2bcc03b8ad6`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Theme spawner comparisons and base-vector particle limits

The ThemeParticleSystem update audit recovered two further observable
differences. First, timed emission uses unordered-aware ARM conditions rather
than the host's ordinary paired range predicates. At
`0x100097168..0x100097170`, `sub_100097090` skips a Spawner only when
`interval < 0`. It then subtracts the delta from timer and skips emission only
when `timer > 0` at `0x100097174..0x100097184`. An unordered NaN comparison
takes neither branch, so a NaN interval enters the timed path, emits once, and
stores the NaN interval back into the timer. The same thing repeats on the next
update. Rust now spells those two predicates with `partial_cmp`, preserving
the native unordered case without changing finite interval behavior.

Second, Purple's particle limits intentionally do not accumulate the derived
theme buckets. The shared add member `sub_10008E524` reads the packed base
vector at `Particles+0x40` for the soft-limit calculation at
`0x1000904DC..0x10009056C` and again for the hard limit at
`0x100090570..0x1000905F8`. Only after it constructs and binds each
ParticleData does `0x100090958..0x100090968` call virtual slot `+0x40`.
ThemeParticleSystem overrides that slot with `sub_1000973CC`, which indexes
the tree at `+0xB8` by ParticleData's `themeLayerIndex` at `+0x58` and
appends there. Its inherited base vector therefore stays empty even while a
layer bucket grows into the hundreds or thousands.

Rust previously passed the target layer-vector length into the shared
soft/hard limit checks. Long-running background mist or leaf Spawners could
therefore halve later bursts after 60 records and stop adding records near
1,000, neither of which Purple does. The emitter now receives the native limit
source explicitly: ordinary particles use their base-vector length, while
both automatic and force-spawn ThemeParticleSystem paths use the still-empty
base count zero.

Focused regressions prove that a NaN interval emits on two consecutive updates
and that three 30-particle theme bursts produce 90 live records; the previous
derived-vector count produced only 75 by incorrectly halving the third burst.
The existing ordinary 60/1,000 limit regressions continue to pass.

The complete workspace passes all 589 tests (82 app/audio/wgpu, 31 assets, one
core and 475 script/physics); formatting, strict all-target/all-feature Clippy
and the release build are clean. Fresh isolated-AppData 600-frame headless and
real-wgpu runs report 73 optional data probes, zero invoked fallbacks and zero
remaining compatibility bindings. The wgpu readback SHA-256 is
`c549be015f434ba69aa744c5c90b39ef3630a972ae1003fd27685ac01b472f29`
and is execution evidence rather than a screenshot oracle.

Current SHA-256 values are
`ad99f7e8d6e9d18a8f5aa2d8543a2925ba3556772dc3072920a1e1070ef6a668`
for `stella-app`,
`800df16cc8054791948f1dc9383b2e1f07da316327b167690f1dd1aed9119f6d`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### `removeObject` keeps its native record live through EndContact

The gravity-field teardown crash on `BLOCK_JUNGLE_LOG_2_58` exposed the other
half of the synchronous DestroyBody lifetime. Rust already retained both Lua
`objects.world` records while dispatching EndContact, but it removed the
native object from `RenderBridge::scene` before dispatch. The shipped
`removeBlockEffect` path restores the leaving object's gravity scale from
`exitTriggerCollision`, so the strict native `setGravityScale` lookup then
reported `Missing object` even though Purple still has that object and its
body at this point.

IDA shows the exact enclosing order in `sub_100042260`: the call to object
teardown `sub_1000674FC` is at `0x100042E50`, while the
`RenderObjectData` red-black-tree erase does not occur until
`0x100042E54..0x100042E78`. Both IDA and Hopper show that teardown loading the
body from `RenderObjectData+0x88`, calling `b2World::DestroyBody`
(`sub_10086E02C`) at `0x100067524`, and clearing the body pointer only after
DestroyBody returns at `0x100067528`. Consequently the EndContact listener
reached by DestroyBody runs while both the lookup record and the body pointer
are live. The independent `setGravityScale` member `sub_10004F608` calls the
throwing object lookup at `0x10004F618`, then loads that same `+0x88` body
pointer and stores gravity scale at body `+0xB0`.

The callback-aware Rust removal path now expires attached contacts and
dispatches `exitTriggerCollision`/`exitCollision` before erasing the scene
object, broad-phase proxy, body-allocation slot and native joints. Lua record
removal remains after the callback as before. Missing names still throw from
`setGravityScale`; the fix restores the native lifetime instead of hiding bad
lookups.

One regression extends the ordinary EndContact ordering test with a strict
gravity-scale write to the body being removed. A second regression reproduces
the reported sensor stack and object name exactly, calls `setGravityScale`
from `exitTriggerCollision`, verifies the callback completes, then verifies
both Lua and native records are gone after `removeObject` returns.

The complete workspace passes all 590 tests (82 app/audio/wgpu, 31 assets, one
core and 476 script/physics); formatting, strict all-target/all-feature Clippy
and the release build are clean. Fresh isolated-AppData 600-frame headless and
real-wgpu runs report 73 optional data probes, zero invoked fallbacks and zero
remaining compatibility bindings. The wgpu readback SHA-256 is
`c475cfb405d1013e05893501ec2de4949da3a83df3d0e2a26994f819e387060f`
and is execution evidence rather than a screenshot oracle.

Current SHA-256 values are
`ca168cd632d1f4fa6e8275c16c28de41496f47ac157c4439f6a48ecc99f7ef27`
for `stella-app`,
`7ea1c57a7e72b0adede2506337c3a6c4d46451c4dcaa67158ea8b0ded0e341ed`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Stella timeout return to fixed-step flight

The Stella-only hitch after a long held ability is not in the authored
parkour tween. Unluac recovery of the shipped `StellaAbility.lua` shows that
the timeout/activation path selects `@Ability`, advances each jump with
`jumpTween.timer += jumpTween.speed * g_realDt`, and calls `setPosition` every
display update. A deterministic Chapter01 L02 probe confirms consecutive
positions during that action, including constant increments while the global
time multiplier is `0.15`. The `Ability` root therefore must not receive the
Poppy/Luca physics extrapolation.

The discontinuity starts after that tween leaves the ability state and the
bird returns to the exact authored `Stella_Flying` action. At this boundary,
position and direction again come from the last Box2D solution. IDA's
`sub_10005E898` shows the scaled frame delta entering the float32 accumulator
and Box2D stepping only at `0x3D088889` (1/30 second). Hopper independently
shows the same add/compare/repeated-subtract loop. The Flash draw member
`sub_10006794C` then reads RenderObjectData `+0xA4/+0xA8` directly for the
animation-root translation. On the fixed 60 Hz display link, the post-timeout
Stella root consequently exposes the 30 Hz body sample while the camera,
background and UI continue to update each display frame.

The existing visual-only fixed-step sampler now also selects the exact
`Stella_Flying` action. It evaluates `position + velocity * unsolvedTime` with
the recovered float32 fused operation order. BirdAnimation derives the
high-speed root angle from `atan2(velocityY, velocityX)`; the display sample
therefore also advances the velocity by world gravity times the body's
gravity scale and unsolved time before evaluating that same direction. At or
below the shipped speed-squared threshold of four it retains the Lua-authored
angle instead of predicting through the low-speed interpolation/collision
path.

Both corrections change only the submitted Flash root. The Box2D transform,
Lua `objects.world` coordinates, collision order, forces and ability timing
remain untouched. `Ability` is deliberately excluded because its original
real-time `setPosition` path is already continuous. Regressions cover the new
Stella position/direction sample, retain the Poppy/Luca samples, prove all
underlying scene positions remain unchanged, and separately prove that
`Ability` keeps its exact Lua-authored pose and angle.

The complete workspace passes all 592 tests (82 app/audio/wgpu, 31 assets,
one core and 478 script/physics); formatting, strict all-target/all-feature
Clippy and the release build are clean. A fresh deterministic Chapter01 L02
wgpu replay holds Stella's ability through its timeout and follows the return
to `Stella_Flying`: the submitted root position changes on every 60 Hz frame,
the high-speed direction sample advances between Box2D steps, and the shipped
low-speed angle branch remains untouched. The run reports zero invoked
fallbacks and zero remaining compatibility bindings. Its readback SHA-256 is
`9e86c0b84636740207afbd9955990b575503cdc7b8eaf42c64ae35c010a51eb0`;
this is execution evidence rather than a screenshot oracle.

Current SHA-256 values are
`df1dbd556acbbbd7d2922951ae010b790cf4a568bee14fddd8c5ea7ad7991822`
for `stella-app`,
`b57e68040102688bc51699c0ec0c2c5bc63849e496a3f86f228e9f1b20886994`
for `stella-headless`, and
`fec875edb8a6a8a5b10635f1ed35004476733ec2ca68b0f7854849d338632a4e`
for the unchanged `stella-tool`.

### Willow held-power fixed-step display sampling

The remaining held-power hitch belongs to Willow's exact authored
`@Willow_Spinning` action. Unluac recovery of the shipped
`WillowAbility.lua` shows that the held aim path passes `aimSlowmo` (`0.3`) to
`updateAbilityAimProgress`. It reconstructs real time as the scaled Lua delta
divided by the current multiplier, advances the spin angle on every display
update, and writes that angle back to the bird while its root translation
continues to come from the moving Box2D body. The release and cancel paths
restore the multiplier to one.

IDA's `sub_10005E898` and Hopper's independent assembly agree that the
multiplied float32 delta enters the same fixed `0x3D088889` (1/30 second)
Box2D accumulator used by the other bird powers. On a fixed 60 Hz display
link, `0.3` game time produces about nine body solutions per wall-clock second,
so Willow's rotation can advance every frame while the directly bound root
position repeats for roughly six or seven frames.

The visual-only fixed-step sampler now selects `Willow_Spinning` in addition
to the already recovered Poppy, Luca and Stella actions. It samples only the
root translation at the unsolved accumulator time. Willow's Lua-authored
angle, Box2D transform, Lua object position, collision ordering, forces and
ability timing remain untouched. A focused regression exercises the exact
held action and proves its submitted position advances without mutating the
scene object; a second regression proves an ordinary Willow action retains
both its solved physics pose and authored angle.

The complete workspace passes all 594 tests (82 app/audio/wgpu, 31 assets,
one core and 480 script/physics); formatting and strict
all-target/all-feature Clippy are clean.

### Held-power release boundary

The first exact-action sampler still covered one state that the original game
does not render as slow-motion physics. Recovered `PoppyAbility.lua` restores
the global delta-time multiplier to `1` as soon as the held power is released,
but deliberately retains `@Poppy_Power` during the short pre-attack phase. In
that phase, its per-frame update writes `poppyStartX/Y` back to the bird until
the drill starts. Selecting the display sample from the action name alone
therefore extrapolated the retained incoming velocity away from a root that Lua
had just pinned; each fixed-step accumulator wrap appeared as a small release
hitch.

IDA's `sub_10005E898` confirms that the float32 multiplier is the same native
state used to scale the fixed-step accumulator. The rehost now requires both
the exact recovered action and a multiplier below `1` before applying the
visual-only unsolved-time sample. This keeps the held Poppy, Luca, Willow and
post-timeout Stella paths continuous, then returns immediately to the exact
Lua/native solved pose when their scripts restore normal time. A release-state
regression leaves Poppy on `Poppy_Power` with retained velocity and proves that
the normal-time pinned position is submitted without extrapolation.

### Native two-slot physics-to-render interpolation

The three preceding fixed-step sampling sections record the interim diagnosis
and are superseded by a later complete recovery of `sub_10005E898`. The
original executable does not extrapolate selected bird actions from velocity.
It performs one global interpolation pass for every awake Box2D-backed
RenderObjectData, so ordinary sprites and Flash birds share exactly the same
display pose path.

IDA and Hopper independently expose the complete implementation at
`0x10005EFD8..0x10005F168`. GameLua `+0x23c` is a zero/one pose-slot selector;
each RenderObjectData begins with two 12-byte `(x, y, angle)` float32 slots at
`+0x00` and `+0x0c`. After a retained physics step the selector flips, the
awake body's `b2Transform::p` is copied into the selected slot, and its
rotation is normalized with `atan2f(q.s, q.c)`. The comparison against
`0x3D888889` skips snapshots only while more than two fixed steps remain on a
catch-up frame. Consequently the last two consecutive solved poses survive
even when one display update runs several `0x3D088889` Box2D steps.

After the fixed-step loop, Purple forms
`alpha = accumulator * f32::from_bits(0x41efffff)` and
`previousWeight = 1 - alpha`. Positions use the recovered float32 FMUL/FMADD
order. Rotation first moves the previous angle by plus or minus two-pi when
the slot delta crosses `f32::from_bits(0x40490fdb)`, then performs the same
blend across the shortest arc. The results are stored at RenderObjectData
`+0xa4/+0xa8/+0xac`; ordinary draw and Flash member `sub_10006794C` both read
these fields. This is interpolation between solved transforms with the
native one-step display latency, not prediction beyond the newest solution.

The explicit pose members confirm the other half of the contract.
`sub_10003FA60` writes a new position to the b2Body, the live render fields and
both interpolation slots; `sub_10003FB78` does the same for rotation. Thus
Lua-authored per-frame poses such as Stella's parkour tween, Dahlia's pinned
aim root and Poppy's release boundary cannot smear across stale physics
slots. The Rust rehost now implements these writes, the two pose buffers,
tail-step capture, float32 position/short-angle blend and shared ordinary/
Flash consumption. All action-name and velocity-extrapolation special cases
for Poppy, Luca, Willow, Stella and Dahlia have been removed. Gameplay body
state, Lua coordinates, collision order, forces and ability timing remain
authoritative and unchanged.

Regressions cover half-step position/angle blending, the two-pi shortest-arc
branch, explicit pose reset of both slots, action-independent Flash
submission, and a 0.1-second catch-up frame retaining its final two
consecutive solutions. The complete workspace passes all 597 tests (82
app/audio/wgpu, 31 assets, one core and 483 script/physics); formatting and
strict all-target/all-feature Clippy are clean. A fresh isolated-AppData
16,500-frame wgpu replay traverses L01, enters `Chapter01_L02`, holds the
Stella tutorial ability through its slow-motion section and returns the
multiplier to one. It completes without a Lua error, invoked compatibility
fallback or remaining compatibility binding. The readback SHA-256 is
`a00e6ee502840c80f103b58ac4236bca61af7387d507f3eb5786d156c2649753`;
this remains execution evidence rather than a screenshot oracle.

### Frame-head AudioOutput recovery and pre-zoom quit latch

Continuing the complete `sub_10005E898` audit past its physics section exposed
two earlier orchestration branches that were still narrower in the rehost.
IDA's `0x10005E8E4..0x10005EAB4` and Hopper's independent assembly first read
GameApp `+0x520`, test whether LuaResources owns an AudioOutput, then read the
actual output-running byte through `sub_1005796F8` (`AudioOutputImpl+0x158`).
When the application is audio-active and the output has stopped, Purple walks
`settings.root.audioEnabled` and calls `sub_10045D634` before publishing input
or invoking any Lua frame callback—but only when that nested value exists and
is exactly Boolean `true`.

This last condition deliberately differs from activation member
`sub_100029C24`. The activation member writes GameApp `+0x520` before its Lua
lookups and defaults a missing/wrong-typed setting to enabled; the per-frame
repair path does not. The Rust host now retains the active byte separately,
shares a three-state nested-setting reader, preserves activation's default,
and implements the stricter frame-head recovery. It reloads the retained
output state before starting so a replacement during the lookup cannot revive
a stale device. Audio input remains outside this recovery path, matching both
disassemblers.

Immediately afterward, `0x10005EAC8..0x10005EB14` applies Lua 5.1 truthiness
to `g_safeToQuit` and stores GameLua `+0x6ac`. Only then does
`0x10005EB18..0x10005EB58` compare the zoom slots and call `applyUserZoom`.
The prior Rust ordering latched the quit byte after that callback, allowing a
zoom handler to change native quit behavior one frame too early. The frame
facade now follows the recovered order. Focused regressions prove output
recovery for exact true, rejection of false/string/nil and inactive frames,
the distinct activation default, and a zoom callback whose Lua mutation is
not visible to the native quit latch until the following frame.

The complete workspace passes all 599 tests (82 app/audio/wgpu, 31 assets,
one core and 485 script/physics); formatting, strict all-target/all-feature
Clippy and the release build are clean. A fresh isolated-AppData 16,500-frame
wgpu route again completes L01, enters live `Chapter01_L02`, holds the tutorial
ability through slow motion and returns the multiplier to one. It reports no
Lua error, invoked compatibility fallback or remaining compatibility binding.
The readback SHA-256 is
`b8d780fa42694e051ad4271d7211b7c03d362dc67e971656259634a3b29fb3f6`;
the release application is
`d90435d1e3e629b41df8735fd879cb79cc52196f1c0e54f594aa7f85cafcd112`.

### Per-display-frame interpolated body export to Lua

The remaining slow-motion hitch was not another draw-only interpolation
case. The body bridge was publishing solved physics state to
`objects.world` at the wrong point in the frame. In Purple,
`sub_10005E898` calls `updatePhysics`, runs `b2World::Step`, clears forces,
invokes `removeBlocks` and consumes the collision-velocity replacement map
inside the repeated fixed-step loop at `0x10005EDB8..0x10005EFC8`. There is
no body-table setter in that loop. `clearLuaForceFunctions` and the complete
two-slot interpolation pass follow at `0x10005F064..0x10005F168`; only after
the second physics-lock read does the native code acquire `objects.world` at
`0x10005F278` and start the once-per-display-frame export.

The exported position is also specifically the interpolated RenderObjectData
pair at `+0xA4/+0xA8`, loaded at `0x10005F3AC` and `0x10005F3C8`. It is not
the newest Box2D transform. The full motion record is emitted only when the
body was awake on the preceding display pass or is awake now, and when its
inverse mass is positive or its type is kinematic
(`0x10005F384..0x10005F3A8`). `velocity` uses the exact float32
`y*y` followed by `FMADD x*x` and `FSQRT` sequence at
`0x10005F604..0x10005F610`. Component fields `xVel`, `yVel` and the
interpolated-angle write are conditional on the controllable byte `+0x140`
or record-velocity byte `+0x12D` at `0x10005F62C..0x10005F688`. Every body
then receives the normalized transform angle through `atan2f(q.s,q.c)` and
the inverse awake flag as `sleeping` at `0x10005F74C..0x10005F798`.

The former Rust loop wrote the newest Box2D `x/y` and velocity fields after
every catch-up step. With a slow-motion multiplier, a single display frame
can execute zero, one or multiple fixed steps; later `updatePhysics` calls in
the same frame therefore observed an intermediate pose, while rendering used
the separately interpolated pose. Bird ability scripts that read their own
Lua record could consequently advance the bird root in discontinuous chunks
even though the rest of the scene remained smooth.

The rehost now keeps collision-driven velocity replacement inside each fixed
step but moves body-to-Lua publication into a dedicated frame-export module
after interpolation. Lua `x/y`, out-of-bound reporting, float32 velocity
magnitude, conditional component fields, normalized angle, sleeping state and
the previous-awake predicate follow the recovered native pass. A focused
0.1-second regression executes two fixed steps, proves that both
`updatePhysics` calls still see the preceding display pose, and then proves
that the frame tail publishes the interpolated pose exactly. Older collision
and joint tests now distinguish authoritative Box2D state from the deliberately
one-step-latent Lua display state.

The complete workspace passes all 600 tests (82 app/audio/wgpu, 31 assets,
one core and 486 script/physics); formatting, strict all-target/all-feature
Clippy and the release build are clean. A fresh isolated-AppData 16,500-frame
wgpu route traverses L01, enters live `Chapter01_L02`, holds the Stella
tutorial ability through slow motion, restores the multiplier to one and
reports 94 optional data probes, zero invoked fallbacks and zero remaining
compatibility bindings. Its readback SHA-256 is
`ff4b11c05116681e7c4ecfa03023c03430b328a7143d861c543118516d0ccb53`;
the image is execution evidence and was not used as a visual oracle. The
final release application SHA-256 is
`969b49f1e8aab8482e6f03b922113540bca16ffd0675d1353034941b228ceabf`.

### Persistent out-of-boundary table and native setter order

The continuation of Purple's once-per-display-frame body export also fixes a
Lua table lifetime and ordering mismatch. IDA's decompilation of
`sub_10006FC0C`, checked independently against Hopper's complete procedure,
shows that the executable retrieves the existing global
`g_outOfBoundariesObjects` and requires it to be a table. It neither creates a
replacement nor clears old keys during this pass. The shipped `game_init.lua`
initializes that table once. GameLua `+0x308`, tested at `0x10005F260`, is the
ordered scene-map node count, so an empty scene skips both the world-table and
out-of-boundary-table lookups; a nonempty scene resolves `objects.world` first
at `0x10005F278` and the global table second at `0x10005F28C`.

For a dynamic body whose interpolated render position is outside the native
world limits, Purple writes the marker with `lua_settable`. The write therefore
honours `__newindex`, retains table identity, and occurs after the object's
interpolated `x/y` setters but before its velocity setter. The Rust frame export
now follows the same lifetime, scene-count gate, lookup order and setter
interleaving. It only marks records that have physics motion and positive
inverse mass, while leaving existing marker keys untouched exactly as the
native pass does.

Two regressions preserve this contract. One installs a metatable observer and
proves that `x/y` are already visible while velocity is still absent when the
marker is inserted; it also proves identity and key persistence after the body
returns in bounds. The other proves that an invalid global is ignored for an
empty scene but raises the native-style table error as soon as the scene owns
one object.

The complete workspace now passes all 602 tests (82 app/audio/wgpu, 31 assets,
one core and 488 script/physics); formatting, strict all-target/all-feature
Clippy and the release build are clean. The isolated-AppData 16,500-frame wgpu
route again traverses L01, enters live `Chapter01_L02`, exercises the tutorial
slow-motion hold, restores the multiplier to one and reports 94 optional data
probes, zero invoked fallbacks and zero remaining compatibility bindings. Its
readback SHA-256 is
`3645829cbc189f4c896795c471a024d11deb4b9610cf0c78548f7b955cb203fa`;
the release application SHA-256 is
`b64946c70762e83f7ec6fa81dff71394d5a48b3d46a2d29040a1eb295af97f49`.

### BirdAnimation rotation after native pose interpolation

The remaining held-ability hitch was isolated to the moving bird root: body
translation and every other scene object were already continuous. Completing
the tail of IDA's `sub_10005E898` explains that split. Purple finishes the
fixed-step loop and the two-slot RenderObjectData interpolation, exports the
interpolated body records to Lua, and only later reaches the main Lua
`update(float, float)` call at `0x10006058C..0x1000605A0`. BirdAnimation's
per-display-frame update therefore runs after the native angle blend.

The recovered shipped `BirdAnimation.lua` derives the flying root from the
current body velocity. Above speed two it calls `setRotation` with
`atan2(yVel, xVel)`. At or below speed two it calls the shipped `angleLerp`
with a start of zero, or pi for a horizontally flipped bird, and weight
`speed * 0.5`. Its state-updated hook then performs a second write for flipped
birds: it rebuilds the direction with `vec2FromAngle`/`atan2`, conditionally
subtracts pi for the middle half-plane, and calls `setAngle`.

IDA confirms that `setRotation` and `setAngle` are aliases of
`sub_10003FB78`. That member writes the Box2D angle, Lua angle,
RenderObjectData `+0xAC` and both native interpolation-angle slots. The late
BirdAnimation writer consequently replaces the already interpolated draw
angle with a value derived from the latest 30 Hz Box2D velocity. In ability
slow motion, velocity changes arrive even less often in wall-clock time, so
only the bird visibly repeats orientation while its interpolated translation,
camera, background and UI remain smooth.

The rehost now keeps a companion velocity pair beside the two recovered pose
slots and blends it with the identical selector, accumulator alpha and
float32 FMADD order. At Flash submission time, only the exact shipped flying
actions (`Stella_Flying`, `Poppy_Flying`, `Luca_Flying`, `Willow_Flying` and
`Dahlia_Flying`) replay BirdAnimation's high-speed, low-speed and flipped
second-writer formulas from that interpolated velocity. Explicit
`setVelocity` and impulse changes reset both visual samples, matching the
coherency rule already recovered for explicit pose changes. This is a
high-refresh rehost compatibility layer, not a claim that Purple stores extra
velocity slots: Box2D state, Lua `objects.world`, collisions, ability timing
and the native two-slot pose implementation remain unchanged.

Regressions cover the high-speed overwritten angle, the low-speed
`angleLerp` and flipped second-writer path, and explicit velocity-slot reset.
The complete workspace passes all 604 tests (82 app/audio/wgpu, 31 assets,
one core and 490 script/physics); formatting, strict all-target/all-feature
Clippy and the release build are clean. A 16,600-frame release-wgpu replay
traverses L01, enters live `Chapter01_L02`, holds Stella's ability through
timeout and follows its return to flight. Both the high-speed and low-speed
root angles advance on every display frame, the multiplier returns to one,
and the run completes with zero invoked fallbacks and zero remaining
compatibility bindings. Its readback SHA-256 is
`fd0371059050d13153b604cab126fbda9052f571dc5c10fea22c0fcb14660744`;
the image is execution evidence and was not used as a visual oracle. Current
release SHA-256 values are
`e8f582d5af2d5e4b5d07d7ca80b612e835c0bebad03ed57326d6d0599f338d46`
for `stella-app` and
`08e7bc93ae3f0af731b0d6a1df3f2765a6470b46891e6b12cab1553607a6def6`
for `stella-headless`.

### L44 pulley fixture scale source and persistent drift

The floating pulley report in `Chapter02_L44` exposed a source-table
distinction inside the already recovered circle branch of
`setPhysicsScale` (`sub_10004050C`). IDA shows that the branch reads the
object definition from GameLua `+0x458`, then indexes its `blocks` member and
the definition name before reading `scale`. GameLua's actual constructor
`sub_10002C274` publishes `blockTable` at `0x10002F5D0..0x10002F5E0`, then
publishes its `blocks` child at `0x10002F5E4..0x10002F5F4`; IDA and Hopper's
`Purple` procedure agree on that sequence. `sub_10001F98C` is instead the
DirtMechanics constructor: it resolves `blockTable` dynamically for its own
material snapshot and is not evidence for the GameLua member lifetime. The
circle path is therefore `blockTable.blocks`, not the unrelated global
`blocks` namespace that owns `BlockComponentManager`.

The former Rust lookup used global `blocks`. Every shipped definition missed,
so the circle branch silently used its native default definition scale of one.
For `PULLEY_7`, Lua has radius `0.0484416`, visual scale `0.0242208`, and the
`PULLEY` definition has scale `0.1`. The wrong lookup rebuilt the fixture with
scale `0.0243208`, producing radius `0.0011781` and mass `0.0000436`. The
native lookup uses scale `0.242308`, producing radius `0.0117378` and mass
`0.0043283`. The almost hundredfold mass error let the five-hertz pull joint
kick the wheel every fixed step while its weld joint repeatedly corrected the
pose, which appeared as continuous floating and also let the hanging rope
links travel far from their authored mechanism.

`circle_definition_scale` now follows the retained
`blockTable.blocks[definition]` path. A conflicting-table regression proves
that a similarly named entry in global `blocks` cannot affect fixture size.
A shipped-data L44 regression initializes the original event system, runs the
real `Pulley` and `Rope` components for 600 display frames, verifies the exact
native radii/masses for `PULLEY_7/PULLEY_8`, bounds their settled speed and
welded-pose drift, and confirms that the terminal rope links remain tethered
to both wheels.

The complete workspace passes all 605 tests (82 app/audio/wgpu, 31 assets,
one core and 491 script/physics); formatting, strict all-target/all-feature
Clippy and the release build are clean. A fresh isolated-AppData 2,201-frame
wgpu route enters the shipped file through its real Chapter02 pack index 46,
captures both pulley poses at frame 1,200 and asserts at the end that each
drifts less than `0.003` physics units and has speed below `0.01`. The route
reports zero invoked fallbacks and zero remaining compatibility bindings. Its
readback SHA-256 is
`40a1570c3330d743aa727338021f857be7bb63eb8efa47964238094ffd852e0a`;
the image is execution evidence rather than a visual oracle. Current release
SHA-256 values are
`1d00f4fb514376dd0d65e969c90aa7de5fab6e38aa94c29ecebdbce828fc83ad`
for `stella-app` and
`7c2d5df1ac268251880c6bb522fa3bcefd40749cd0828`
for `stella-headless`.

### BirdSimulation/AimStream settings are level-load snapshots

Continuing from the held-ability audit exposed one remaining lifetime mismatch
in the trajectory predictor. IDA's `loadLevelImpl` (`sub_100065D3C`) converts
`worldAttributes.simulationIterations` at `0x1000669B4`, retains
`simulationTimeStepMultiplier` at `0x100066A0C`, converts
`simulationStorePointsSampler` at `0x100066A68`, and stores the results at
GameLua `+0x4F8/+0x4FC/+0x500`. It then retains
`simulationAimSpawnTime/simulationAimSpeed` at `+0x504/+0x508`, copies those
last two values into AimStream `+0x40/+0x48`, calls reset
(`sub_1000086CC`) and deactivates the stream. Hopper independently shows the
same five reads and contiguous store order.

The predictor (`sub_100032970`) consumes the three retained GameLua fields and
uses `objects.currentTimeStep` as its only live Lua scalar. `getAimingTime`
(`sub_10004B8EC`) is a deliberate exception: IDA and Hopper both show it
resolving GameLua `+0x4D0/+0x4E8`, reading the current
`worldAttributes.simulationIterations`, applying `FCVTZS`, and multiplying it
by the live time step while excluding the time-step multiplier. The native
number paths use Lua 5.1 conversion, so numeric strings are accepted while
absent or nonnumeric values become zero.

The Rust bridge now mirrors those three native fields and snapshots all five
settings during `loadLevel`. Prediction retains only `objects.currentTimeStep`
as a live input, while aiming time also observes the live iteration field.
Regressions load numeric-string settings, mutate all five Lua fields afterward
and prove that trajectory sampling and AimStream population keep their loaded
values while `getAimingTime` changes immediately; a subsequent level load then
adopts the mutations for prediction too. AimStream reset still clears only
particles, control points and the active flag, preserving its native spawn
timer across the boundary. A wrapper-order regression also preserves
`sub_10004C4CC`: fewer than four control points skip both drawing and the
pending enabled-state write, whereas a valid path applies `setActive` even if
the sprite name is empty.

The complete workspace now passes 606 tests (82 app/audio/wgpu, 31 assets,
one core and 492 script/physics); formatting, strict
all-target/all-feature Clippy and the release build are clean. A fresh
isolated-AppData 1,200-frame release-wgpu route enters Chapter02 L44, creates
an isolated BirdSimulation body outside the authored world, mutates the three
predictor fields at frame 800, proves the sampled trajectory length is
unchanged, and simultaneously proves `getAimingTime` observes the new live
iteration count. The route completes with 79 optional data
probes, zero invoked fallbacks and zero remaining compatibility bindings. Its
readback SHA-256 is
`a94f2e9fbbfc8edbab882b43fef9ee1cb90e472c55859243e068a72dee324ee9`;
the image is execution evidence rather than a visual oracle. Current release
SHA-256 values are
`77dd66c9f842ee5efb8c1d2d848fbaafe33c39ed1690502025f48bb687711e18`
for `stella-app` and
`47c9fc7978747fc606070a9572a86b6efbba9867b80360364c6a70c190632110`
for `stella-headless`.

### Retained GameLua LuaObject identities

The pulley source-table correction exposed a broader identity rule that is
easy to lose in a Rust/Lua rehost. GameLua does not resolve all of its native
tables through their current script-visible names. IDA places the real
GameLua constructor at `sub_10002C274`: `0x10002F44C..0x10002F45C`
publishes the constructor's `objects` LuaObject, while
`0x10002F5D0..0x10002F5E0` publishes `blockTable` and
`0x10002F5E4..0x10002F5F4` installs its `blocks` child. Hopper's complete
`Purple` procedure independently shows the same strings, arguments and call
order. Later native users reach the retained members at GameLua `+0x408` and
`+0x458`; replacing `gamelua.objects` or `gamelua.blockTable` therefore does
not retarget them.

The constructor also creates and publishes its clipping result table before
any game scripts run. IDA shows `sub_10002C274` initializing the LuaObject at
GameLua `+0x430` (`0x10002C510`) and publishing it as `clippedText` at
`0x10002F438..0x10002F448`. Hopper shows the same member offset and store.
`clipText` (`sub_10004F630`) writes `widestLine` and `lines` through that
retained member at its tail, rather than resolving the current global. This
identity is consequently captured at constructor/bootstrap time; waiting for
the later game-logic load would incorrectly allow an intervening script
replacement to become the native result object.

Four earlier constructor members follow the same identity rule. The
constructor publishes `keyPressed`, `keyReleased`, `keyHold` and `cursor` from
GameLua `+0xF0/+0x118/+0x140/+0x168`. IDA's GameApp frame member
`sub_1000293C8` writes the five fixed platform keys directly through the first
three offsets at `0x1000295E8..0x100029644`, then clears the native press and
release bytes. Its wheel-frame tail writes `wheelTriggered` through `+0x168`.
The wheel callback `sub_100029FF8` likewise writes `wheel` and
`wheelTriggered` through the retained cursor at
`0x10002A1B4..0x10002A204`. Hopper independently shows the identical four
offsets and write order. The script-owned compact `g_*` event tables are a
separate dynamic representation and do not become constructor members.

The two adjacent gesture objects are constructor-owned too. IDA initializes
GameLua `+0x1A0/+0x1C8` at `0x10002C3A0..0x10002C3B4`, publishes them as
`multitouchSweep`/`multitouchZoom` at
`0x10002F3F4..0x10002F418`, and writes the latter's
`zoomCoolingTime = -1.0f` through the member. Those names have no other string
xrefs in either IDA or Hopper, but replacing the public names cannot release
the two LuaObjects while GameLua remains alive. `blockEditorTable` has a
separate rebind boundary: `sub_100044F90` resolves a new table into GameLua
`+0x480` before the module sequence, and `sub_100067A40` writes every named
module through that member. A definition script that shadows the public name
therefore cannot redirect the remaining loads; the next editor-load call
rebinds the member again.

`worldAttributes` has a different boundary. `loadLevelImpl`
(`sub_100065D3C`) resolves the current level table and replaces the LuaObject
at GameLua `+0x4D0/+0x4E8` on each successful level load. Collision force and
score code then reads that retained object; for example
`sub_100062520` loads `+0x4D0` at `0x100062558`, follows `+0x4E8`, and reads
`forceDamageMultiplier`. A same-name global replacement between level loads
must not affect those paths, but the next load must rebind them.

The same loader has a second retained level object. IDA and Hopper both show
`0x100065DEC..0x100065E14` resolving `deadBlocks` by name and assigning it to
GameLua `+0x4A8`. The delayed-destruction pass stores that member on the stack
at `0x10005F094` and passes it to the table setter at
`0x10005F1EC..0x10005F1F4`; it does not look up the current global again.
Collision damage shares the same native queue helper. Thus a same-name
replacement is ignored within the current level and adopted at the next
successful load, exactly like the retained pointer in Purple.

The rule is per native owner rather than per string. ThemeManager construction
(`sub_1000985DC -> sub_100072DC0`) still resolves `objects` dynamically by
name, while its theme definitions come from GameLua `+0x458`. DirtMechanics
constructor `sub_10001F98C` also resolves `blockTable` dynamically and then
snapshots its material fields. Those two routes deliberately remain dynamic
in the rehost.

The host now retains the twelve concrete `mlua::Table` identities in the Lua
registry. Bootstrap captures constructor-created `keyPressed`, `keyReleased`,
`keyHold`, `cursor`, both multitouch tables and `clippedText` immediately;
game-logic constructor completion captures `objects` and `blockTable`; every
successful level load replaces the retained `worldAttributes` and
`deadBlocks`; every editor-definition load replaces `blockEditorTable`.
Native input and cursor publication, scene/world access, definition merges,
fixture scaling, joint descriptors, level save, theme selection, collision
damage, scoring, delayed destruction, editor modules and clipped-text output
now consume the corresponding retained object. A separate bound marker
distinguishes an explicitly retained nil from a pre-boot object that has never
been captured. Pre-boot unit runtimes lazily capture the first concrete table
so their lifetime matches the first native use without inventing a separate
script-visible owner.

Regressions replace each same-name global after capture and prove that circle
fixture scale, definition-pack merges, trajectory time step, joint/world
access, collision multipliers, the native dead-block queue, `clipText`, native
key states and cursor/wheel publication continue to use the retained table.
The input regression verifies press, hold, pointer, wheel and frame-tail clear
behavior while all four same-name shadows remain untouched. The clipping
regression likewise verifies that the retained `lines` table is replaced per
call while its shadow remains untouched. Weak-reference coverage proves both
multitouch objects survive public-name replacement and two full collections.
The editor regression shadows `blockEditorTable` from its first available
module and proves the later `groups` module still enters the retained table.
A later successful level load is separately proved to rebind both level-owned
tables and snapshot the new BirdSimulation/AimStream settings. Dynamic
ThemeManager and DirtMechanics paths retain their own focused coverage.

The complete workspace now passes all 607 tests (82 app/audio/wgpu, 31
assets, one core and 493 script/physics); formatting, strict
all-target/all-feature Clippy and the release build are clean. A fresh
isolated-AppData 1,200-frame release-wgpu run loads the real Chapter02 L44
content, records both pulley poses after entry and asserts at the end that
each drifted less than `0.003` physics units. It reports 76 optional data
probes, zero invoked fallbacks and zero remaining compatibility bindings. The
readback SHA-256 is
`79c81aeb69323533f9e436fc726819f617eb3465ba8bd1d64e0ec5e74b4cd700`;
the image is execution evidence rather than a visual oracle. Current release
SHA-256 values are
`ffd56fa6b4d2016de110d399d4f54dc44432053af9a82ad93a77292eaf5c10bc`
for `stella-app` and
`1c7afc28f4e6fa5dd0852cf0b78c61c181e821a73611cd16d5f17c793813076f`
for `stella-headless`.

## BirdRun L09 wheel mounts and Chapter01 L61 gold submission

The map save's current event resolves to `BirdRun_L09`. Purple interprets its
stored variant seed `1725440811` numerically and selects group variants
`{1, 1, 5, 6, 8, 5, 8, 4}`. Group five, variant eight contains the four
reported circles `BLOCK_ROCK_ROUND_2X2_1_19` through `_22`. They use the
authored `BLOCK_ROCK_ROUND_2X2_1` radius `0.1`, density-backed dynamic bodies
and the ordinary stone sprite; the native circle constructor
`sub_100034FB0` in both IDA and Hopper copies that radius and chooses body type
two when density is nonzero. There is no wheel-specific anti-gravity or
floating branch.

The complete runtime joint table corrects the misleading result of looking
only at a partial level-object list. These four circles are intentionally
mounted by six physical type-two revolute joints. `_19` is joined to wood
block `_97`, `_21` to `_96`, `_20` to `_95` and the glass triangle, and `_22`
to `_98` and the same triangle. The wood joints for `_20` and `_22` are
breakable at authored force `2`; the remaining four mounts are not
breakable. Their visible suspension is therefore authored behavior, not the
Chapter02 L44 pulley fixture-scale defect. Forcing gravity-only motion or
deleting the mounts would diverge from Purple.

A deterministic ten-second native fixed-step replay now pins the exact eight
variant choices and all six mount descriptors. All four circles remain alive,
settle below `0.01` speed, enter Box2D sleep, stay within `0.05` physics units
of their loaded poses and leave score at zero. A separate 3,000-display-frame
run through the actual save/map entry also leaves score at zero and reports no
moving objects. Thus delayed collapse is not an unconditional idle defect.

A later fixed-step audit repeats the exact saved variant layout six times:
three independent runs idle for 60 simulated seconds and three for 120. The
first ten seconds are treated as the normal settling window. Every run retains
all 90 ordinary dynamic building blocks and all 33 level joints, removes or
adds no blocks, breaks no joints, leaves those buildings asleep and keeps score
at zero. The largest post-window position and angle changes are respectively
`0.004058409` physics units and `0.019947529` radians, identically reproduced
in all six runs and below the structural-drift audit threshold.

Full 60 Hz host update/draw comparison adds an important distinction. Depending
on the run's authored actor state, the lower chainsaw vehicle can remain posed
for a while and then move between the 10- and 120-second captures. A live
object audit identifies the complete moving set as
`BLOCK_JUNGLE_CHAINSAW_21/22`, its two round wheels, eight attached platform
blocks and `pig_medium_15/18`. No object is removed. The ordinary upper and
right-hand towers remain unchanged; the visible difference is coherent
movement/rotation of this mobile pig contraption, not an idle building collapse.
Screenshots are retained only as navigation evidence; this classification comes
from the queried object identities and poses.

The exact retail descriptors also establish that this motion is passive rather
than a hidden timed demolition. The vehicle's wood wheels
`BLOCK_WOOD_ROUND_4X4_1_10/11` are dynamic circle bodies attached to
`BLOCK_ROCK_1X10_1_27` by non-breakable type-three revolute joints. Both
descriptors explicitly contain `motor=false`, `motorSpeed=0` and `limit=false`;
their retained `backAndForth=true` metadata therefore has no enabled motor or
limit to drive. The two `BLOCK_JUNGLE_CHAINSAW_21/22` bodies are fixed to the
frame by non-breakable type-two weld joints, while `pig_medium_15/18` are
ordinary dynamic payloads and are not joint endpoints. IDA's `createJoint`
path at `sub_100037374` reads `motor`, `motorSpeed`, `limit` and
`backAndForth` independently before inserting the selected Box2D joint through
`sub_10086E470`; Hopper independently shows the same reads at
`0x100038A94`, `0x100038B54`, `0x100038E58` and `0x100039380`. Neither
disassembler exposes a timer or force application on the motor-disabled path.
Thus whole-cart settling, rolling or rotation under gravity/contact is authored
behavior. Unassisted removal of parts or collapse of one of the separate towers
is not: the repeated fixed-step audit above observes neither.

The supplied before/after captures already show scores `10` and `6190`, so
some collision/destruction path ran in that session; because this event uses
dynamic structures, autonomous actors and two deliberately breakable wheel
mounts, a later partial collapse after such interaction is consistent with
the authored level. The captures are navigation evidence, not a substitute
for the deterministic physics trace.

Chapter01 L61 exposed an independent native scene-submission omission. The
shipped `GaleSlice` raycast calls the gameplay gate `makeGolden(object)`, which
creates a live Lua shader named `2d-sprite-gold` with
`DIFFUSEC={1,0.6000000238,0}`, `LIGHTNESS=0.400000006` and `HIGHLIGHT=0.5`, then
changes material and scoring.
Both disassemblers show ordinary/composite scene draw member
`sub_10006D5B4` looking up the retained Lua object's `shader` field at
`0x10006D6D8..0x10006D744` after its pre-draw callback, resolving the cached
shader and passing the same pointer into the sprite submission. The Rust
ordinary-object path previously submitted `shader: None`, so gameplay state
became gold while wgpu could not display the coloration.

Scene submission now performs that live per-draw lookup and supplies the
recovered `SpriteShader` to the ordinary sprite command. One focused
regression proves removal of the Lua field removes the effect on the next
draw; a shipped-data regression boots the real Chapter01 L61, runs its actual
`makeGolden` gameplay gate on a wood building block and observes exactly one
gold shader reaching native scene submission with all three recovered
parameters.

The complete workspace now passes all 610 tests (82 app/audio/wgpu, 31
assets, one core and 496 script/physics); formatting, strict
all-target/all-feature Clippy, diff whitespace validation and the release
build are clean. A fresh release-wgpu map-to-event run reaches the exact
`BirdRun_L09` scene after 1,500 deterministic display frames with score zero;
its readback SHA-256 is
`f944378a2ff4ec92196ac42dff6b6ecacca5db9bcb236e03fcc7a5b50d414340`.
A second run binds `2d-sprite-gold` to a real mounted wood block and produces
a different deterministic readback from the otherwise identical baseline;
these images are execution evidence, not visual oracles. Both runs report
zero invoked fallbacks and zero remaining compatibility bindings. Current
release SHA-256 values are
`01d090d2dc886cc6cf1171350c2dcaeee34da69d6f606c0fff3ad6aa83c8fb32`
for `stella-app` and
`2f39428d1e24deeab785292b96ae40e006fb0e217acc7671021456becc330923`
for `stella-headless`.

## Authored gravity-sensor visuals

The `renderGravityVisualsNative` registration at `0x10002CDCC` points to the
hand-written GameLua member `sub_100032DC4`; Hopper independently resolves the
same constructor adapter and procedure. The member does not enumerate physics
sensors or draw their AABBs. It gates on the authored object table
(`sensorType`, `addVisualTimer`, `active`, `definition`), resolves
`blockTable.blocks[definition].type`, and supports only `circle` and `box`
definitions.

Circle definitions draw every authored `gravityVisuals` sprite eight times:
four quarter turns plus four diagonal quarter turns. Scale uses two float32
products followed by the binary64 `0.019` constant; the live GL translation,
scale, angle and sprite pivot are overwritten before every immediate draw.
Arguments two through four use the generated exact NUMBER-tag adapter, while
the type gates and authored fields retain their distinct Lua 5.1
`lua_isstring`/`lua_isnumber` and `lua_tostring`/`lua_tonumber` coercions.

Box definitions normalize by the **height** returned for
`THEME_1_GRAVITY_SLICE_BOX_FADED`, retain that sprite's pivot, rotate the
context by `angle + 3.1416f/2`, and place every authored slice along the
rotated height with the FMADD/FNMSUB sequence at `0x1000335A8`. Assembly at
`0x1000335B0..0x1000335C8` confirms the sine-derived value is ResourceManager
argument S0/X and the cosine-derived value is S1/Y.

The Rust/wgpu adapter now shares the ResourceManager immediate-sprite path,
including atlas/composite pointer capture and HPIVOT/VPIVOT anchoring. Sprite
bounds and pivots also use the concrete integer AtlasSprite/CompoSprite fields
queried by `sub_10045CD60`, `sub_10045CDAC` and `sub_10045CDF8`. The former
translucent AABB-cross approximation and its now-unused software-line shim
have been removed.

The complete workspace now passes all 614 tests (82 app/audio/wgpu, 31
assets, one core and 500 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. A copy of
the supplied AppData completed a 180-frame release-wgpu upload, render and
readback with zero invoked fallbacks and zero remaining compatibility
bindings. Its execution-evidence PNG SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
the image is not treated as a visual oracle. Current release SHA-256 values are
`139b3568714f686efc30174671fec10afe92b2c7dd86c15ba61071688bf88d85`
for `stella-app` and
`4ffc56627949a9099e9dceb48da6d98f544824d76a882abb94f030f64ce9f7df`
for `stella-headless`.

## Exact Lua tags for late particle, sensor and service adapters

The adjacent `setMenuParticlesScale` constructor publication at
`0x10002E004` selects generated adapter `sub_100088D24`. IDA and Hopper both
show its only argument passing through `sub_10052859C`, followed by a virtual
call through `GameLua+0xE8`. That object is the `Particles` instance built by
`sub_10008E160`; vtable slot `+0x10` resolves to the two-instruction
`sub_10008E51C`, which stores S0 directly at `Particles+0x38`. The Rust adapter
now requires the exact Lua NUMBER tag, ignores trailing slots and performs the
same float32 narrowing before the store.

`native_applySensorForces` is instead the hand-written GameLua member
`sub_10005B570`, published at `0x10002F330`. Its two names are both read by
`sub_1005285CC`, whose `sub_1005281F8(..., 4)` check requires the exact Lua
STRING tag before lookup and `sub_10005DE90` dispatch. The former Rust tuple
used mlua's ordinary number-to-string coercion. It now applies the two exact
string slots and preserves Purple's ignored trailing-stack behavior.

The same coercion audit reached two offline platform services. Generated
FusionGamerServices adapters `sub_1000CC2F4` and `sub_1000CC0C4` require,
respectively, `(STRING)` and `(STRING, NUMBER)` for `postAchievement` and
`postScore`. Assets adapter `sub_1000ACDE8 -> sub_1000ACE50` reads all three
`createSpriteSheet` arguments with `sub_1005285CC`. These bindings no longer
accept numeric values through mlua's convenient string conversion; their
offline/no-result behavior and successful sprite-sheet transaction remain
unchanged. Focused regressions cover missing, wrong-tag and trailing-slot
cases for every corrected boundary.

The complete workspace now passes all 616 tests (82 app/audio/wgpu, 31
assets, one core and 502 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. A copied
AppData 180-frame release-wgpu run again completes with zero invoked fallbacks
and zero remaining compatibility bindings. The unchanged deterministic
readback SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is execution evidence rather than a visual oracle. Current release SHA-256
values are
`2582905de2b03dd813eb17d470b21525d25e75212a7b8544ff558d249e8575ec`
for `stella-app` and
`8e5067cfbd0cdb5c7a8f66bbf4134a57e86c34d91bbd9edb96e03f4c1dc4e1e7`
for `stella-headless`.

## Exact Lua tags for direct GameLua sprite draws

The direct-render family at `0x10002E354..0x10002E40C` publishes four
separate generated adapters. IDA resolves `drawCompoSprite` to
`sub_100085BB4 -> sub_100085C1C`, `drawSpriteWithShader` to
`sub_1000845C0 -> sub_100084628`, and `drawSpriteWithoutShader` to
`sub_100084330 -> sub_100084398`. The final `isCompoSprite` binding reaches
`sub_100088F68 -> sub_100088FD0`; Hopper independently reports all four
registration targets and adapter procedures.

All three adapters read the sprite name with `sub_1005285CC`, whose
`sub_1005281F8(..., 4)` gate requires an exact Lua STRING. Composite draw then
reads slots two through five with exact NUMBER helper `sub_10052859C`. The
plain atlas draw reads slots two through six the same way. Shader draw first
converts slot two through `sub_1005286CC`, whose type-five gate requires an
exact TABLE, then reads slots three through seven with `sub_10052859C`.
Neither disassembler shows a stack-top equality check, so extra trailing Lua
values remain intentionally ignored. The adapters move the complete returned
floating registers into the member call without an additional conversion;
the already recovered member/render paths retain their own float32 boundaries.
`sub_100088FD0` likewise reads only exact STRING slot one and pushes exactly
one boolean result.

The former mlua `String`/`f64` tuples accepted convenient number-to-string and
numeric-string conversions that Purple rejects. The three Rust bindings now
decode `MultiValue` through the exact native STRING/TABLE/NUMBER helpers while
leaving lookup, shader-cache, composite fallback and affine behavior
unchanged. The former lookup shim also incorrectly scanned backward for the
last string argument; it now requires slot one and ignores the rest. A focused
regression rejects numeric sprite names, string-valued coordinates and a
non-table shader for all applicable functions, and proves that each generated
adapter still accepts a trailing ignored slot without changing lookup.

The complete workspace now passes all 617 tests (82 app/audio/wgpu, 31
assets, one core and 503 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`5e6d2c0e8ae2d6b867ccda0c375d09e05908d7111785d4a91ba213aae88ccc1e`
for `stella-app` and
`2a036cc4ae56c18fe1303cbe1b4229e31ebc9ac903aa3208ee9dfc3c4de44973`
for `stella-headless`.

## Exact Lua tags for textured, masked, selected and 3D-text draws

The adjacent GameLua registration spans expose four more generated adapters.
At `0x10002CF3C..0x10002CF94`, `renderMaskedImageNative` publishes
`sub_1000343CC` through `sub_100087F58 -> sub_100087FC0`, followed by
`drawString3D` publishing `sub_10003457C` through
`sub_100087B4C -> sub_100087BB4`. At `0x10002D9B4..0x10002DA3C`,
`drawSelectedTexturizedObject` uses `sub_1000855CC -> sub_100085634`, while
`drawTexturedRect` uses `sub_1000851C8 -> sub_100085230`. IDA and Hopper
independently resolve the same registration targets and conversion helpers.

`sub_100087FC0` requires exact STRING slot one and exact NUMBER slots two
through ten. It then narrows every number to float32, applies `FCVTZS` only to
the first eight coordinates, and leaves slot ten as the float32 UV factor.
`sub_100087BB4` requires exact STRING slots one/two and exact NUMBER slots
three through nine before the member installs its 3D projection.
`sub_100085634` similarly requires two exact strings and four exact numbers.
Finally, `sub_100085230` reads one exact string, four exact numbers and exact
BOOLEAN slot six through `sub_1005281BC`; that helper proves the boolean gate
is `sub_1005281F8(..., 1)`. None of the four adapters compares the Lua stack
top, so trailing values are deliberately ignored.

The Rust members already reproduced the recovered float32, integer-conversion,
state-reset, resource-pointer and projection behavior, but their mlua tuple
decoders still admitted numeric resource names and numeric strings. All four
bindings now decode `MultiValue` through the exact native helpers without
changing member-side math. A focused regression covers wrong tags in every
string/number/boolean position, ignored trailing slots, and successful
selected/masked/text submission after the stricter boundary.

The complete workspace now passes all 618 tests (82 app/audio/wgpu, 31
assets, one core and 504 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`c48aebc74fb24d3ef45c87a351f96ce348ce9b7fd71411a309f6b7f8556761ad`
for `stella-app` and
`8572dd3d6d565049f17971e884ca8933f5c29ad9a75d92e524f646e37557d083`
for `stella-headless`.

## Exact Lua tags for primitive, textured and rubber-band lines

The line registration family resolves to three generated adapter shapes.
`drawRubberband` is published at `0x10002C86C` through wrapper
`sub_10008973C`, whose generated body `sub_1000897A4` calls member
`sub_100030EB0`. The nearby GameLua registrations publish
`drawTexturedLine2D` at `0x10002E2D4` through
`sub_100084A34 -> sub_100084A9C` and member `sub_10004DB90`, then publish
`drawLine2D` and `drawRectLines` at `0x10002E304`/`0x10002E334` through the
shared `sub_1000848B0 -> sub_100084918` adapter and members
`sub_10004DC44`/`sub_10004DC8C`. Hopper independently identifies the same
wrappers, helpers and members; its instruction labels are four bytes after
the IDA string-reference labels where it names the following instruction.

`sub_100084918` reads exactly nine NUMBER slots with `sub_10052859C` for
both primitive line members. `sub_100084A9C` first reads an exact STRING with
`sub_1005285CC`, then nine exact NUMBER slots. Although the textured-line
member consumes only the first five numbers, the generated adapter still
validates the remaining four before dispatch. `sub_1000897A4` reads five
exact NUMBER slots followed by one exact STRING. None of these bodies compares
the current Lua stack top, so valid calls may carry ignored trailing values.
All numeric return registers cross the recovered float32 ABI before member
math; primitive positions and width then retain their separate `FCVTZS`
boundary.

The former mlua tuples reproduced arity but admitted Lua 5.1 convenience
coercions and rejected harmless extra arguments. All four Rust bindings now
use the exact native STRING/NUMBER slot readers while preserving the already
recovered signed-width, one-pixel cutoff, endpoint quantization, color packing,
UV order and mixed-precision rubber-band geometry. Focused regressions reject
numeric sprite names, numeric strings and a wrong type in an otherwise unused
textured-line slot, while proving that every adapter accepts an extra trailing
value.

The complete workspace now passes all 620 tests (82 app/audio/wgpu, 31
assets, one core and 506 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`a71589bdfc74cb48312e130ddd41cf94e1a6029665d98b7ed2721c0a50725482`
for `stella-app` and
`57a1d242e38adbcfd4420ad4761131d66eb4dc5d9fd7816d9c883d096452aa08`
for `stella-headless`.

## Exact rectangle adapter and direct polygon table traversal

The preceding primitive pair uses two different publication mechanisms.
IDA's `drawRect` string reference at `0x10002D9F4` and Hopper's following
instruction label at `0x10002D9F8` publish member `sub_100043C14` through
`sub_100085450 -> sub_1000854B8`. That generated helper reads exact NUMBER
slots one through eight with `sub_10052859C`, then exact BOOLEAN slot nine
with `sub_1005281BC`. It does not compare the Lua stack top, so trailing
values are ignored. The member's existing float32 color multiplication,
`FCVTZS` geometry, low-byte packing and optional complete state reset remain
unchanged; only the Lua boundary now rejects numeric strings and accepts
additional arguments exactly like Purple.

`drawPolygon` is instead published directly at the IDA/Hopper string-reference
pair `0x10002DA44`/`0x10002DA48` as LuaState member `sub_100043F28`, without a
generated adapter. Its first argument passes through `sub_100527F10`, whose
`sub_1005281F8(..., 5)` gate proves the exact TABLE requirement; slots two
through seven use the exact NUMBER helper. The member then calls
`sub_10052B324`, which uses `lua_next` to count every key/value entry rather
than applying the Lua length operator or `ipairs`. It fetches raw integer
indices `1..count` through `sub_100070444 -> sub_100528978`, and every fetched
value must itself be a table. A hash key therefore increases the required
contiguous integer range, while an array hole or non-table value raises an
error instead of ending or being skipped.

Point fields deliberately use a different coercion boundary. The member
performs ordinary `lua_gettable` access for `x` and `y`, then calls
`sub_10052A014`/`lua_tonumber`. Numeric strings convert; missing, boolean and
other nonnumeric fields become zero, and the result is retained as float32.
The former Rust `sequence_values::<Value>` implementation stopped at holes,
ignored non-table entries and made missing point coordinates an mlua error.
It now reproduces the complete count/raw-index/table-check/field-coercion
sequence. Focused regressions cover strict outer slots, ignored trailing
values, numeric-string and zero coordinate conversion, non-table entries and
the otherwise surprising extra-hash-key failure.

The complete workspace now passes all 621 tests (82 app/audio/wgpu, 31
assets, one core and 507 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`55f0e68bffcc20767a2a86be0c53626cb9bf4e83edb5bb77e307791679c4695f`
for `stella-app` and
`316b07fa0cdc579cedf8876199024efb98ff8e99ec0463329f501d0c35ea63b1`
for `stella-headless`.

## Exact notification adapter tags and ignored trailing values

The four adjacent notification registrations are visible at IDA string
references `0x10002CDFC`, `0x10002CE2C`, `0x10002CE5C` and `0x10002CE8C`;
Hopper labels the following instructions four bytes later. The enabled-state
member `sub_100034038` is published through `sub_10008962C`, which reads exact
BOOLEAN slot one with `sub_1005281BC`. Add member `sub_100034070` is published
through `sub_100088348 -> sub_1000883B0`; its helper reads exact STRING,
NUMBER and STRING slots with `sub_1005285CC`, `sub_10052859C` and
`sub_1005285CC`, narrows the delay through the float32 calling convention and
pushes the platform result as one boolean. Keyed removal member
`sub_100034398` uses shared `sub_100088F68 -> sub_100088FD0`, which requires
one exact STRING and returns one boolean. Cancel-all member `sub_1000343A0`
uses zero-argument/zero-result wrapper `sub_10008A07C`.

None of these generated wrappers compares the Lua stack top. The former Rust
typed tuples therefore had two observable mismatches: they accepted mlua's
number/string coercions and rejected harmless trailing values. The offline
notification map and enabled-state mirror are unchanged, including float32
delay storage, keyed result booleans, disabled addition and cancel-all. Their
Lua boundaries now use the exact native helpers. The notification regression
now rejects numeric identifiers/messages, numeric-string delays and numeric
booleans, while proving that add, keyed remove, enable and cancel-all all
accept extra trailing arguments.

The complete workspace still passes all 621 tests (82 app/audio/wgpu, 31
assets, one core and 507 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`e7457e712ed670a670663f14db2243e00ff33a66ab8ca3562b6c074e60579ad4`
for `stella-app` and
`fc988ff7f0d40e453a057fbad296d2b81b483b9c79e8e512afc2c920ae5ec576`
for `stella-headless`.

## Exact LuaResources locale and string-width dispatcher tags

The LuaResources constructor publishes `loadLocale`, `useLocale` and
`getStringWidth` at IDA string references `0x100446858`, `0x100446884` and
`0x100446B2C`; Hopper again labels the following instructions four bytes
later. These are reusable templated dispatchers rather than the GameLua
generated-adapter family. `loadLocale` uses dispatcher `0x10044BDC4` and
`ReturnValue<void>::callMethod<std::string,std::string>` at `0x10044BE2C`.
That body reads exact STRING slots one/two with `sub_1005285CC` before calling
`sub_1004482B8`/`sub_10045B7AC`. `useLocale` uses dispatcher `0x10044C4D8`
and callMethod body `0x10044C540`, which reads one exact STRING before member
`sub_1004482C0`/`sub_10045BBF8`. Neither void dispatcher inspects the stack
top, so both deliberately ignore extra values.

`getStringWidth` publishes thunk `sub_10044A998` through float/string
dispatcher `0x10044B5A0` and callMethod body `0x10044B608`. It likewise reads
exact STRING slot one with `sub_1005285CC`, ignores later slots, calls the
current IFont member and pushes the returned float32 through
`sub_1005287FC`. The Rust bitmap/system font paths already returned their
native f32 result widened to Lua's number representation. The three bindings
now use strict slot readers instead of mlua `String` tuples, eliminating
number-to-string coercion and exact-arity rejection without changing locale
lifecycle or font metric behavior. A focused regression covers wrong tags,
ignored trailing arguments and the unchanged shipped bitmap width.

The complete workspace now passes all 622 tests (82 app/audio/wgpu, 31
assets, one core and 508 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`7d4977b9bbc28bd1afe92b359a4568985abac457e43745cc15c489492a85b0a7`
for `stella-app` and
`ceea36818a120b89fbf28c5f8a587cc2a467c18c535b1131e1d99d8087e5d59c`
for `stella-headless`.

## Exact AnimationWrapper dispatcher tags across resources, playback and scene queries

The complete constructor span at `sub_10000EC80` exposes the remaining
generated AnimationWrapper adapter families. Resource loads at
`0x10000ED90`/`0x10000EDBC`, stop and skin dispatch through the shared
two-string wrapper `sub_10001D43C -> sub_10001D4A4`; close, pause, resume,
draw and both preload methods use one-string wrapper
`sub_10001D22C -> sub_10001D294`; close-all, stop-all and cache-clear use
zero-slot wrapper `sub_10001D1B4`. Playback start uses the three-string
wrapper `sub_10001CAF0 -> sub_10001CB58`, while is-playing uses the
one-string/boolean wrapper `sub_10001CF94 -> sub_10001CFFC`. Translation and
scale use `sub_10001C67C -> sub_10001C6E4` (STRING, NUMBER, NUMBER), and
rotation, speed and seek use `sub_10001C8C0 -> sub_10001C928` (STRING,
NUMBER). IDA and Hopper independently show `sub_1005285CC` for every string
slot and `sub_10052859C` for every number slot. None of these template bodies
checks the stack top, so later values are ignored; numeric arguments cross
the native float32 ABI before member logic.

The adjacent nontrivial entries follow the same tag contract. Contains-entity
uses boolean two-string adapter `sub_10001C014 -> sub_10001C07C`.
Set-playback-event uses `sub_10001C380 -> sub_10001C3E8`, whose first slot is
an exact string and whose second slot passes exact FUNCTION gate
`sub_100528760 -> sub_1005281F8(..., 6)`. Update wrapper `sub_10001C5F4`
reads one exact NUMBER, narrows it to float32 and ignores extras. Direct
members `sub_100015AB8`, `sub_100015C78`, `sub_100015E38`,
`sub_100015FF8`, `sub_10000F46C` and `sub_1000161B8` each read exact STRING
tag/entity pairs before producing their two-, five/six- or four-value query
results. Get-actions uses one-string/table adapter
`sub_10001BD5C -> sub_10001BDC4`. Finally, direct set-shader member
`sub_10000FC30` requires exact STRING slot one, then constructs a shader only
when the Lua stack top is exactly two and slot two is a table; every other
arity/type clears the scene shader.

The Rust registrations now read `MultiValue` with those exact slot helpers
instead of mlua typed strings/numbers. This removes number-to-string and
numeric-string coercion while retaining native ignored tails, float32
quantization, return counts and the set-shader exact-top exception. The
existing end-to-end AnimationWrapper lifecycle regression now exercises every
resource, playback, transform, draw, callback, entity-query, action-query and
shader tag boundary, including invalid values and harmless extra arguments.

The complete workspace still passes all 622 tests (82 app/audio/wgpu, 31
assets, one core and 508 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`5378ab5c00632ce6ab0d0d8e9c8f55a3296f1ebe47675f4c46cacf64e05354ea`
for `stella-app` and
`932898bd1333cf180cda933bda47d6b212596e73d595a5adf3c2d0dd8bc9b846`
for `stella-headless`.

## Exact resource clip, legacy sheet and composite-audio adapters

The LuaResources constructor publishes `setClipRect` at string reference
`0x100446968` through raw `ffff` dispatcher `0x10044BC28`. IDA and Hopper
independently show four exact NUMBER reads through `sub_10052859C`, float32
argument passage and no stack-top comparison. `getClipRect` is the adjacent
direct member `sub_1004489EC`; it reads no arguments and therefore also
ignores extras. The Rust setter now uses the same strict slots instead of
mlua numeric coercion while retaining its already recovered float32 additions
and independent `FCVTZS` edge conversions.

The older ResourceManager registrations at `0x1000939B8` and `0x1000939E4`
publish `native_createSpriteSheet`/`native_releaseSpriteSheet` through shared
wrapper `sub_100094A94 -> sub_100094AFC`. That helper reads one exact STRING
with `sub_1005285CC`, ignores every later value and returns zero results. Both
Rust calls now expose that exact adapter while preserving the common
LuaResources sheet pointer, legacy byte-accounting map and release-to-zero
node behavior.

`createCompositeAudio` is published at `0x10044673C` through dispatcher
`0x10044C124`, whose instantiated call method requires exact STRING and TABLE
slots and ignores extras. Its member `sub_100447CBC` has a separate Lua 5.1
element contract: each raw integer index is first tested by
`sub_10052811C`, which is `lua_isstring` rather than truthiness or an exact
string check. Strings and numbers therefore continue; the number is converted
by `sub_100529FB4 -> sub_100508E38`/`lua_tolstring`. Nil, boolean, table and
every other tag terminate the contiguous scan immediately. Resolved clip
pointers are retained and missing names are skipped. The Rust loop previously
stopped only at nil and silently skipped all other non-string values; it now
coerces numeric names and stops at the first non-string-convertible value in
the recovered order.

Focused regressions cover every clip rectangle NUMBER slot, ignored tails,
strict legacy sheet names, strict composite outer slots, numeric clip names
and boolean sequence termination. The complete workspace still passes all
622 tests (82 app/audio/wgpu, 31 assets, one core and 508 script/physics);
formatting, diff whitespace checks, strict all-target/all-feature Clippy and
the release build are clean. An isolated copy of the supplied AppData again
completed a 180-frame release-wgpu upload, render and readback with zero
invoked fallbacks and zero remaining compatibility bindings. Its deterministic
execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`4bb498ff34b1f79fea307e28262cdee014ff9eab759f57b305a11f1f16e0106f`
for `stella-app` and
`dd788b5b8fff1aa7ea13cc1482efd68131495ca6ce885ef587e0e203af28ce22`
for `stella-headless`.

## Exact audio stop/query selector order

`stopAudio` and `isAudioPlaying` are handwritten direct Lua members rather
than generated overloads. The constructor registers them at
`0x1004469F4 -> sub_100448C2C` and
`0x100446A40 -> sub_100448D68`. IDA and Hopper independently recover the
same order in both members: first test Purple's private INTEGER tag with
`sub_10052817C`, otherwise test Lua 5.1 string convertibility with
`sub_10052811C`, and only then enter the selected `LuaResources` member.
The string-convertible NUMBER case immediately passes through exact STRING
extractor `sub_1005285CC`, so a fractional number raises a type error instead
of being ignored. Boolean, table, nil and other tags select no member;
`stopAudio` returns silently and `isAudioPlaying` pushes false.

This selector order precedes AudioOutput lifetime handling. Handle member
`sub_10045C93C` and name member `sub_10045C714` throw when no output exists,
whereas query members `sub_10045CCEC`/`sub_10045CBBC` return false. Therefore
`stopAudio(false)` remains a no-op even before output construction, a valid
name or handle reports the missing output, and an invalid fractional number
reports its tag error first. The Rust implementation previously checked the
output before dispatch and silently ignored fractional numbers. It now uses
one shared selector parser before the member-specific lifetime branch. The
stock-Lua integral-number compatibility path remains restricted to finite,
exactly integral values returned for Purple handles.

Focused coverage now includes pre-output names, handles, fractional numbers,
booleans, ignored trailing values and post-start name/handle stopping. The
complete workspace still passes all 622 tests (82 app/audio/wgpu, 31 assets,
one core and 508 script/physics); formatting, diff whitespace checks, strict
all-target/all-feature Clippy and the release build are clean. An isolated
copy of the supplied AppData completed a 180-frame release-wgpu upload,
render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`43cc2c373667723e2eb14c38074de565f3a9dac1143300f155d8eed652c3ce00`
for `stella-app` and
`454131246f9e003648db5760265899954e51a98dbdec62ebaeb1573afc5298ba`
for `stella-headless`.

## AudioOutput-owned track, channel-limit and instance-volume state

The LuaResources tail registers `setTrackVolume` and `getTrackVolume` through
generated strict float dispatchers at `0x100446D40` and `0x100446D6C`.
Their members `sub_10044AAA0`/`sub_10044AAD0` both first call
`sub_10045D8D4`, which is a raw load of the AudioOutput pointer at
`LuaResources+0x38`. They then follow `AudioOutputImpl+0x18` to the embedded
AudioManager before `sub_100572BE0` clamps/writes a track or
`sub_100573030` reads it. IDA and Hopper independently expose this pointer
chain and no null branch. Track gains therefore do not exist independently
before an AudioOutput has been constructed. The Rust adapters previously
read and wrote their detached AudioRuntime defaults in that state; they now
validate all generated Lua NUMBER slots first, require the live output owner,
then perform native float32 `FCVTZS`, bounds and clamping behavior.

The same ownership applies to GameLua's adjacent native audio façade.
`setChannelCountLimit` reaches
`sub_100058FFC -> sub_10045D8D4 -> sub_1005796C8`; the latter dereferences
the output and enters the same embedded AudioManager before its track bounds
check. `setAudioClipVolume` direct adapter `sub_10005920C` first extracts an
exact INTEGER handle and NUMBER volume, then follows the output pointer for
both `sub_100579630` handle liveness and `sub_1005796D4` volume update.
Neither operation owns pre-output state. Their Rust installers now receive
the ResourceRuntime owner explicitly and preserve this adapter-then-owner-
then-member order. Startup tests construct the output before exercising the
callback, as Purple's shipped `createStartUpAssets` does; the later five
hard-coded Stella limits remain a direct AudioManager write rather than a
second Lua call.

Focused regressions cover all four pre-output operations, generated wrong-tag
ordering, ignored trailing values, post-construction track defaults/clamping,
channel limiting and live-handle volume updates. The complete workspace still
passes all 622 tests (82 app/audio/wgpu, 31 assets, one core and 508
script/physics); formatting, diff whitespace checks, strict
all-target/all-feature Clippy and the release build are clean. An isolated
copy of the supplied AppData completed a 180-frame release-wgpu upload,
render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`ca5db1a6699042371c8fd99f02f911652890bd430d019f2f37a873127f019fd7`
for `stella-app` and
`eff332d641ea2f0cb0bf04bef29665b5e54b59a8b658a4ef6c9f3d44d6248969`
for `stella-headless`.

## Startup audio callback and direct channel-limit ownership

IDA and Hopper independently show that GameLua's handwritten
`playAudioReturnUniqueHandle` adapter `sub_10005902C` first reads the current
stack top, requires an exact STRING in slot one, and supplies native defaults
only for absent or explicit-nil optional slots: volume `1.0`, looping false
and channel zero. Present values must respectively be exact NUMBER, BOOLEAN
and NUMBER tags; the channel NUMBER is narrowed to float32 before `FCVTZS`.
Extra arguments are ignored. The call then reaches `sub_10045C4BC`, which
requires the live AudioOutput before resource lookup and playback, and pushes
the returned integer handle. This complete branch order already matches the
Rust optional adapters, so no compatibility path was added.

The deeper startup member `sub_10005D44C` first invokes the Lua global
`createStartUpAssets`. After the callback returns, it reloads the
LuaResources AudioOutput pointer through `sub_10045D8D4` before each of five
direct `sub_1005796C8` AudioManager writes: `(1,4)`, `(2,6)`, `(3,3)`,
`(4,5)` and `(5,5)`. There is no independent startup-owned limit array and no
null-output branch. The Rust host previously wrote those limits to detached
runtime state even when a replacement callback returned without constructing
an output. It now requires the live output after the callback and before the
first hard-coded write. Invalid embedding order becomes a recoverable host
error instead of leaving pseudo-state; the shipped callback still follows the
recovered construction order.

Focused coverage verifies that a callback is observed before the native
limits, and that a callback which creates no output leaves every limit at its
uninitialized value. The complete workspace still passes all 622 tests (82
app/audio/wgpu, 31 assets, one core and 508 script/physics); formatting, diff
whitespace checks, strict all-target/all-feature Clippy and the release build
are clean. An isolated copy of the supplied AppData completed a 180-frame
release-wgpu upload, render and readback with zero invoked fallbacks and zero
remaining compatibility bindings. Its deterministic execution-evidence
SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`3c702ec89ccf69bf21c5846afdb3823427e66d6507d38106245d73cc6fd5cc73`
for `stella-app` and
`82334920fda603925d846997238364f25f7f116ac35f6da03465c94cbd6c9631`
for `stella-headless`.

## Audio device controls and active-channel range exceptions

The final LuaResources constructor block publishes `startAudioOutput` at
`0x100446C64` through the generated zero-argument Boolean dispatcher, followed
by three zero-result control dispatchers for `stopAudioOutput`,
`startAudioInput` and `stopAudioInput` at
`0x100446C90/0x100446CBC/0x100446CE8`. IDA and Hopper independently recover
the same wrappers `sub_10044AA78..sub_10044AA90`. Output start requires the
pointer and returns true even when its worker is already active; output stop
is a no-op without the pointer. Input start requires its pointer but the
underlying two-instruction `sub_10057AB98` merely returns one, which its void
Lua dispatcher discards; input stop reaches `nullsub_288`. The existing Rust
control ABI already has these exact return counts, repeat behavior and
lifetime branches.

Following output start into playback exposed a separate range discrepancy.
`sub_10045C4BC` first requires the AudioOutput and resolves the clip name. A
missing resource returns unsigned `0xffffffff`. Only a resolved clip reaches
`sub_100579600 -> sub_100572208`, which first checks the manager active byte
and returns the same sentinel while stopped. On an active manager it calls
`sub_1005724A8` before loading the channel-limit slot. Both disassemblers show
that helper comparing the signed channel as an unsigned value against eight;
negative values and values at least eight actively throw
`Track {0} out of bounds! Range [0-{1}]`. This is therefore a defined
exception boundary, not behavior inferred from the later indexed load.

The Rust playback bridge previously returned `-1` for every invalid channel.
It now preserves the recovered sequence: output owner, clip lookup, active
gate, then channel validation and instance allocation. Consequently an
invalid channel still returns `-1` for a missing clip or stopped output, but
throws for an existing clip on an active output. The error path neither
consumes the wrapping handle counter nor reaches the legacy ResourceManager's
post-play count increment. Focused Lua regressions cover negative/eight
channels through `res.playAudio`, `playAudioReturnUniqueHandle` and
`ResourceManager.native_playAudio`, plus the missing-resource and stopped-
output precedence cases.

The complete workspace still passes all 622 tests (82 app/audio/wgpu, 31
assets, one core and 508 script/physics); formatting, diff whitespace checks,
strict all-target/all-feature Clippy and the release build are clean. An
isolated copy of the supplied AppData completed a 180-frame release-wgpu
upload, render and readback with zero invoked fallbacks and zero remaining
compatibility bindings. Its deterministic execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`ccc746811bd9753eb592bcc73b89913548e33ab3c9959b8b61221ea061666928`
for `stella-app` and
`ca0e1b22aa50cbed480a0869fd0e8f971342ff2f162d2ef522c09ea4e6835b51`
for `stella-headless`.

## Strict object, calendar and SimpleRandom argument boundaries

The remaining typed-tuple audit found three places where mlua's convenient
conversions were broader than Purple's Lua ABI. `setSpriteRotation` and
`multiplyVelocity`, registered at `0x10002D484` and `0x10002D7C4`, both enter
the generated `sub_100086690 -> sub_1000866F8` adapter. IDA and Hopper show
slot one being read by `sub_1005285CC` as an exact STRING and slot two by
`sub_10052859C` as an exact NUMBER; trailing stack values are never examined.
The members still retain their distinct effects: `sub_10003FC88` narrows to
float32, normalizes the sprite angle with `fmodf` and writes the visual angle,
while `sub_100041A44` multiplies both float32 body-velocity components and
wakes only a non-static body with a non-zero result. The Rust adapters now
reject number-to-string and numeric-string coercions before touching either
native or Lua state while continuing to ignore extra arguments.

`addDurationToTime` is the hand-written `sub_100056D68`, published at
`0x10002ECB0`. It similarly requires an exact TABLE in slot one and exact
NUMBER in slot two before reading the six mandatory calendar fields. The
duration remains narrowed to float32 before the recovered `mktime` arithmetic;
only the argument boundary changed. A numeric string can therefore no longer
silently advance a date, and an extra third value remains harmless.

The complete `SimpleRandomNative` registration at `sub_100094D34` uses
individual hand-written members rather than one common generated signature.
`newSeedFromString` (`sub_100094EC8`) requires STRING and streams it into an
unsigned integer; failed parsing returns zero Lua results.
`newSeedFromNumber` (`sub_100095228`) requires NUMBER, narrows it to float32,
then executes `FCVTZU W1, S0`. `random` (`sub_100095260`) requires INTEGER,
NUMBER, NUMBER in that order, performs the recovered MSVC LCG correction, and
applies separate `FCVTZU` operations to both bounds before its unsigned
remainder. `seedToString` (`sub_10009530C`) requires INTEGER but explicitly
masks it to 32 bits before unsigned decimal formatting. No-argument seed
members and every hand-written member ignore trailing values.

Rust now has a shared AArch64 `FCVTZU W,S` helper: finite in-range values
truncate, negative/NaN inputs produce zero and positive overflow saturates to
`u32::MAX`. The random bindings consume `MultiValue` with Purple's exact tag
helpers, preserving lower-32-bit integer behavior and the existing CMWC/LCG
sequence. Regressions cover every rejected wrong tag, ignored trailing slot,
parse failure, overflow/negative conversion and unchanged state/value result.

The complete workspace passes all 622 tests (82 app/audio/wgpu, 31 assets,
one core and 508 script/physics); formatting, diff whitespace checks, strict
all-target/all-feature Clippy and the release build are clean. An isolated
copy of the supplied AppData completed a 180-frame release-wgpu upload,
render and readback with 19 optional data probes, zero invoked fallbacks and
zero remaining compatibility bindings. Its execution-evidence SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`f122acf448fe896e10ae85ad017ef5134e9afc9b3e0676fc37d4bbe3793f8937`
for `stella-app` and
`621f86f841aed0957ea6b5523f85e6b2185ce2b6d11ce228c6b2c9d33a68f92d`
for `stella-headless`.

## Process-global unique shaders and strict file probes

The late service audit found that `createUniqueShaders` is not one of the
generated tuple adapters. Its constructor publication at `0x10002E454`
installs handwritten member `sub_10004E720`. IDA and Hopper independently
show an exact STRING in slot one and exact NUMBER in slot two, followed by a
float32 narrowing and signed `FCVTZS W,S`. Trailing values are ignored. A
positive count produces a return table whose names concatenate the supplied
base with signed decimal values from process-global `dword_100C0FF88`; the
32-bit counter wraps and is not reset with a GameLua instance. The shader map
itself remains owned by that GameLua instance.

`destroyUniqueShaders`, published at `0x10002E484`, enters generated table
adapter `sub_100084204 -> sub_10008426C` before member `sub_10004EC88`. The
adapter requires an exact TABLE and ignores trailing arguments. The member
counts every key with `lua_next`, returns without work when the count is below
two, then raw-fetches integer indices `1..count-1`. Each fetched value follows
`lua_tolstring` semantics: strings and numbers become names, while other
values become the empty string. This unusual exclusive upper bound is
observable in the shipped `BlockHighlighter.lua`, which passes the complete
shader-name table rather than variadic strings. The former Rust variadic
binding consequently destroyed the wrong set and kept a per-runtime counter;
both lifetimes and the table traversal now match Purple.

The adjacent file probes use the shared generated Boolean/string adapter.
`checkForLuaFile` at `0x10002E778` reaches member `sub_1000504C0`, while
`fileExistsInAppData` at `0x10002F194` reaches `sub_10005A290`. Both require
an exact STRING in slot one, ignore trailing values and return one Boolean.
Their Rust bindings no longer admit mlua's numeric-to-string coercion.

Focused regressions cover fractional, negative, NaN and infinite shader
counts, contiguous signed names, the process-global counter across two
GameLua owners, strict table/string tags, ignored trailing arguments and the
native destroy traversal. The complete workspace passes all 622 tests (82
app/audio/wgpu, 31 assets, one core and 508 script/physics); formatting, diff
whitespace checks, strict all-target/all-feature Clippy and the release build
are clean. An isolated copy of the supplied AppData completed a 180-frame
release-wgpu upload, render and readback with 19 optional data probes, zero
invoked fallbacks and zero remaining compatibility bindings. Its unchanged
execution-evidence SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`888f482687b7af4897cac9deef74a4a0169747b31b565b7b8e40b8d46fc276e7`
for `stella-app` and
`05b015a26b5bd067f7f54e47178facc508343228afc645b81b3438489b8a5132`
for `stella-headless`.

## Hand-written time differences and stack-top joint parameters

The GameLua constructor publishes `getTimeDifference` and
`getTimeDifferenceInSeconds` at `0x10002EC70` and `0x10002EC90` as direct
members `sub_100056AB0` and `sub_100056C98`, not generated tuple dispatchers.
IDA and Hopper independently show both members extracting exact TABLE values
from slots one and two with `sub_100527F10`, without inspecting the remaining
stack. They feed both tables through the shared `sub_10005D700` calendar
converter before local `mktime`/`difftime`; the first member takes the
absolute difference and publishes four float32 components, while the second
publishes the signed float32 result. The Rust members now preserve the exact
two table tags while ignoring trailing values instead of rejecting them at
mlua tuple decoding.

`setJointParameters` is also registered directly, at `0x10002D30C`, as
hand-written member `sub_10003E890`. Its first operation is
`sub_100527F10(..., -1)`: the descriptor is the current Lua stack top. Earlier
arguments are ignored, an absent/non-table top fails, and this is observably
different from a generated slot-one adapter. The `name` field then follows
`lua_isstring`/`lua_tolstring`, accepting both strings and numbers. Optional
numeric joint fields first use `lua_isnumber` and therefore accept numeric
strings before `lua_tonumber` and float32 storage; optional Boolean fields
retain their exact Boolean-tag test. The Rust binding now follows this
stack-top and field-coercion sequence before its existing concrete joint-type
dispatch and Lua descriptor mirroring.

The neighboring LuaResources `getAvailableSystemFonts` and `getLocale`
registrations at `0x1004468DC` and `0x100446C38` use the generated no-argument
table/string dispatchers. Both disassemblers show no stack-count gate. The
existing Rust unit decoders were verified to ignore trailing values already,
so no member-side compatibility branch was needed.

Regressions cover strict time-table tags, ignored time tail values, missing
and non-table joint tops, ignored leading joint values, numeric joint names,
numeric-string scalar fields and ignored arguments on both no-argument
resource queries. The complete workspace still passes all 622 tests (82
app/audio/wgpu, 31 assets, one core and 508 script/physics); formatting, diff
whitespace checks, strict all-target/all-feature Clippy and the release build
are clean. An isolated copy of the supplied AppData completed a 180-frame
release-wgpu upload, render and readback with 19 optional data probes, zero
invoked fallbacks and zero remaining compatibility bindings. Its unchanged
execution-evidence SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`b33b113ec22d41f3ea9094f37c28dba66776c8fe082be6750aa6241fae47dac1`
for `stella-app` and
`62986eaaed8074edda536062cd02f23c1f581eca368e85b7d94b545ff735444f`
for `stella-headless`.

## Assets native/script ownership and strict clipping/loading adapters

The Assets constructor at `sub_1000AC118` has a deliberately small native
surface. IDA and Hopper independently show `loadFiles` being installed at
`0x1000AC1A8` through wrapper `sub_1000AD28C` and member `sub_1000AC25C`,
followed by `createSpriteSheet` at `0x1000AC1D4`. The binary contains the
corresponding strings at `0x1009425FC` and `0x100942606`, but contains no
`haveBeenDownloaded` or `getAssetFilename` registration strings. Those two
helpers are instead defined by the shipped
`scripts_common/cloud/rovioid/Assets.lua` after native publication. The Rust
constructor no longer preinstalls competing native implementations, so the
same ownership and boot-time replacement order now applies.

`sub_1000AD28C -> sub_1000AD2F4` requires an exact TABLE in slot one and does
not inspect trailing stack values. Member `sub_1000AC25C` pushes nil and
traverses the table with `lua_next`; every current value is extracted through
the exact STRING helper before the value is popped and the key retained. Thus
all entries participate regardless of key shape, and a number, Boolean or
other non-string value fails rather than being coerced. `Assets.loadFiles`
now mirrors that adapter instead of relying on mlua's typed-closure decoder.

The global `clipText` registration enters `sub_100086070 -> sub_1000860D8`
and member `sub_10004F630`. Both disassemblers show exact STRING values in
slots one and two, an exact NUMBER in slot three, float32 narrowing of the
width, no stack-count equality check and zero returned Lua values. Its Rust
adapter now performs those same indexed tag checks, ignores tail arguments
and preserves the existing native wrapping member behavior.

Focused regressions cover the pre-script Assets surface, script-owned helper
publication, exact load-table and entry-value tags, ignored load tail values,
exact clipping group/key/width tags and ignored clipping tail values. The
complete workspace passes all 622 tests (82 app/audio/wgpu, 31 assets, one
core and 508 script/physics); formatting, diff whitespace checks, strict
all-target/all-feature Clippy and the release build are clean. An isolated
copy of the supplied AppData completed a 180-frame release-wgpu upload,
render and readback with 19 optional data probes, zero invoked fallbacks and
zero remaining compatibility bindings. Its unchanged execution-evidence
SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`51be2a548eee7e3ce71cf8e52ccfeff5149f60cdc0f25d78d8da8f50773ca653`
for `stella-app` and
`ad6ef6b0b22aa573802e5490ea92ed355d96b55256258a0a9cadd290a362df9f`
for `stella-headless`.

## Align stack ABI, scale-derived position ratios and disconnected social adapters

The remaining typed native tuple audit identified `Align.getPositionAndScale`
as the only original GameLua member still relying on mlua's coercive tuple
decoder. Purple's Align constructor at `sub_1000E0A6C` publishes direct wrapper
`sub_1000E0B70`, which calls member `sub_1000E0CB0` and returns four values.
IDA and Hopper independently show an exact TABLE in slot one followed by exact
NUMBER values in slots two through five. Each number is narrowed to float32;
the wrapper does not inspect trailing stack values. Numeric strings are
therefore rejected at the outer boundary even though numeric fields inside the
layout table retain the native Lua 5.1 number coercion.

`InitFunc_16` at `0x1000E1EDC` confirms all comparison literals and their
storage order: `LEFT`, `RIGHT`, `TOP`, `BOTTOM`, `CENTER`, `NORMAL`, `SQRT`,
`SQR`, `NORMAL_SQRT`, `TRUE`, `UP`, `DOWN`, `FALSE`, `FREE`, `FIXED`, `UP` and
`PRODUCT`. The member hardcodes `FIXED` scale combination and `NORMAL`
post-processing. It first applies the independent TRUE/UP/DOWN scale
permissions, chooses the smaller permitted axis ratio, and multiplies each
authored scale by that ratio.

A second, observable step had been missing from the Rust port: the position
pass recomputes each ratio as `output_scale / authored_scale` instead of
reusing the viewport ratio. This normally yields the same value but preserves
the original rounding and produces NaN for a zero authored scale. The anchor
helper at `sub_1000E19C8` also exposes exact ARM64 staging. LEFT/TOP use one
`FMUL`; RIGHT/BOTTOM use `FSUB` then `FMADD`; CENTER uses `FMADD` for
`position + (-reference * 0.5)`, a separate `FMUL` by the ratio, then another
`FMADD` for `target * 0.5 + offset`. The Rust implementation now follows that
instruction order with float32 `mul_add` only at the two actual fused sites.

The adjacent SocialManager audit confirms that its existing disconnected
backend is intentional. `sub_1000C032C` reports connected only when the active
provider pointer at `+0xA0` is non-null; the offline host has none.
`native_connectToSocialNetwork`, `native_getFriendsProgress` and
`native_unloadAllAvatars` use the generated no-argument adapter and ignore all
stack values. Score, leaderboard, progress and avatar members enforce their
recovered indexed STRING/NUMBER tags while ignoring tails. Disconnected friend
and local IDs are empty strings, and `sub_1000C2E98` returns a newly allocated
empty friends table. No compatibility behavior was required, but regressions
now lock every strict and no-argument boundary.

Focused tests cover the five exact Align tags, ignored tail values, all anchor
modes, fixed-scale permissions and zero-scale NaN behavior, together with the
complete disconnected SocialManager surface. The complete workspace passes
all 622 tests (82 app/audio/wgpu, 31 assets, one core and 508 script/physics);
formatting, diff whitespace checks, strict all-target Clippy and the release
build are clean. An isolated copy of the supplied AppData completed a
180-frame release-wgpu upload, render and readback with 19 optional data
probes, zero invoked fallbacks and zero remaining compatibility bindings. Its
unchanged execution-evidence SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`11a62ee69162840989ad27efdb28d36e42e5c9a48424f3169e0d3b0bd85cc499`
for `stella-app` and
`3bffea67a8bc0d3988cdb700a20e5b24498b2df8af146ec412bddba75842b25e`
for `stella-headless`.

## Process-global signed screenshot sequence

The screenshot lifecycle audit had recovered the right native address but the
Rust state owner still contradicted it. Both IDA and Hopper show
`sub_10005ADD4` addressing `dword_100C0FF8C` directly at
`0x10005ADF4..0x10005AE04`: it loads one 32-bit process-global value,
increments it with a wrapping W-register `ADD`, and stores it before touching
the current GameLua or renderer. Reconstructing a GameLua/RenderBridge must
therefore not reset screenshot numbering.

The formatting signedness is observable too. At `0x10005AE1C..0x10005AE20`
Purple reloads the same W value and calls `std::ostream::operator<<(int)`
(`__ZNSolsEi`), not the unsigned overload. The successor of `INT_MAX` is
formatted as `-2147483648`; the successor of bit pattern `0xFFFFFFFF` becomes
zero. The existing per-RenderBridge `u32` field produced a positive large
filename and allowed a second runtime in the same process to overwrite
`Stella_Screenshot1.png`.

Rust now owns one process-global signed 32-bit sequence, applies the native
pre-format wrapping increment, and keeps only the already-numbered pending
requests in each renderer. `ScreenshotShareRequest.sequence` is signed to
match the filename and native stream input. Regressions lock strict STRING
title validation, ignored trailing Lua values, consecutive numbering across
two `StellaLua` instances, the `INT_MAX -> INT_MIN` filename and the
`-1 -> 0` wrap.

The complete workspace still passes all 622 tests (82 app/audio/wgpu, 31
assets, one core and 508 script/physics); formatting, diff whitespace checks,
strict all-target Clippy and the release build are clean. A fresh isolated
copy of the supplied AppData completed a 180-frame release-wgpu upload,
render and readback with 19 optional data probes, zero invoked fallbacks and
zero remaining compatibility bindings. Its unchanged execution-evidence
SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`a9a9be1eb6bffc8a251f03693919bc0b26808bec96629bfff201e13a14f34308`
for `stella-app` and
`ee4f142778d55b5801a3416fe5cde629c3ecd0e85603be26064af20c3863c725`
for `stella-headless`.

## Process-global pinch baseline and current level verification

The input audit found one remaining state-ownership mismatch in
`sub_1000293C8`. IDA and Hopper independently show the two-touch active byte at
`byte_100C0FF20` and its initial distance/scale at `dword_100C0FF24` and
`dword_100C0FF28`; none of those fields belongs to GameApp. The
current and previous zoom values remain GameApp fields at `+0x4FC` and
`+0x51C`. With exactly two touches, Purple initializes the three process
globals once, computes the live distance ratio in float32, copies current to
previous and writes the new scale. Leaving the exact-two-touch state clears the
static active byte and copies current to previous. Rust now preserves that
split ownership and the cross-runtime lifetime of an unfinished native pinch.
Parallel unit tests model each test thread as its own one-GameApp process so
unrelated synthetic runtimes cannot clear another test's static gesture.

The BirdRun L09 wheel and Chapter01 L61 pollen conclusions above were rechecked
against the supplied save and the shipped gameplay gate. The six authored
revolute mounts remain intact: deleting them or forcing the four circles to
fall would be a behavioral regression. The deterministic idle replay remains
settled with zero score, while the supplied before/after captures contain a
score increase and therefore do not demonstrate an unconditional idle
collapse. For pollen, the shipped L61 regression now calls `makeGolden` itself,
not the lower-level shader helper, and proves the resulting live
`2d-sprite-gold` table reaches ordinary native scene submission.

The complete workspace passes all 622 tests (82 app/audio/wgpu, 31 assets, one
core and 508 script/physics); formatting, diff whitespace checks, strict
all-target/all-feature Clippy and the release build are clean. An isolated copy
of the supplied AppData completed a 180-frame release-wgpu upload, render and
readback with 19 optional data probes, zero invoked fallbacks and zero remaining
compatibility bindings. Its unchanged execution-evidence SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
it is not treated as a visual oracle. Current release SHA-256 values are
`04fffb7bdbce6c0726a0c732ca5f6ae491811e9430cd5a9848c049032d9bf50f`
for `stella-app` and
`f72fbc824aa9dad70fbd5949a41c3653338875a047318965ce86fb4bd35d8f6e`
for `stella-headless`.

## Native BGM restart and forced termination persistence

The main-menu music restart after a desktop focus cycle is not an audio
decoder reset. IDA and Hopper independently show
`-[AppController applicationWillResignActive:]` at `0x100404B74` clearing
`m_allowUpdate` and entering `stopUpdate` at `0x100404E24`. `stopUpdate`
invalidates the display link, calls the App/GameLua active member at virtual
offset `+0x98` with false and then calls the audio-output member at `+0xB0`
with false. The GameLua path resolves through
`sub_100029BE8 -> sub_10005D4D4` and invokes the shipped `gamePaused`
callback. The active transition invokes the matching `gameResumed` callback.

Instrumenting those original shipped Lua functions confirms the remaining
observable sequence. `gamePaused` stops the audio named by
`previousMusicName`, clears that field and performs the playtime, settings,
highscores and BI persistence pass. `gameResumed` calls `handleLevelMusic`;
because the old name was cleared, `changeMusic` reaches `playAudio` with only
the music resource name and no playback position. The decoder therefore
starts from frame zero. Purple has no seek/resume handoff for this transition:
the restart is the original iOS background/resume behavior exposed by the
desktop host's focus-to-lifecycle mapping.

The adjacent termination audit found a real host omission.
`-[AppController applicationWillTerminate:]` at `0x1004047A8` clears
`m_allowUpdate` and, on Purple's normal target branch, calls `stopUpdate`
again even when an earlier resign-active notification already stopped the
display link. It then resets the default `Configuration` through
`sub_100401398` and destroys the controller. Because the GameLua active member
itself does not deduplicate false transitions, that second `stopUpdate`
delivers a final `gamePaused` and hence the final script-owned persistence
pass. The previous Rust close path exited winit without this callback.

The desktop host now routes winit's guaranteed `exiting` notification through
a forced `application_will_terminate` boundary. Ordinary repeated focus-loss
notifications remain deduplicated, while close, script-requested exit and
fatal shutdown all deliver the native final pause/audio-stop ordering. An app
regression boots the shipped scripts and proves one ordinary pause followed by
one additional termination pause; the GameLua regression separately proves
that two direct false dispatches remain observable and that both clear held
input before the callback.

The complete workspace passes 624 tests with one intentional ignored test (83
app/audio/wgpu, 31 assets, one core and 509 passing plus one ignored
script/physics test). Formatting, diff whitespace checks, strict
all-target/all-feature Clippy and the release build are clean. A fresh
180-frame release-wgpu upload, render and readback completed with 19 optional
data probes, zero invoked fallbacks and zero remaining compatibility bindings.
Its execution-evidence PNG SHA-256 is
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
the loading-frame image is not treated as a visual oracle. Current release
SHA-256 values are
`ec9c9c60c62f14e2fa4f33bfae888c95c5ac62aabfa492fef6ba3f77df584fb0`
for `stella-app` and
`f72fbc824aa9dad70fbd5949a41c3653338875a047318965ce86fb4bd35d8f6e`
for `stella-headless`.

## Native device information model publication

The startup audit found one remaining host-authored value in an otherwise
native-owned constructor path. Rust published the literal
`Stella Rust rehost` as `deviceInfoModel`, but Purple never manufactures a
product label there. In `sub_100026D2C`, instructions
`0x1000270B8..0x100027100` allocate the 32-byte `pf::DeviceInfo` facade,
construct it through `sub_10053B1E0`, obtain the model through
`sub_10053B334` and publish the resulting `std::string` with
`sub_10002BD74`. The independent `deviceModel` value remains the recovered
platform literal `ios`.

IDA resolves the virtual implementation to
`pf::DeviceInfo::DeviceInfoImpl::getModel` at `0x10053B404`. Hopper confirms
the same 224-byte procedure and eight basic blocks. The member first calls
`sysctlbyname("hw.machine", null, &length, null, 0)`. A return value of `-1`
or a zero length produces an empty string. Otherwise it allocates exactly
that length, zeroes the buffer and repeats `sysctlbyname`; the same failure or
zero-length branch frees the buffer and again returns empty. Only a successful
second call constructs the returned C string and frees the temporary buffer.

The recovered owner now lives in a separate `device_info` module instead of
the GameLua registration coordinator. Apple targets reproduce the exact
two-stage `hw.machine` query. Other Unix targets publish `uname.machine`, and
Windows targets publish the Rust target machine name with `aarch64` normalized
to `arm64`; these preserve the native hardware-identifier role without
injecting rehost branding into the shipped scripts. Unit regressions cover
both query stages, both failure/zero boundaries and NUL termination. A startup
regression proves `deviceInfoModel` receives the platform query result, and an
ARM64 MSVC metadata compile covers the non-Unix branch independently of the
host's unavailable Windows C toolchain.

The complete workspace passes 627 tests with one intentional ignored test (83
app/audio/wgpu, 31 assets, one core and 512 passing plus one ignored
script/physics test). Formatting, locked metadata, diff whitespace checks,
strict all-target/all-feature Clippy and the locked release build are clean.
The current macOS release publishes the queried `arm64` value. A fresh
180-frame release-wgpu upload, render and readback completes with 19 optional
data probes, zero invoked fallbacks and zero remaining compatibility bindings.
Its execution-evidence PNG SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
the image is not treated as a visual oracle. Current release SHA-256 values are
`5b04a56eea108dee6acbeb0e802fcb9ef1c74c2bdffc90bfc9c2825b417bcd0f`
for `stella-app` and
`5be691ed29cf7276651502f46f48a0df570b9548a0b526cd62677f192067505d`
for `stella-headless`.

## Script-owned physics and world scale publication

The next startup-global comparison removed two values that Rust published too
early. Neither IDA's complete string catalog nor Hopper's exact string search
contains a native `physicsScale` or `worldScale` literal. Both disassemblers
show the `sub_100026D2C` publication sequence at
`0x100026FC8..0x100027100` containing `res`, the eight resource paths,
`deviceModel` and `deviceInfoModel`; it then loads `starLimits.lua` and enters
the shipped common `gamelogic.lua` through `sub_10005CD58` at
`0x100027114..0x1000271B0`. There is no native scalar setter between those
operations. The prior host-created `_G.physicsScale = 20` and
`_G.worldScale = 1` were therefore visible at a point where Purple has no such
globals.

Instrumenting the original 1.1.6 bytecode with an environment `__newindex`
observer establishes the positive owner and value. The common gamelogic chunk
first creates `gamelua.physicsScale` with the binary64 value `0.05`; it does
not create `worldScale`. The native post-chunk `updateValues` call preserves
that value and still does not publish `worldScale`, and the complete shipped
boot has the same raw state. `worldScale` appears only when the native
`setWorldScale` member is subsequently called by the camera/gameplay path.
The old `_G` fallbacks had hidden all three boundaries and even exposed the
reciprocal value `20` before the original script wrote `0.05`.

Rust no longer injects either scalar from the registration coordinator. A
shipped-data regression proves both raw globals and both GameLua fields are nil
before the common chunk, observes the bytecode's sole first-stage assignment,
checks the post-`updateValues` and complete-boot states, and leaves
`setWorldScale` as the only native publisher of the later world value. The
complete all-level construction/update/draw regression also passes without
the convenience fallbacks.

The complete workspace passes 628 tests with one intentional ignored test (83
app/audio/wgpu, 31 assets, one core and 513 passing plus one ignored
script/physics test). Formatting, locked metadata, diff whitespace checks,
strict all-target/all-feature Clippy and the locked release build are clean. A
fresh 180-frame release-wgpu run asserts `physicsScale == 0.05`, raw
`worldScale == nil`, then proves an explicit native `setWorldScale(1)` publishes
the value. It completes with 19 optional data probes, zero invoked fallbacks
and zero remaining compatibility bindings. Its execution-evidence PNG
SHA-256 remains
`ed626e4c19182f75407128c5ba721a43f1d070ae8fe6d01c691772618bcdf683`;
the image is not treated as a visual oracle. Current release SHA-256 values are
`a2672cbd70b8d99603fc883e6ff1685dbcad683358fb068883caf0708ff3934e`
for `stella-app` and
`01fcfef92f4b1145fffe416fa57e3ebbdcc682f8add7b2690ef875d72b9986c5`
for `stella-headless`.

## Script-owned clocks and parameter-only frame deltas

Continuing the startup-global audit found five more values that Rust created
before Purple would. IDA has no exact `deltaTime` or `playtimeCounter` native
string. Its only exact `currentTimeStep` string is at `0x1009404C4`, and every
xref belongs to `sub_100032970` or `sub_10004B8EC`, which read the live
`objects.currentTimeStep` gameplay field for trajectory calculations. The
apparent `g_time` hits are unreferenced suffixes inside unrelated longer
strings such as `html5_endcard_loading_time`; byte inspection and both
disassemblers find no standalone publisher. Hopper's exact string search
independently returns only `time` and `currentTimeStep`, with neither attached
to the GameLua constructor or frame callback as a global setter.

The complete native frame path supplies the positive ABI. In
`sub_10005E898`, `0x10005EC7C..0x10005EC88` retains the incoming float32
delta, multiplies it once by GameLua's float32 time multiplier and stores the
raw/scaled pair in stack locals. After the native physics, scene-export and
service passes, `0x10006058C..0x1000605A0` calls
`lua::LuaObject::call<float,float>("update", scaled, raw)` directly. There is
no Lua-table setter for `g_time`, `deltaTime` or `currentTimeStep` anywhere
between those operations. Hopper recovers the same direct two-float call and
the same absence of an intervening field publication. The nearby native
global publication is instead the explicitly named
`g_physicsUpdateMillis` at `0x10005F228..0x10005F238`.

Instrumenting the original 1.1.6 bytecode closes the ownership question. The
common gamelogic chunk, its native `updateValues` continuation and a complete
boot all leave raw `time`, `g_time`, `deltaTime`, `currentTimeStep` and
`playtimeCounter` absent. On the first direct shipped `update(scaled, raw)`,
the script creates only `time = 0` and `playtimeCounter = 0`; the second call
adds the supplied frame delta to both. The other three fields remain absent.
This also distinguishes the global name from the separately authored
`objects.currentTimeStep` used by gameplay and the recovered trajectory
members.

Rust therefore no longer injects five zero-valued clock globals during
bootstrap and no longer writes three convenience globals before every Lua
update. The native host passes only the two float32-derived callback values;
the shipped script owns its two persistent clocks and their first-frame
creation boundary. Regressions observe the original bytecode assignments,
prove the pre-chunk/post-`updateValues`/complete-boot raw state, and verify
that a synthetic update receives the exact scaled/raw parameters without any
host-created clock fields.

## Native Assets service announcement and script facade

Extending the real-flow run beyond the initial loading screen exposed a
separate startup omission: a menu component eventually called
`Assets.haveBeenDownloaded`, but the Rust host had announced only the native
`social` cloud service. The function is intentionally absent from the native
Assets constructor and is supplied later by the shipped cloud facade, so the
1200-frame run stopped with a nil-function Lua error instead of entering the
menu normally.

IDA identifies the Assets vtable at `_ZTV6Assets` (`0x100A91080`), its RTTI at
`_ZTI6Assets` (`0x100A910E0`) and the exact service-name string `"assets"` at
`0x100942639`. The constructor `sub_1000AC118` registers only `loadFiles` at
`0x1000AC1A0..0x1000AC1C0` and `createSpriteSheet` at
`0x1000AC1CC..0x1000AC1EC`, then publishes the table as `Assets` at
`0x1000AC1F4..0x1000AC204`. Its service-name members `sub_1000ACB34` and
`sub_1000ACB60` both construct `"assets"`, while `sub_1000ACB18` invokes
`onEnableService`. Hopper independently recovers the same two 44-byte
service-name members, constructor publication and enable callback.

The shipped `RovioCloudManager.lua` handles
`EID_CLOUD_SERVICE_REGISTERED`, maps the `assets` name to
`scripts_common/cloud/rovioid/Assets.lua` and loads that file only after the
dispatcher exists. The facade defines `haveBeenDownloaded` and
`getAssetFilename`; it is deliberately distinct from native `_G.Assets` and
reaches native `loadFiles` through that explicit global table. Treating the
two tables as one would hide the original wrapper/native ownership boundary.

Startup now announces the recovered Channel, Assets and SocialManager service
subset in native construction order (`channel`, `assets`, then `social`) after
the shipped dispatcher is ready. The common announcer first checks
`RovioCloudManager.isServiceAvailable`, so replaying the boundary is
idempotent. A regression proves all three service facades load before menu
updates, the two Assets tables remain distinct, the native methods remain on
`_G.Assets`, and the facade accepts the original filename-table argument to
`haveBeenDownloaded`.

The complete workspace passes 629 tests with one intentional long-duration
test ignored (83 app/audio/wgpu, 31 assets, one core and 514 passing plus one
ignored script/physics test). Formatting, locked metadata, diff whitespace
checks, strict all-target/all-feature Clippy and the locked release build are
clean. A final 1200-frame release-wgpu run loads Chapter01 L50 after frame 300,
asserts the Assets facade and script-owned clock boundaries, and completes
with 77 optional nil probes, zero invoked fallbacks and zero remaining
compatibility bindings. The execution-evidence PNG SHA-256 is
`51cda78d54a2f9c5f0b01c6840f93c3d97d4b70722e37e5436533443b7857b33`;
the image is execution evidence rather than a visual oracle. Current release
SHA-256 values are
`865556232ea61f392a42859a7cd77c4e267baa8e90ecf91dbc6203b701bdfe09`
for `stella-app` and
`4fe5292e28d2df9c6cb650580b41118780c245abd4bc02a284b4ffba4941ab4d`
for `stella-headless`.

## Native input publication boundaries and script-owned screen

The constructor-global audit found a final cluster of values that Rust made
visible too early. IDA's `sub_10002C274` publication run shows
`screenWidth`/`screenHeight` at `0x10002F364..0x10002F3A0`, followed directly
by the retained `keyPressed`, `keyReleased`, `keyHold`, `cursor`,
`multitouchSweep`, `multitouchZoom` and `clippedText` tables at
`0x10002F3A4..0x10002F448`. The later native block republishes the two
dimensions and creates `g_startingResolutionWidth/Height` at
`0x10002F610..0x10002F6A4`. There is no constructor publication for a
provisional `screen` table, `touches`, `touchcount`, or any standalone pointer
event-name global between those instructions.

The string inventories close the event-name boundary. Neither IDA nor Hopper
contains exact native strings for `LPRESS`, `LHOLD`, `LRELEASE`, `RPRESS`,
`RHOLD`, `RRELEASE`, `HOVER`, `PRESS`, `RELEASE`, or `WHEEL`. The only exact
`LBUTTON` and `RBUTTON` strings are at `0x100986CDF` and `0x100986CEF`; IDA
places their sole references in the static key-name data table at
`0x100AA3748/0x100AA3758`, while Hopper reports no code xrefs. The shipped Lua
chunks own the pointer-event strings as bytecode constants. Publishing all
twelve as globals was a host convenience, not a Purple interface.

Positive ownership for the other fields is equally explicit. The original
common `gamelogic.lua` creates its raw `screen` table from the native
dimensions; an environment observer sees `left`, `top`, `right` and `bottom`
there while native `_G.screen` stays nil. The native frame member
`sub_10005E898` constructs the current touch table, caps its vector traversal,
publishes `touches` at `0x10005EC4C..0x10005EC5C`, converts the count to
float32 and publishes `touchcount` at `0x10005EC60..0x10005EC78`, immediately
before the scaled/raw delta path. Hopper independently recovers the same
ordering and setters.

The retained cursor also starts as an empty LuaObject. The actual native
position member `sub_100029F8C` checks the GameLua pointer and writes only
float32 `x` and `y` at `0x100029FB4..0x100029FE4`; Hopper's pseudocode matches
the two fields and contains no `down` setter. Button edges remain in the
native key tables. Rust now derives the prior primary-button state from the
retained `keyHold.LBUTTON`, publishes only cursor `x/y`, and uses the native
literal directly instead of performing a false Lua-global lookup.

Rust therefore no longer creates the twelve event globals, a provisional
native `screen`, constructor-time `touches/touchcount`, or cursor
`x/y/down` defaults. Regressions prove all event globals and the native screen
are nil at the relevant boundaries, the common bytecode owns screen creation,
the cursor begins empty and never receives `down`, and every native frame
replaces the touch table while preserving the two-touch cap, signed low-id
format and float32 count.

The complete workspace passes 630 tests with one intentional long-duration
test ignored (83 app/audio/wgpu, 31 assets, one core and 515 passing plus one
ignored script/physics test). Formatting, locked metadata, diff whitespace
checks, strict all-target/all-feature Clippy and the locked release build are
clean. A final 1200-frame release-wgpu run loads Chapter01 L50 at frame 300
and injects a complete press/release at frame 1100. It asserts every removed
event global, native `_G.screen`, frame-owned touches/count, script-owned
screen, cursor `x/y` without `down`, released `keyHold.LBUTTON`, the Assets
facade and the script-clock boundary. It completes with 80 optional nil
probes, zero invoked fallbacks and zero remaining compatibility bindings. The
execution-evidence PNG SHA-256 is
`4ff2625b474421a237e130b4798908c242a18be4ce2011d1a568a8a235377239`;
the image is execution evidence rather than a visual oracle. Current release
SHA-256 values are
`7cba7b2570aaa7a691798e714f4f52aee8fbb5049a7a7dd9524d85ef90a17416`
for `stella-app` and
`bb86a06a83d4bb38818dce0e7207e92292a915ee96df41d872c718132f11d5cb`
for `stella-headless`.

## Zappar native service and unsupported-host close continuation

Opening the Zappar entry from the shipped map UI exposed a missing native
table rather than a script defect. `options.lua` enables `g_use_zappar` for
the Apple build, `game_init.lua` consequently loads `ZapparHandler.lua`, and
the handler calls `_G.Zappar.native_launchZappar(callback)` after stopping all
audio and unloading its static clips. Because the Rust constructor had never
published `_G.Zappar`, the pointer event stopped at the table lookup and the
audio-restoration callback could not run.

IDA recovers the native constructor at `sub_1000E3468`. It registers exactly
`native_isZapparSupported` at `0x1000E34C8` and `native_launchZappar` at
`0x1000E34F4`, then publishes the table under the exact name `Zappar` through
`sub_1000E37D4` at `0x1000E3510`. The aggregate GameLua constructor calls the
class constructor at `0x10002C6B0`. The support member thunk
`sub_10085CD5C` delegates directly to Objective-C
`+[ZapparEmbed isDeviceCompatible]`.

The launch lifecycle is asynchronous in the original app. Adapter
`sub_1000E38CC` reads the required Lua function from argument slot one and
returns zero Lua results. `sub_1000E3558` captures that function and calls
`sub_10085CD74`, which creates the Zappar view controller, installs an
`onClosed` block and presents it. The close block at `sub_10085CF0C` invokes
the captured continuation. Decompiling the original `ZapparHandler.lua`
confirms why this is required: the continuation reloads static audio assets,
restores the saved audio-enabled flag and music name, restarts or stops the
audio output as appropriate, and resumes the prior music.

The desktop rehost has no Zappar camera SDK or compatible presentation
controller. It now publishes the exact two-member `Zappar` table, reports
`native_isZapparSupported()` as false, validates the required launch callback
and immediately completes the native close continuation. Immediate completion
is the unsupported-host equivalent of presenting and closing the unavailable
view; a silent no-op would leave the game audio disabled. Regressions verify
the table/member ABI, zero-result launch contract, callback validation and
single invocation. A second test executes the shipped bytecode handler itself
and proves its complete stop/unload/event/reload/audio-volume/music restoration
sequence finishes without a nil-table error.

The complete workspace passes 632 tests with one intentional long-duration
test ignored (83 app/audio/wgpu, 31 assets, one core and 517 passing plus one
ignored script/physics test). Formatting, locked metadata, diff whitespace
checks, strict all-target/all-feature Clippy and the locked release build are
clean. A 600-frame release-headless run against the complete runtime data then
calls the shipped `ZapparHandler.launchZappar()` path and asserts the native
table, unsupported result and restored audio/music state. It completes with
zero invoked fallbacks and zero remaining compatibility bindings. Current
release SHA-256 values are
`c61172600a84c7cc7736c5944d006e60762df7e80cad4161f61bd7adeb19464e`
for `stella-app` and
`e1c84f5cc3f94963a670c962fc764c8960c6bcb26d531d1f33235a4895e6ca61`
for `stella-headless`.

## Rovio Channel and Toons service boundary

The shipped Toons button is a frontend for the old Rovio Channel SDK, not a
local movie player. Decompiling
`scripts/BlockComponents/UIComponents/ToonsTvButton.lua` shows that both its
visibility and click path are guarded by `rovioChannel.isAvailable()`. A click
calls `rovioChannel:openView("", "map_screen")`. The common
`RovioCloudManager.lua` does not construct that facade at initial script load;
it waits for the native `channel` cloud-service announcement, then loads
`ChannelManager.lua`, `ChannelButton.lua` and `ChannelIntroPopup.lua` and
constructs the global `rovioChannel` manager.

IDA recovers the native table constructor at `sub_1000AD450`. It registers
exactly seven members: `openChannelView` at `0x1000AD554..0x1000AD574`,
`cancelChannelViewLoading` at `0x1000AD580..0x1000AD5A0`,
`updateNewContent` at `0x1000AD5AC..0x1000AD5CC`, `numOfNewContent` at
`0x1000AD5D8..0x1000AD5F8`, `onMenuInitialised` at
`0x1000AD604..0x1000AD624`, `isAvailable` at
`0x1000AD630..0x1000AD650`, and `isChannelViewOpened` at
`0x1000AD65C..0x1000AD67C`. The constructor publishes the resulting table as
`RovioChannel` at `0x1000AD684..0x1000AD694`. Hopper independently recovers
the same seven registrations and publication name.

The generated adapter `sub_1000AEFE0` establishes the exact open-call ABI. It
reads strings from Lua slots 1, 2 and 3, numbers from slots 4 and 5, and
strings from slots 6 and 7 before invoking the bound member, with zero Lua
results. These values are respectively the game id, view mode, locale,
viewport width, viewport height, content path and entry point supplied by the
shipped `ChannelManager:openView`. The other six registered adapters take no
arguments, ignore trailing Lua values and return either zero values, one
numeric content count, or one boolean.

The null behavior is the constructor's pre-enable state, not Stella's final
service state. The native Channel SDK pointer is stored at
`RovioChannel+0x38`. `sub_1000AE178` implements `isAvailable` as a single
non-null comparison. `sub_1000AE188` returns false without querying view
state when that pointer is null; `sub_1000ADBAC` returns zero new items; and
`sub_1000AD7D8`, `sub_1000ADB70`, `sub_1000ADB80` and `sub_1000ADBC0` skip
their SDK work in that temporary state. IDA and Hopper both show the missing
lifecycle edge at `sub_1000AE354`: the cloud service enable callback allocates
a `0x110`-byte Channel client with `sub_1005DEACC`, retains it and writes it to
`+0x38` before invoking the script-side `onEnableService` hook. The aggregate
cloud constructor registers `RovioChannel` at `0x1000B0198`, followed by
Assets at `0x1000B0308` and SocialManager at `0x1000B0478`.

The original iOS bundle includes the native viewer chrome under
`skynestdata/images/channel`, three transition sounds and the other shared
Skynest resources. The Channel constructor receives the literal root
`skynestdata/images/channel` in both ordinary-open paths
(`sub_1005E05C4` and `sub_1005E1614`). The close control constructor
`sub_100602E9C` loads `/close.png` and `/close_press.png`; the video-player
setup `sub_100608FF4` loads `/share_vid_player.png`; and
`sub_1006097B4` selects `/age_rate_{s,7,12,16,18}.png` from the Finnish age
rating code. The local runtime now carries the original 404-file
`skynestdata` tree and `channel_push_notification.wav` at those recovered
relative paths.

The episode catalog, HTML front end, promotional sprite sheet and videos were
remote; embedded binary configuration points to the retired
`cloud.rovio.com/channel/1.2` and `toons.tv` services. In particular,
`ChannelButton.lua` and `ChannelIntroPopup.lua` name sprites that were supplied
by the downloadable island-map promotion sheet, while `TOONS` and
`PANAMA_AD` are nil in the installed payload. Those local files cannot
reconstruct the missing catalog or video payload. The desktop rehost now
publishes the complete seven-member table, enables its SDK state before the
Lua service announcement like `sub_1000AE354`, and keeps the shipped map and
chapter Toons objects visible. A failed open completes the original
`onChannelLoadingFailed` continuation so the connection overlay cannot remain
stuck against a dead endpoint. Missing promotional sprite names fall back to
role-equivalent bundled Toons/button atlas regions only while the exact name
is absent; a recovered downloadable sheet retains the native last-entry-wins
priority and replaces every fallback automatically.

Regressions cover all seven members, the strict seven-argument open adapter,
ignored trailing values, zero-result command ABI, the pre-enable null state,
the post-announcement enabled state, the retired-request failure continuation
and all twelve promotional sprite fallbacks. A complete original-data boot
proves the `channel` service is announced, the three shipped facades and
native callback slots are installed, and the Toons service is available to
the original map scripts.

## QR/Telepods scanner and cross-promotion store launcher

The next real-script service audit found two native tables that were still
absent from the Rust constructor. `scripts/telepods/telepods.lua` explicitly
distinguishes a missing `_G.QrScanner` from a present scanner whose camera is
unsupported: `Telepods.areSupported()` calls `isCameraSupported()`, while
`Telepods.hasFrontCamera()` calls `isFrontCameraSupported()`. The shipped
`TelepodPage.lua` starts the scanner on entry, retains a recognized-code
callback and clears it with `nil` on exit. Separately,
`CrossPromotionButton.lua` calls `AppStoreLauncher.updateGameData(file)` when
its downloaded metadata arrives and `launchAppStore()` on click.

IDA recovers the complete `QrScanner` constructor at `sub_1000DDCFC`. It
registers exactly five members: `isCameraSupported` through member
`sub_1000DE050`, `isFrontCameraSupported` through `sub_1000DE094`, `start`
through `sub_1000DE0C0`, `stop` through `sub_1000DE1A8`, and the direct Lua
member `setQrRecognizedCallback` at `sub_1000DE1F4`. Publication under the
exact `QrScanner` name occurs at `0x1000DDEE0..0x1000DDEF0`. Hopper
independently recovers the same five registrations, object offsets and table
name.

The camera predicates establish the unsupported branch precisely.
`sub_1000DE050` first calls the platform camera-device probe
`sub_100536C24`; when no device exists it returns false without querying a
side. Otherwise it accepts camera side 2 or side 1. `sub_1000DE094` uses the
same no-device guard before testing side 2 only. `start` allocates a native
scanner session for the selected side, falling back from side 2 to side 1;
without a device it allocates nothing. `stop` releases that session and
restores side 2. The generated boolean adapters return one value, while the
start/stop adapters return zero values and ignore trailing stack entries.

The callback member is deliberately more permissive than an ordinary strict
adapter. `sub_1000DE1F4` tests Lua slot one with `isFunction`: a function is
strongly retained at object offset `+0x88`, while `nil`, a missing slot, or any
other Lua tag clears the old reference without error. The recognition event
member `sub_1000DDFF0` invokes that retained function with the decoded string
only when the platform event reports success. The desktop host publishes the
exact table and defaults to the native no-device branch. A host-provided code
source can explicitly advertise a virtual scanner: `start` retains the native
session state, a queued code is delivered only after the callback exists, and
`stop` prevents delivery. It never claims a front-facing camera. This keeps an
ordinary run on the original false-camera branch while allowing deterministic
desktop or future platform scanner backends without adding a sixth Lua member.

Following the real Telepod page beyond camera recognition exposed a second,
larger missing native boundary. `TelepodPage:onQRRecognized` stops the scanner
and calls the shipped `IAP.redeemCode`; a successful response does not directly
unlock a bird. It first fetches the provider wallet, delivers the product and
only then reaches `TelepodPage:onPurchaseDone`. IDA recovers the IAP constructor
at `sub_1000CC6C8`. Hopper independently confirms all eight registrations:
`native_buyItem`, `native_restorePurchases`, `native_getAvailableItems`,
`native_isPaymentInitialized`, `native_fetchWallet`,
`native_useWalletValidation`, `native_redeemCode` and
`native_refreshCatalog`. The constructor reads `g_iapBundleId`, publishes the
table as `IAP`, and `sub_1000CE2A0` calls the shipped
`registerPaymentCallbacks` exactly once.

The provider-success continuation `sub_1000CE5C4` calls wallet fetch
`sub_1000CEDAC`, invokes Lua `onPaymentInitialized(bundleId)`, and only then
publishes its completed state. `native_useWalletValidation` is not a guessed
policy: direct member `sub_1000CDD54` returns literal true. Redeem member
`sub_1000CDD5C` installs success and failure continuations before entering the
provider. Success continuation `sub_1000CF178` calls
`onRedeemResponse(code, "CODE_OK", productId)`. Failure continuation
`sub_1000CF278` maps provider errors -31 through -37 and -101 to the shipped
`CODE_*` strings, using `INVALID_CODE` for every other status.

Finally, wallet processor `sub_1000CF4E4` establishes the non-obvious delivery
order. For an ordinary voucher it calls `deliverItem(productId)`, optionally
marks the voucher delivered, then calls
`onWalletProcessVoucher(voucherProductId, productId, source)`. Decompiling the
original float32 Lua 5.1 `iap.lua` confirms that a successful redeem moves its
listener from the scanned code to `productId`; the wallet callback completes
that product with `PURCHASE_SUCCEEDED`. Reversing only the QR table would
therefore leave every visible scan stuck before character delivery.

The retired RCS voucher and mobile-store providers cannot be contacted by a
cross-platform offline release. The rehost nevertheless preserves the exact
eight-member table, initialization order, strict string adapters, empty store
catalog, wallet-validation result, `CODE_NOT_FOUND` failure shape and full
wallet callback chain. Its deterministic provider accepts only one of the 24
configured `hasbro.telepod.*` identifiers (or a host payload containing that
exact token), never guesses arbitrary numeric codes. `--telepod-code` exposes
the virtual scanner and queues such a payload until the shipped Telepod page
registers its callback.

IDA places the separate two-member `AppStoreLauncher` constructor at
`sub_10009E2B8`. It registers `updateGameData` at
`0x10009E310..0x10009E330`, `launchAppStore` at
`0x10009E33C..0x10009E35C`, and publishes `AppStoreLauncher` at
`0x10009E364..0x10009E374`; Hopper agrees. Generated adapter
`sub_10009EAC4` requires an exact string in slot one, ignores trailing values
and returns zero results. Its bound member `sub_10009E44C` calls the already
recovered `sub_1000512D8` text pipeline with encrypted-resource, alternate-key
and no-decompression flags `(true, true, false)`, parses the JSON, copies the
string fields `launchId` and `storeId`, and caches the platform
`canOpenURL(launchId)` result.

`sub_10009E940` then opens `launchId` directly when that cached result is
true. Otherwise it sends `storeId` and the literal product type 3 to the
native store launcher, with the legacy URL form as the platform fallback.
The cross-platform offline host has no mobile application registered for the
launch scheme. It therefore reuses the existing alternate-key text pipeline,
retains both metadata values, caches the native false-installed state and
records the exact `(storeId, 3)` request in the host bridge rather than
causing an external process side effect. This table alone does not invent
promotion content: the shipped button still also requires the retired remote
Assets payload before becoming visible.

Regressions cover all three complete member inventories, strict/ignored argument
boundaries, zero-result command ABI, callback function/clear semantics,
predecoded AppData promotion metadata and the recorded type-3 store request.
A second test boots the original 1.1.6 chunks and proves their real
`Telepods.areSupported()` and `hasFrontCamera()` calls observe the native
false-camera branch. A scanner/wallet regression then queues a host code and
proves all 24 configured products map to the correct shipped character and
complete through `PURCHASE_SUCCEEDED`; an unknown payload completes through
`CODE_NOT_FOUND`. The complete workspace passes 679 tests with one
intentional long-duration idle test ignored (85 app/audio/wgpu, 31 assets,
one core and 562 passing plus one ignored script/physics test). Formatting,
locked metadata, diff whitespace checks, strict all-target/all-feature Clippy
and the locked release build are clean. A final 600-frame release-headless run
against complete `runtime/data` asserts the QrScanner, Telepods,
AppStoreLauncher and RovioChannel boundaries and completes with zero invoked
fallbacks and zero remaining compatibility bindings. Current local release
SHA-256 values are
`cb57fb0f1d9620dddbf31db5109cb83bcbe221d4d2fb4e062d314ee7b03153bc`
for `stella-app` and
`256bdaec658c20d31bbcbac5ea8a3c6b8aafc4a4b5c9cbfc9318aa9b97e2b88b`
for `stella-headless`.

## Skynest account/storage and Ads facades behind the Toons cloud dispatcher

The follow-up Toons audit widened the native cloud boundary rather than
inventing a local video catalog. `RovioCloudManager` registers nine native
service types in this order inside `sub_1000AF9C0`: Analytics at
`0x1000AFAD8`, remote notifications at `0x1000AFC2C`, account at
`0x1000AFD60`, storage at `0x1000AFEAC`, ads at `0x1000B001C`, Channel at
`0x1000B0198`, Assets at `0x1000B0308`, SocialManager at `0x1000B0478` and
server time at `0x1000B05DC`. Rust now announces the recovered account,
storage and Ads services before the already implemented Channel/Assets/Social
run, preserving their relative native order. This lets the original
dispatcher load the same account, storage and Ads bytecode facades used by
the map, settings, challenge nickname/avatar, cloud-sync and Toons paths.

IDA recovers the complete account constructor at `sub_1000A744C`; Hopper
independently recovers the same object layout, registrations and publication.
It publishes `SkynestAccount` and registers exactly ten members:
`native_getServiceName`, `native_isLoggedIn`,
`native_isLoginInProgress`, `native_getAccountDetailsUrl`, `native_login`,
`native_logout`, `native_loginWithSocialNetwork`, `native_unRegister`,
`native_hasNickname` and `native_validateNickname`. The service-name vtable
slot `sub_1000A7D28` returns the literal `identityLevel2`, while
`sub_1000A7964` returns `https://account.rovio.com`. Adapter
`sub_1000A8C1C` requires three exact booleans for login; the command adapters
ignore trailing values and return no Lua values.

Nickname validation has a non-obvious asynchronous ABI. Adapter
`sub_1000A8998` requires a string in slot one and a LuaFunction in slot two.
The failure completion at `sub_1000A7D80` calls `callback(false)`; the success
completion at `sub_1000A7EEC` calls `callback(true, isValid)`. A second
counterintuitive contract was confirmed in both disassemblers:
`sub_1000A3D68`, exported as `native_hasNickname`, actually returns
`profileNickname.empty()`, so the observable result is true before a nickname
exists and false afterwards. The rehost preserves both shapes. Because the
validation server no longer exists, it supplies a bounded local validator
through the native success shape so the shipped challenge nickname flow can
finish instead of leaving its spinner pending.

The original account manager maps backend error 5 to `ERROR_OTHER` in
`sub_1000A3BA0`, and `sub_1000A4578` forwards the mapped code and provider
message to `_G.SkynestAccount.onLoginFailure`. The offline host never reports
a false login success: both login entry points complete through that exact
two-string failure route, while `native_isLoggedIn` and
`native_isLoginInProgress` remain false. Logout and unregister retain their
zero-result native command boundary. This is important because a silent login
no-op would leave the original connection screen and global login flag stuck.

The storage constructor at `sub_1000B9B38` publishes `SkynestStorage` with
exactly seven members: `native_loadCloudSettings`,
`native_saveCloudSettings`, `native_setRequestTimeout`,
`native_isTransactionInProcess`, `native_setKey`, `native_getKey` and
`native_getKeyForAccountIds`. Vtable slot `sub_1000BADF0` returns the service
name `storage`. The generated adapters establish the argument shapes:
set-key consumes string/string/function, get-key consumes string/function,
and multi-account get consumes string/table/function. The multi-account
member walks only the contiguous string sequence in the supplied table.

Native success/error completions recover the result protocol precisely.
Both set-key branches invoke the callback with zero arguments; get-key
success supplies one string while failure supplies none; multi-account
success supplies one account-to-value table while failure supplies none.
The retired backend is replaced by session-local key storage so nickname and
avatar operations complete, missing keys take the native zero-argument
failure shape, and multi-account lookup completes with an empty success table
for the signed-out host. Cloud load/save return false and transaction state
remains false because there is no authenticated remote state to start. The
original `SettingsWrapper` remains responsible for persistent local nickname
and avatar settings; no remote account or cloud success is fabricated.

IDA recovers the complete Ads constructor at `sub_1000A8EEC`; Hopper agrees
on its registrations, publication and object layout. It publishes `RovioAds`
with exactly nine members: `refresh`, `addPlacement`,
`addPlacementWithGeometry`, `addPlacementNative`, `show`, `hide`, `click`,
`trackConversion` and `startSession`. Vtable slot `sub_1000A9540` returns the
service name `ads`. Generated adapter `sub_1000AB0BC` is the strict one-string
boundary, `sub_1000AAE34` consumes a string followed by four numbers and
converts the geometry to native float coordinates, `sub_1000AAC14` maps
`show(string)` to one boolean, and `sub_1000AAB9C` is the zero-argument
command adapter.

All nine members dispatch through the provider pointer at object offset
`+0x48`. The desktop/offline host has no retired advertising provider, which
is a normal native state: commands return no Lua values and do nothing, while
`show` returns false. It does not synthesize placements, impressions or
rewards. The Toons connection is explicit in the executable rather than an
inference from filenames: `sub_1000A9998` routes the provider action ending
in `opentoons` to the Lua callback `adOpenToons`. The Rust startup announces
the native service and then loads the original `cloud/ads/Ads.lua` facade;
this compensates only for the rehost's later native-table installation order
and preserves the shipped `g_use_ingame_ads` gate for constructing
`adSystem`.

Regressions cover all three exact member inventories, strict tags, ignored
tails, all callback arities, the inverted nickname predicate, local
write/read, signed-out facades, the null Ads provider and one-shot
cloud-service announcement. A complete original-data boot proves that
`identityLevel2`, `storage`, `ads`, `channel`, `assets` and `social` are
available to the shipped dispatcher and that the original
account/storage/Ads/Toons facades are installed. The workspace passes 637
tests with one intentional long-duration idle test ignored (83
app/audio/wgpu, 31 assets, one core and 522 passing plus one ignored
script/physics test). Formatting, locked metadata, diff whitespace checks,
strict all-target/all-feature Clippy and the locked release build are clean.
A final 600-frame release-headless run validates the signed-out account,
nickname callback, local storage callback, null Ads provider and null-Channel
Toons branch with zero invoked fallbacks and zero remaining compatibility
bindings. Current local release SHA-256 values are
`4391534c66a59129efc016456878564ba7150673e3ed95843d0d87f1d35d8718`
for `stella-app` and
`a14c86c36fd203b3e9df5ce02cab1352f9e23a798b69dfc96a7f9436b75f4954`
for `stella-headless`.

## Complete nine-service RovioCloudManager publication

The follow-up startup audit found that the native tables were no longer
falling through compatibility bindings, but the service dispatcher still did
not reproduce Purple's complete registration set. Rust announced the middle
six services only, so the shipped `RovioCloudManager.isServiceAvailable`
returned false for three services which are unconditionally registered by the
original constructor.

IDA recovers the exact sequence inside `sub_1000AF9C0`: AnalyticsManager at
`0x1000AFAD8`, RemoteNotificationsService at `0x1000AFC2C`, account at
`0x1000AFD60`, storage at `0x1000AFEAC`, Ads at `0x1000B001C`, Channel at
`0x1000B0198`, Assets at `0x1000B0308`, SocialManager at `0x1000B0478` and
ServerTime at `0x1000B05DC`. Hopper independently identifies the same nine
templated `registerService` calls in the same order.

The virtual service-name leaves remove any ambiguity about the event payloads:
`sub_1000AB6BC` returns `analytics`, `sub_1000A0188` returns `push`, and
`sub_1000BD540` returns `time`; Hopper's pseudocode agrees for all three.
RemoteNotificationsService is intentionally event-only at this boundary. Its
constructor `sub_10009F384` installs platform-event listeners but publishes no
Lua global table, whereas Analytics and ServerTime already have their native
Lua tables. The offline rehost therefore emits the `push` service registration
without inventing a notification provider or a fake Lua API.

Rust now publishes all nine names in native order, preserves one-shot service
map insertion, and keeps the initial account completion after every facade has
had a chance to observe its enable event. A real original-data boot verifies
all nine `isServiceAvailable` results, the account-loading screen completion,
and a 600-frame run with zero invoked compatibility fallbacks and zero
remaining compatibility bindings.

## Chapter 02 L02 trap-sucker capture window

The shipped `TrapSucker.lua` does not implement a radial suction force or a
distance falloff. It creates an invisible density-zero `TrapSuckerSensor` box
at the intake, moves that box to the first chain segment's `suckingOffset`
every component update, and traps the exact object delivered by its contact
callback. The authored fixture is 0.5 by 0.5 world units. Its later visual
scale of 0.08 is unrelated to collision geometry.

IDA confirms the native geometry at `sub_100034740`: `createBox` halves both
full dimensions before Box2D's `SetAsBox`, and density zero selects a static
body. `sub_10003FA60` reaches `b2Body::SetTransform` at `sub_10086B794`, which
synchronizes fixture proxies and immediately calls `UpdatePairs` but does not
wake either endpoint. The decisive gate is visible at
`0x10086BAFC..0x10086BB28` in `sub_10086BAB0`: `ContactManager::Collide` calls
`Contact::Update` only if at least one endpoint has both its awake bit set and
a non-static body type. Therefore a moving static sensor may have a valid
broad-phase pair with a sleeping building without ever producing BeginContact.

This is the Chapter02_L02 failure reproduced in the rehost: the sensor tightly
overlapped `BLOCK_WOOD_1X10_1_9`, but the right-hand structure had settled to
sleep before the slightly divergent trap chain reached it. The previous 1.4
fixture enlargement was not the underlying fix and has been removed. The
compatibility path now retains the exact 0.5 by 0.5 fixture and, only for a
moving `TrapSuckerSensor_*`, wakes a sleeping dynamic endpoint after an exact
tight-fixture overlap. Fat-AABB proximity is insufficient, and every ordinary
SetTransform call retains the no-wake native behavior. The following 30 Hz
contact step then takes the unmodified Collide/BeginContact path and the shipped
`blockTrapped` callback disables gravity/collision and starts its suction tween.
A real Chapter02_L02 regression verifies the original fixture dimensions and
capture of the sleeping right-hand structure.

## Animation playback update split at native procedure boundaries

The playback update implementation had grown into one Rust source file even
though Purple divides the same work across separate native procedures. IDA
identifies `sub_100411230` (0x1a4 bytes) as control-time advancement and
completion handling, `sub_10041E41C` (0x138 bytes) as entity-target selection
and property application, and `sub_100016FE4` (0x17c bytes) as the queued Lua
event snapshot/dispatch boundary. Hopper independently reports the same
procedure extents and call separation.

The Rust module now follows those boundaries: `update/advance.rs` owns native
float32 time advancement, repeat/end/seek completion and zero-duration rules;
`update/apply.rs` owns winning-state selection and property publication;
`update.rs` retains event queue snapshotting, six-value Lua callback dispatch
and public registration; and `update/tests.rs` keeps the behavioral
regressions beside this unit. The only shared entry points are visible to the
parent `playback` module, matching their use by the separate controls adapter
without widening the crate API. All eight focused regressions pass unchanged,
including large-delta repeat truncation, latest shared completion mode,
float32 upper-bound behavior, newer-state precedence and callback-queued event
deferral.

The adjacent control-registration source now follows the same layout. Both
disassemblers delimit `sub_100012F18` as the 1,148-byte start member,
`sub_100013720` as the 252-byte named/scene stop member,
`sub_100012910` as the 232-byte global stop member, `sub_100013A98` and
`sub_100013B9C` as pause/resume, and `sub_100013D08` plus
`sub_10001396C` as speed/seek. Rust keeps the small ordered registration
facade in `controls.rs`, places the start path in `controls/start.rs`, both
stop paths in `controls/stop.rs`, retained-state members in
`controls/state.rs`, and the shared target-application bridge in
`controls/helpers.rs`. Publication remains in the exact order recovered at
`0x10000EE54..0x10000EF88`; all four focused control regressions pass without
changing playback behavior.

One superficially suspicious ordering detail is intentionally retained.
`sub_100410EA0` searches the active-control pointer vector, exchanges the
matched entry with the last pointer at `0x100410F84..0x100410F90`, and then
shrinks the end pointer. It is a swap-with-last removal, not order-preserving
`vector::erase`. The Rust named-stop path therefore correctly uses
`swap_remove`; a three-control regression now proves that stopping the first
entry leaves `[last, middle]`, matching both IDA and Hopper rather than
silently changing target-state precedence.

The 888-line desktop SystemFont raster source was also split at the native
platform boundary already recovered for `game::SystemFont::Impl::drawString`
(`0x100475B98`). `system_font.rs` now retains NSString-layout-equivalent
measurement, label dimensions, mask allocation, anchor selection and the
final cached-label payload. `system_font/raster.rs` owns the operations Purple
delegates beyond that member to UIKit/CoreGraphics: embedded sbix/bitmap
decoding, outline construction, stroke/fill mask compositing and color-raster
sampling. The pixel and cache regressions moved unchanged to
`system_font/tests.rs`. This reduces the coordinator to 198 lines while
preserving the separately recovered LabelPool/cache and COLR paint modules;
all fourteen focused SystemFont/COLR tests pass pixel-for-pixel.

The whole-bundle level regression now accepts `STELLA_ALL_LEVEL_FRAMES` while
retaining one frame as its normal fast default. This made it possible to run
all 149 extracted level containers for 120, 600 and finally 3,600 consecutive
60 Hz updates per level before their native draw checkpoint. The one-minute
per-level sweep completed in 54.36 seconds without a Lua exception, unresolved
sprite, invoked fallback or remaining compatibility binding. As with the
existing construction audit, this validates delayed component/physics
lifetime stability rather than claiming that every authored solution path was
played.

The complete workspace now passes 645 tests with one intentional long-idle
test ignored (83 app/audio/wgpu, 31 assets, one core and 530 passing plus one
ignored script/physics test). Formatting, diff whitespace checks and strict
all-target/all-feature Clippy are clean. A final 600-frame wgpu upload/render/
readback completes with an empty stderr log, zero invoked fallbacks and zero
remaining compatibility bindings; its execution-evidence PNG SHA-256 is
`ebe8ceb9cb49a2771b44b1620191c0e6987b42a441b9675c723e6b5cbc72bcbe`.

## Chapter 2 themed-terrain alpha-mask coordinates

The Chapter 2 rock terrain is not a collection of ordinary pre-textured
sprites. `level_load.lua` applies each authored `themeTexture` with
`setTexture`, then applies `(theme.textureScale or 1) * gameWorldScale` with
`setTextureScale`. For `theme_hometree_bottom` this produces the shipped
float32 value `1.075 * 0.0867 = 0.0932025`. The atlas sprite such as
`BLOCK_STATIC_ROUND_8X8` is only the alpha mask; its color comes from the
repeating `THEME_HOMETREE_BOTTOM_TEXTURE_1` image.

IDA shows the complete native branch in `sub_10004BAB4`. `setTexture`
(`sub_10004CC74`) retains the image pointer at `RenderObjectData+0x80`, while
`setTextureScale` (`sub_10004CE38`) stores its float32 scalar at `+0xC4`.
Objects without that pointer use the ordinary `sub_10006D5B4` path. Textured
objects instead reach `sub_10008D428` at `0x10004C1D4`, passing position times
20 divided by object scale and object scale divided by texture scale. In
`sub_10008D428`, the latter quotient is divided into the fill-image width and
then reciprocated (`0x10008D7A0..0x10008D884`). Consequently the final fill UV
is proportional to `world_position / textureScale`, not
`sprite_local_position * textureScale`. Hopper's pseudocode independently
shows the same `/ var_138` and `/ var_134` scale arguments followed by the
reciprocal UV construction.

The old wgpu reconstruction had both observable errors: it multiplied by
`textureScale`, and every mask restarted at local `(0,0)`. Adjacent static
round/box masks therefore sampled the same tiny source patch instead of one
continuous rock surface. `RenderState` now carries a separate camera-free
`masked_texture_matrix` (world-pixel translation plus Purple's
`Scale * Rotation` linear basis). GPU vertices and the
software reference renderer use that matrix and divide by the signed texture
scale before normalizing by fill-image dimensions. The projected mask still
uses the camera translation and zoom, so panning cannot make the texture swim.
The textured branch also uses Purple's caller-composed `+0xAC + +0xB0` angle
and raw visual scales instead of accidentally entering ordinary-sprite scale
rules.

A second IDA pass over `sub_10008D428`'s vertex construction resolves the
non-uniformly-scaled case. The live GL matrix at offsets `+0x10..+0x1C` first
rotates `(vertex - pivot)`; its X result is then multiplied by the X argument
and its Y result by the Y argument at `0x10008D7A0..0x10008D8C4`. This is
`Scale * Rotation`, not `Rotation * Scale`. The atlas branch in
`sub_10004BAB4` installs `atlasPivot + RenderObjectData+0xB4/+0xB8`, so the
extra object pivot offsets also shift the fill phase. The host now preserves
both details. IDA and Hopper additionally show that `sub_100043990`
(`drawSelectedTexturizedObject`) calls the same `sub_10008D428` member after
replacing only translation/scale and preserving the current angle/pivot; that
selected/immediate path now publishes the same camera-free fill matrix rather
than silently reverting to local `(0,0)` sampling.

Regressions cover the exact non-uniform `Scale * Rotation` basis, additional
pivot phase, camera-independent phase, generated wgpu source coordinates,
signed scale uniform, selected-object state and the real Chapter02_L01 terrain
set. Deterministic wgpu screenshots of Chapter02_L01, L02, L11 and L23 complete
with empty stderr logs and no invoked fallback or compatibility binding.

## Native frame-load optimizations without adaptive degradation

The performance follow-up keeps the earlier negative result intact: Purple
does not dynamically lower frame rate, resolution, particle count or Box2D
iterations when a frame runs long. The useful positive controls are instead
the fixed 30 Hz physics accumulator/two-slot interpolation, the 100 ms host
delta clamp, retained render resources, clip rejection and GL state caching.
The rehost therefore removes host overhead without adding a quality mode that
does not exist in the executable.

Four previously faithful-looking paths still performed work Purple does not.
Theme drawing cloned the complete retained `ThemeLayer` vector on both passes,
including animation strings and timelines; it now borrows the vector and
copies only the scalar draw snapshot for one layer. Render preparation rebuilt
and sorted a combined sprite/text/geometry/capture list even though the four
queues already carry one monotonically allocated immediate order; it now uses
a stable four-way linear merge with the same synthetic equal-order tie rule.
Body export and motion aggregation likewise shared one ordered scene-map walk
in `sub_10005E898`, but Rust scanned the map twice; the Lua snapshot visitor
now runs before the cached previous-awake byte is updated in that same pass.

The GPU path now interns each base/fill texture pair once per prepared frame,
resolves one bind group per unique pair and calls `set_bind_group` only when
the pair changes, matching the purpose of `GL_State::begin` at `0x10059ACE8`.
The texturized/masked quad path also restores `sub_10008D428`'s exact clip-space
rejection: maximum X/Y are inclusive at -1, while minimum X/Y are strict at 1.
Ordinary atlas sprites remain unaffected because their native draw member has
no corresponding host epsilon or blanket viewport cull.

An isolated 1,200-frame Chapter01_L50 release route moved from approximately
1.97 to 1.71 seconds wall time on the same host. User CPU rounded to 0.86
seconds in both short samples, so the wall-time difference is recorded only as
directional evidence rather than a stable percentage claim. More importantly,
the complete workspace passes 646 tests with one intentional long-idle test
ignored (84 app/audio/wgpu, 31 assets, one core and 530 passing plus one ignored
script/physics test); strict all-target/all-feature Clippy, formatting, diff
whitespace checks and the locked release build are clean.

## Retained localization lookup and frame-profile follow-up

A symbolized release profile of a 100,000-update Chapter01_L50 route exposed
an avoidable host cost which the earlier command/GPU pass did not cover.
`resolve_localized_string` cloned the complete retained `LocalizationTable`
before every `res.getString` call, even when the requested current-language
group was already loaded. Menu/HUD text consequently spent a large fraction
of its draw time allocating, copying and destroying the complete source TEXT
table.

IDA's `sub_10045C380` instead searches the ResourceManager TextGroupSet tree
at `+0x498`, takes the retained set pointer from the matching node and calls
`sub_1004743D4` with the current locale stored at `+0x488`. The latter searches
the already-loaded TextGroup tree and returns its retained pointer directly.
Only a miss scans the source locale vector to distinguish “not present in data
file” from “which is not loaded”. Hopper independently shows the same two
tree searches and the same exceptional vector scan; neither procedure copies
the TextGroupSet or its source string arrays on a successful lookup.

Rust now performs the existence probe and loaded-language lookup without
cloning the source table. The ResourceRuntime and LocaleRuntime mutexes remain
non-overlapping, preserving the existing lifecycle lock order; the source
table is borrowed only after a loaded-group miss to select the native error.
The public `getString`, 2D/3D ResourceManager text and ordinary UI text all use
this shared path, so the optimization covers the complete original call set
without adding a cache or changing invalidation semantics.

On the same host, isolated fresh-AppData 10,000-frame L50 runs moved from
5.27 seconds real / 5.15 seconds user to 4.15 seconds real / 4.05 seconds user.
The resulting map checkpoints differ in the position of a continuously
animated locked-level marker, so their hashes are not used as equivalence
evidence. The exact absent/present/unloaded/loaded/`ALL`/release lifecycle
regression remains the semantic comparison and passes unchanged.

The adjacent particle hypothesis was also checked and rejected rather than
turned into an adaptive quality shortcut. Ordinary particle draw
`sub_100091D90` and ThemeParticleSystem draw `sub_100096E9C` traverse and
submit every matching packed particle; neither contains a viewport test.
The rehost therefore retains full particle submission instead of introducing
screen culling that Purple did not perform.

The complete workspace still passes 646 tests with one intentional long-idle
test ignored. Formatting, diff whitespace, strict all-target/all-feature
Clippy and the locked workspace release build are clean. A final isolated
1,200-frame Chapter01_L50 wgpu checkpoint reaches the requested live level
with an empty stderr log, zero invoked fallbacks and zero remaining
compatibility bindings. Its PNG SHA-256 is
`d54c892ff805536597b89a1269282c0a8e2e808df6a96c3afa54b1785c3c51cd`;
the local release `stella-app` SHA-256 is
`798cce0ce8059d22c9615c09549472c6cf1399671f72ec6768a7ade041b24896`.

## Retained composite bounds and native scene ownership optimization

The symbolized follow-up profile still showed host copies around two native
pointer-owning paths. IDA decompiles `CompoSprite::updateBounds` at
`sub_100436D40` as a direct walk from the retained Entry pointer vector at
`+0x18..+0x20`. Each visible Entry temporarily retains its AtlasSprite,
transforms the four corners and releases that same pointer before advancing;
there is no copied Entry array, atlas-region array or string graph. Hopper's
pseudocode independently shows the same pointer-vector walk, reference-count
pair and four `FCVTZS` corner conversions.

`active_native_sprite_metrics` previously called `active_bound_composite`,
which deep-cloned every `CompositePart`, sprite name, texture source and
`SpriteRegion` before performing that calculation. It now zips the two
retained resource slices by reference and feeds the unchanged float32
`FMUL`/`FMADD`/`FADD` and signed conversion implementation. Focused tests
compare the borrowed and old owned representations for a transformed,
flipped, hidden-part mixture and reject mismatched retained arrays.

The scene dispatcher had a related extra copy. `sub_10004BAB4` retains the
live RenderObjectData pointer through pre/post callbacks, installs its scalar
GL state, and reloads alpha, sprite/composite, transform and decoration after
the pre callback. Rust correctly performed the reload but first deep-cloned
the complete render snapshot solely to install callback state. The callback
phase now calculates the native atlas/composite pivot while holding the scene
lock and copies only scale, angle, offsets, alpha, flip, sensor mode and the
two pivot scalars. The post-callback submission still reacquires the complete
live object, so a shipped pre-draw callback can replace its sprite, scale,
shader or decoration in the same frame exactly as before.

Finally, the compatibility lifetime bridge rebuilt and sorted a BTreeSet from
every table-valued `objects.world` entry before every draw. Purple has no such
mirror pass: `sub_10004BAB4` walks GameLua's retained three-level scene tree,
and native removeObject edits that ownership graph directly. The Rust-only
reconciliation is needed for shipped Lua that assigns
`objects.world.name = nil`, but it now probes only names that own native scene
or track records with raw Lua lookup and allocates a name only for a real
miss. World-table replacement, contact/sensor expiry, joint/track cleanup and
pre/post callback retirement remain at the same lifecycle boundary.

Two equal-duration 10 ms symbolized L50 profiles show the collapsed
top-of-stack `sync_scene_lifetime` count falling from 31 to 13; the complete
Lua-world iterator, string clone and BTreeSet sort frames disappear. The
remaining `SceneDrawObject::from` represents the one necessary deferred-wgpu
submission snapshot rather than the removed callback-only duplicate. On the
same host, isolated fresh-AppData 10,000-frame runs report user CPU moving
from 3.96 to 3.66 seconds (about 7.6 percent); wall time is intentionally not
used because the paired wgpu runs varied in the opposite direction with GPU
and scheduler noise.

The complete workspace passes 648 tests with one intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target/
all-feature Clippy and the locked workspace release build are clean. A final
isolated 1,200-frame Chapter01_L50 wgpu upload/render/readback asserts the
loaded level and finishes with 75 optional nil probes, zero invoked fallbacks
and zero remaining compatibility bindings. Its execution-evidence PNG
SHA-256 is
`eb56a30c917392fe36ef695e1b77f0f8d6e5f0fca33987410e105e8eeeb14cca`;
the release `stella-app` and `stella-headless` hashes are respectively
`073134b92fec0b5137b50270040f48daed1b0f121e2c36f1faa6d309109fa517`
and `dec06dc3331a49c92d19ff558b647ce8c62708223ae52bbb5c1f10f546604cef`.

## Island-scoped fixture-proxy synchronization

The next L50 host profile placed the largest remaining Rust-only physics cost
under `synchronize_native_body_proxies`. The implementation recomputed AABBs
for every active scene object after every discrete island pass, repeated that
full-world pass before and after each TOI candidate, and addressed retained
proxy data through `(String, fixture index)` BTreeMap keys. None of those three
ownership or traversal choices exists in Purple's Box2D path.

IDA decompiles `b2World::Solve` at `sub_10086E634`. After all islands have
finished, the loop at `0x10086E9BC` walks the native world body list. It tests
island-flag bit zero at `0x10086E9C4..0x10086E9C8`, rejects body type zero at
`0x10086E9CC..0x10086E9D0`, and only then calls fixture synchronization
`sub_10086B3BC` at `0x10086E9D8`. Static endpoints had their island flag
cleared after each solved island, so the effective set is precisely the
non-static bodies visited by this step. One broad-phase `FindNewContacts`
follows the body-list walk at `0x10086E9F0`.

`b2World::SolveTOI` at `sub_10086EA54` is even narrower. Its post-island loop
at `0x10086F308` clears the island flag, compares the body type with dynamic
type two at `0x10086F31C..0x10086F324`, synchronizes only those dynamic entries
at `0x10086F32C`, then calls `FindNewContacts` once at `0x10086F364`. There is
no full-world synchronization before each TOI candidate scan. Hopper's ARM64
assembly independently exposes the same `TBZ`/nonzero-type gates in Solve and
the same type-two comparison, fixture call and final broad-phase call in
SolveTOI.

Finally, IDA and Hopper agree on the storage boundary in `b2Fixture::Synchronize`
at `sub_10086CB74`: the fixture walks retained 32-byte proxy records, computes
the old/new transform bounds, writes their swept union into that record,
computes body displacement, and calls broad-phase `MoveProxy`
`sub_10085E3E8` at `0x10086CC58`. It does not allocate an object-name key or a
body-wide cloned AABB list.

The rehost now retains those boundaries. Discrete solving sorts only the
non-static island members into native world-list order before one move-buffer
drain. TOI synchronizes only the dynamic bodies retained by the selected TOI
island and performs its contact discovery at that same boundary; the two
former full-world passes are gone. Per-fixture tight/fat AABBs and the prior
body position are co-located in one retained body proxy record, replacing
three string-addressed maps. The hot synchronizer borrows proxy ids, computes
one fixture AABB directly from its retained local vertices, and updates the
aligned record without cloning the id vector, polygon graph or object name.
Fixture creation, destruction, active-state changes and Dirt rebuilds update
the same aligned record.

Focused regressions prove that a discrete step updates an awake island body
while leaving an unvisited sleeping body and static body untouched, and that
SolveTOI updates its selected dynamic proxy while leaving an unrelated active
body unchanged. Existing broad-phase allocation/reuse, contact timing,
continuous collision, Dirt fixture rebuild and Chapter02 trap-sucker tests
remain green.

Three fresh isolated-AppData 10,000-frame Chapter01 L50 runs report user CPU
of 3.47, 3.45 and 3.49 seconds (median 3.47), compared with the preceding
3.66-second retained-scene build, about a 5.2 percent reduction. The complete
workspace passes 649 tests with one intentional long-duration BirdRun audit
ignored. Formatting, diff whitespace, strict all-target/all-feature Clippy
and the locked release build are clean. A final 1,200-frame L50 wgpu
upload/render/readback reaches `Chapter01_L50.lua` with 75 optional nil probes,
zero invoked fallbacks, zero remaining compatibility bindings and empty
stderr. Its execution-evidence PNG SHA-256 is
`2ce4ffbdcb9b161ed1279fa28e37a7a4a7638009e725c508439923694f5cc47b`;
the release `stella-app` and `stella-headless` hashes are respectively
`05b37124af9b4c030acdb3914f3138d4cb88d552a01b7b502d1615de2e5e301a`
and `cd39b2e17b06c74f2e06343d1e50809f3eb6281cd364d4f9d12c665045804b64`.

## RenderObjectData Lua-reference ownership and scene-walk optimization

The next symbolized 100,000-frame Chapter01 L50 profile no longer placed
fixture synchronization on the Rust hot path. Its largest rehost-only draw
cost was instead the per-leaf reconstruction of
`game_environment -> retained objects -> world -> world[name]` before every
native scene submission. Besides repeated Lua table hashing, that model could
silently change which table a callback received if script code replaced a
same-named `objects.world` entry after construction.

IDA exposes the actual ownership boundary in the non-physics constructor
`sub_100036D38`. It allocates the 0x1A8-byte RenderObjectData at
`0x100036D7C`, initializes the embedded Lua reference at `+0x20` through
`sub_100529BDC` at `0x100036D90`, publishes the fresh table with
`sub_10007F33C` at `0x100036FBC`, immediately retrieves `world[name]` through
`sub_100009944` at `0x10003703C`, and assigns that exact value into `+0x20`
with `sub_100529F68` at `0x10003704C`. The polygon constructor
`sub_1000357A4` has the same fresh-table, publish, retrieve and retained-field
sequence.

The draw dispatcher `sub_10004BAB4` then reads pre/post callbacks from
RenderObjectData `+0x158/+0x160`, but both calls to `sub_100528834` at
`0x10004BFC0` and `0x10004C31C` push the Lua value retained at object `+0x20`.
`sub_100528834` itself checks the embedded registry index and calls
`sub_100509CAC` for a valid reference, otherwise pushing nil through
`sub_100509624`. Hopper independently shows the same object record, callback
slots `r21[0x2b]/r21[0x2c]`, retained-value push and boolean callback argument
on both sides of the object draw. Neither disassembler shows a name-based
`objects.world` lookup in the scene loop.

The Rust constructors now retain the freshly published `mlua::Table` beside
their pre/post function ownership and pass that exact handle to callbacks and
live shader resolution. Native removal, world-owner replacement, failed or
successful level load and ordinary scene reconciliation retire the handle at
the same boundary as the RenderObjectData record. A fallback first lookup is
kept only for tests or extensions that manufacture a scene record without
calling a recovered constructor. A focused regression replaces
`objects.world[name]` after construction and proves the native callback still
receives the original table and its original fields.

The follow-up profile also exposed a smaller host-only cost: the diagnostic
`STELLA_TRACE_DRAW_CALLBACKS` environment variable was queried once per scene
leaf. Purple has no such call in `sub_10004BAB4`; diagnostic flags are now
captured once when the Lua member is installed. In the final five-second
symbol profile, `object_world`/`game_environment` has no descendant under the
scene dispatcher and the per-object `getenv` branch disappears. Collapsed
`luaH_get` top samples move directionally from 52 in the preceding profile to
11; the remaining Lua hashes belong to script execution and the once-per-frame
lifetime bridge rather than callback-object resolution.

Using identically configured optimized-plus-debug-info binaries, three
isolated-AppData 10,000-frame L50 scene routes report old user CPU of 10.47,
10.46 and 10.50 seconds (median 10.47), versus 9.85, 9.78 and 9.78 seconds
(median 9.78) after retaining the constructor table, about a 6.6 percent
reduction. The complete workspace passes 651 tests with one intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace, strict
all-target Clippy and the locked workspace release build are clean. A final
1,200-frame direct-L50 wgpu upload/render/readback reaches and renders
`Chapter01_L50.lua` with 42 optional nil probes, zero invoked fallbacks and
zero remaining compatibility bindings. Its execution-evidence PNG SHA-256 is
`f083709f8340c87d79df795440c33b892cc43f95e2022e4e98a18fbe6183dd08`;
the release `stella-app` and `stella-headless` hashes are respectively
`9be16def71a65b86c7da9420fb8883bca3ea701d14dd26f3dd61e3cb8a077fa7`
and `c12d3338696244af0755aee12c59fb574257256e0120be11792be26289c36758`.

## Intrusive Box2D world/edge order and island scratch optimization

The next symbolized L50 profile moved the remaining host-only physics cost
into `RenderBridge::assemble_box2d_islands`. Rust gathered body, contact and
joint names from lexically ordered maps, cloned them into temporary vectors
and recovered native creation order with three `sort_unstable_by` calls.
That happened once per fixed physics step even though Purple never loses the
corresponding native order.

IDA's `b2World::Solve` at `sub_10086E634` establishes the complete traversal.
The flag-clear loop starts from the world body-list head at `0x10086E6F0` and
follows `b2Body+0x68`. The island seed loop reloads that same head at
`0x10086E768`, rejects static, sleeping and inactive seeds at
`0x10086E7B4..0x10086E7C8`, and advances through `+0x68` at `0x10086E99C`.
Its temporary body stack is a real LIFO array: `0x10086E7F4` decrements the
tail index before reading the next body. Contact expansion starts at
`b2Body+0x90` and follows each `b2ContactEdge+0x18` at
`0x10086E82C..0x10086E8A0`; joint expansion starts at `b2Body+0x80` and
follows `b2JointEdge+0x18` at `0x10086E8A8..0x10086E8F8`. Newly reached
non-static bodies are appended to the same stack. After every island has
been solved, `0x10086E9BC..0x10086E9DC` walks the original body list once
more and synchronizes only non-static entries whose island flag remains set.
Hopper independently exposes the same head load, `LDR [body,#0x68]` body
advance, both edge `LDR [edge,#0x18]` advances and the final body-list pass.
There is no name collection or comparison sort at any of those boundaries.

The rehost now retains inverse native-order indexes beside its lookup maps:
body, contact and physical-joint construction link their creation sequence
to the native record name/key, while replacement, contact expiry, fixture or
body destruction, joint destruction and level teardown unlink the matching
entry immediately. Reverse index iteration is therefore the native
head-to-tail order used by `Collide`, discrete island construction, TOI's
auxiliary contact-edge expansion and destruction callbacks. Island contact
and joint flags are compact boolean arrays indexed by those ordered edge
snapshots instead of string-valued `BTreeSet`s. Metadata-only destruction
links remain outside the physical joint index.

`SolverIsland` and the post-solve synchronized-body list are also Step-local
scratch now. The host moves them into the solve rather than deep-cloning all
body/contact/joint strings, clears them after fixture synchronization and
returns their outer capacity to the next fixed step. Focused regressions
cover live-body indexing, broad-phase contact index retention, newest-first
physical-joint order, delayed reverse joint destruction, shared-static-body
island termination and Dirt's fixture/contact destruction order.

In the preceding five-second symbol sample, the two remaining contact/joint
sort instantiations contributed about 43 collapsed top-of-stack samples. In
the new five-second 100,000-frame sample, neither
`assemble_box2d_islands` nor any of its former `sort_unstable_by`
instantiations appears. Three paired copied-AppData 10,000-frame L50 runs of
identically symbolized builds report old user CPU of 3.39, 3.40 and 3.37
seconds (median 3.39), versus 3.34, 3.33 and 3.30 seconds (median 3.33), a
small directional reduction of about 1.8 percent. This is recorded as a
host-overhead result, not as an adaptive-quality or universal frame-rate
claim.

The complete workspace passes 652 tests with one intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, and the locked workspace release build are clean. A
final 1,200-frame wgpu checkpoint constructs and renders Chapter01 L50 on
the final frame with 80 optional nil probes, zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`8a909484905fcfb2ac8940d0fe6265a0f3dfd27f2bd0a2d4fe8d43c6745cbcf6`;
the release `stella-app` and `stella-headless` hashes are respectively
`06f6501173e73530da3b4bd56ccea3075120557538a4c75ed73311e4a351968c`
and `55771ce571a06afd61711dcb6590940f7c7075dad4235bf00833896b26a2941d`.

## Retained scene resources and removal of the host-only Lua lifetime scan

The next optimized L50 profile placed the remaining scene-submission cost in
two Rust ownership adaptations. `SceneObject -> SceneDrawObject` deep-cloned
the complete `SpriteCatalogRegion` or every `BoundCompositePart`, and
`SceneDrawObject -> RenderCommand` cloned the same resource graph a second
time. Separately, every host draw and every post-`removeBlocks` fixed step
called `sync_scene_lifetime`, probing `objects.world[name]` for every retained
native scene record and track.

IDA's `sub_10006D5B4` shows a different resource boundary. The ordinary path
passes the AtlasSprite pointer at RenderObjectData `+0x90` directly to
`sub_10006C838`. The composite path tests byte `+0x138`, reads its retained
CompoSprite owner at `+0x78`, obtains one part with `sub_100437C0C`, retains
that part wrapper with `sub_1005821A4`, copies one small string/scalar draw
record, submits it, and releases the wrapper. It never copies the complete
atlas/composite graph before and after the scene snapshot. Hopper independently
shows the same `LDR [object,#0x90]` ordinary pointer and the same `+0x78`
per-part retain/submit/release loop.

The rehost now stores the scene object's bound atlas and composite resources
as shared retained pointers. `SceneDrawObject::from` copies only the deferred
draw state and increments those pointers. The later retained-command work
described below extends this ownership through final wgpu consumption instead
of materializing the complete composite graph. Construction and
`native_setSprite` establish the retained pointer at the same assignment
boundary, including the existing empty-composite null sentinel.

The lifetime scan was also non-native. IDA decompiles `sub_10004BAB4` as a
direct walk of GameLua's z/SpriteSheet/name tree at `+0x310`; each name is
resolved in the native RenderObjectData map and the object-held Lua reference
at `+0x20` is pushed for callbacks. There is no `objects.world` traversal or
missing-name erase. Hopper's assembly exposes the same nested tree walk and
retained-reference callback calls. The shipped `game.lua` `removeBlocks`
prototype calls the registered `removeObject` member, whose recovered
`sub_100042260` path owns object and joint destruction; LevelLoad independently
clears the full native scene in `sub_100065D3C`. Those are the real native
lifetime boundaries.

The per-draw and post-`removeBlocks` scans have therefore been removed.
Constructor commit still detects an explicitly replaced world-table owner
before publishing a new native record, and LevelLoad and `removeObject` retain
their synchronous cleanup. Directly assigning `objects.world[name] = nil`
without calling `removeObject` now matches Purple: it removes only the Lua
mirror and does not silently destroy the RenderObjectData still owned by the
GameScene. The corresponding regression now asserts that native ownership
instead of the former compatibility behavior.

Three paired 10,000-frame Chapter01 L50 runs of the preceding island build
report user CPU of 3.39, 3.41 and 3.39 seconds (median 3.39), versus 3.18,
3.20 and 3.21 seconds (median 3.20) after retaining the resource pointers,
about a 5.6 percent reduction. A second paired set compares that retained
resource build with and without the lifetime scan: 3.18, 3.18 and 3.19
seconds (median 3.18) versus 2.62, 2.61 and 2.61 seconds (median 2.61), about
a further 17.9 percent reduction. The final five-second symbol profile has no
`sync_scene_lifetime`, island assembly sort or former resource-graph clone
descendant. `SceneDrawObject::from` remains visible for the required deferred
wgpu scalar/string snapshot, but its atlas/composite ownership is pointer-only.

The complete workspace passes 653 tests with one intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, doc tests and the locked workspace release build are
clean. A final isolated 1,200-frame wgpu checkpoint constructs and renders
Chapter01 L50 on the final frame with 80 optional nil probes, zero invoked
fallbacks, zero remaining compatibility bindings and empty stderr. Its PNG
SHA-256 is
`54c81ff87e6a77c33e29ce3723f05cea6b7f6e26e95914c50ed86e715ab1d59d`;
the release `stella-app` and `stella-headless` hashes are respectively
`e7cee8652487fd00650808261243d809d247c0b12f313d4d74206323b5a83c57`
and `ac53c1f6ec7e7d4a3e19d33969544a625c7d2e6b334d00d60373138365096b37`.

## COW scene-name traversal and retained visual payloads

The next symbolized L50 sample still placed avoidable host work in
`NativeSceneWalk::next`, `SceneDrawObject::from` and `memmove`. The remaining
adapter mismatch was broader than atlas ownership: every z-tree leaf name was
copied into an owned Rust `String`, and every deferred object snapshot copied
its sprite and texture names, mask binding, decoration, ray polygon, Dirt
component and Dirt-hole vector before wgpu command materialization.

IDA's `sub_10004BAB4` establishes the original ownership and mutation boundary.
At `0x10004BD1C` it forms the leaf address as `base + index * 8` and passes that
old-libstdc++ `std::string` directly to the RenderObjectData lookup at
`0x10004BD24`; the eight-byte record and `_Rep::_M_destroy` calls identify the
old COW string ABI rather than an inline owned character vector. The ordinary
sprite at RenderObjectData `+0x90`, composite owner at `+0x78`, decoration at
`+0xF8`, ray/custom geometry at `+0x190`, and Dirt/masked-texture members are
likewise consumed through retained pointers. Hopper independently exposes the
same leaf-address calculation and object-pointer loads.

The traversal is deliberately not frozen across callbacks. After one leaf is
drawn, `0x10004C33C` increments the index, `0x10004C340` reloads the current
vector start/end, and `0x10004C344..0x10004C34C` recomputes its length before
the next iteration. The rehost therefore acquires the render lock once per
iterator step, releases it before entering Lua, and rereads the live bucket on
the next step. Callback-driven z-order changes and erasure retain their native
visibility; this optimization does not snapshot or skip scene-tree mutation.

Scene z leaves and visual names now use `Arc<str>`, matching the recovered COW
sharing behavior. Deferred sprite, texture binding, decoration, ray, Dirt and
Dirt-hole payloads use retained `Arc` handles as the corresponding native
pointers do. `SceneDrawObject::from` copies scalar state and increments these
handles; only the final deferred `RenderCommand` materializes the owned values
required after the scene lock is released. Dirt rebuilds use `Arc::make_mut`,
so a cut preserves existing deferred snapshots while mutating the current
native component. Focused regressions verify name and sprite pointer identity,
resource rebinding, ray and decoration submission, Dirt cutting, and the
existing callback-driven z-order change.

Three paired, copied-AppData 10,000-frame Chapter01 L50 runs compare the prior
retained-resource build at 2.61, 2.58 and 2.61 seconds of user CPU (median
2.61) with 2.43, 2.41 and 2.42 seconds after this change (median 2.42), about a
7.3 percent reduction. In equal five-second symbol samples,
`SceneDrawObject::from` falls from 48 to 24 samples, `NativeSceneWalk::next`
from 58 to 51, and `_platform_memmove` from 397 to 320. The live callback walk
remains present, while the owned-payload copying is reduced.

The complete workspace passes 653 tests with one intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, doc tests and the locked workspace release build are
clean. A final isolated 1,200-frame wgpu checkpoint constructs and renders
Chapter01 L50 on the final frame with 80 optional nil probes, zero invoked
fallbacks, zero remaining compatibility bindings and empty stderr. Its PNG
SHA-256 is
`ac751276e99d7647945c79b83549098e9845c9a4e43a9a3f7f3fe5f92d795e6e`;
the release `stella-app` and `stella-headless` hashes are respectively
`6e5e184786388c3b154e1f2d7d56c42e78950bc53fae423c2776c7b3aa0bee7c`
and `d402a4c6fc0f64c453b868950d40b4785ee731c23697ac5469d1ed69f5323e99`.

## Object-local draw callbacks and recycled deferred frame queues

The next release profile showed two remaining adapter-only costs around the
scene dispatcher. The rehost stored each retained Lua object table, pre-draw
function and post-draw function in three independent name trees, so one scene
leaf performed three callback-tree lookups in addition to separate
RenderObjectData lookups for callback state and drawing. The desktop host also
used `mem::take` to replace all four deferred wgpu command vectors with fresh
empty vectors every displayed frame, forcing the runtime side to regrow its
queues even though Purple renders immediately.

IDA's `sub_10004E3C0` and `sub_10004E570` first call `sub_10005DAF8` once and
then write the retained pre/post function directly into the returned
RenderObjectData at `+0x158` or `+0x160`. Hopper independently shows the same
single lookup followed by `LDR/STR [record,#0x158]` and
`LDR/STR [record,#0x160]`. `sub_10005DAF8` is the RenderObjectData red-black
tree lookup; its missing branch formats `Missing object: %s` at
`0x10005DC1C..0x10005DD98` and throws. A callback registration for a nonexistent
name is therefore an error, not a detached entry waiting for a later object.

The rehost now retains one `DrawCallbackRecord` per native object, containing
the exact constructor Lua table and its two optional functions. Construction,
replacement, removal, world-owner replacement and LevelLoad mutate this one
record at the same RenderObjectData lifetime boundaries. Registering either
callback resolves that record once and reports the recovered missing-object
error. Drawing also resolves the record once. The first scene lookup returns
both callback state and a compact draw snapshot; when no pre callback exists,
both are consumed from that same record lookup. When a pre callback does run,
the dispatcher deliberately reloads visual fields afterward so its same-frame
alpha, scale, sprite and z-order mutations remain visible.

IDA's ordinary and composite draw paths at `0x10006D870` and `0x10006D8F4`
call `sub_10006C838` immediately. The composite path retains one part wrapper
at `0x10006D7C8`, submits it, and releases it at `0x10006D798`; it does not
allocate a frame command vector. Hopper shows the same two direct branches and
retain/submit/release loop. The wgpu host still requires deferred ownership,
but it now exchanges its completed render, text, rectangle and capture vectors
with the runtime in one lock acquisition. The preceding host buffers return to
`RenderBridge`; the next native draw startup clears their old elements and
reuses their allocations. Deterministic screenshot runs leave queues in place
between unconsumed frames and exchange them only for an actual capture or the
final frame, preserving their existing single-buffer fast path.

A focused regression verifies that all four buffers exchange together, retain
native cross-class order, reuse supplied capacities on the next frame and drop
the old completed frame only at the following draw boundary. Scene regressions
cover missing-object errors, callback clearing and removal, retained constructor
tables, pre-draw visual mutation, post-draw flip state, live z moves and level
teardown.

Three paired runs boot for 1,200 frames and then execute 10,000 Chapter01 L50
frames. The preceding build reports 6.72, 6.71 and 6.75 seconds of user CPU
(median 6.72), versus 6.40, 6.35 and 6.39 seconds after the object-local lookup
and host-buffer changes (median 6.39), a reduction of about 4.9 percent. This is
a host-overhead result; update, native scene traversal, final wgpu submission
and all callback mutation semantics remain enabled.

The complete workspace passes 654 tests with one intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, doc tests and the locked workspace release build are clean.
A final isolated 1,200-frame wgpu checkpoint constructs and renders Chapter01
L50 on the final frame with 80 optional nil probes, zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`f8b00949e370a80ce0b41dd290c1d460e3f23e813a9cab30947dc1e4be3dee0c`;
the release `stella-app` and `stella-headless` hashes are respectively
`93fe71ffedafdcaf7deabde4277f9525379bec9c4d5e60db0982f58ea73a7f9f`
and `ee5ab5a43eb596c2cc5dda5a80baded323dfd9cb29b64c36aae2a23c168c9015`.

## Fixture-local TOI transforms instead of complete scene-object clones

The next symbolized physics profile exposed a remaining ownership mismatch in
the continuous-collision path. For every inactive broad-phase pair, the Rust
host cloned a complete `SceneObject` to test the start pose, cloned both
endpoints again for the selected TOI interpolation, retained those copies in
the candidate vector, and cloned both endpoints once more while constructing
the reduced TOI island. A `SceneObject` also owns fixture vectors, material
lists, retained sprite/texture/composite handles, decoration, ray and Dirt
state, so a temporary collision transform copied substantially more than the
native solver state.

IDA's `b2World::SolveTOI` at `sub_10086EA54` keeps the opposite boundary. The
candidate contact supplies its two fixture pointers at contact `+0x60/+0x68`;
their stable `b2Body*` owners are read at fixture `+0x10`. The function calls
`sub_1008602D4` at `0x10086ED38` and `0x10086ED48` to construct only two
stack-local `b2DistanceProxy` values, copies the two 36-byte `b2Sweep` records
from body `+0x1c`, then calls the float32 TOI routine `sub_100861B54` at
`0x10086ED94`. After choosing a contact it advances the same two body sweeps
and derives each four-scalar transform in place. It calls Contact::Update,
writes those same body pointers into the island array at
`0x10086F03C/0x10086F050`, and invokes `b2Island::SolveTOI` at `0x10086F2F0`.
There is no RenderObjectData copy, body copy or fixture/resource-graph copy.
Hopper independently shows the two distance-proxy calls, the stack sweep
copies, both stable body-pointer stores and the final reduced-island call.

The rehost now projects one requested fixture through two compact
`NativeToiTransform` values. A sweep centre and angle reconstruct only the
body origin, sine and cosine; circle, polygon and edge geometry dispatches
directly to the existing narrow-phase factories. The fixture-specific member
also avoids the old detour that generated manifolds for every fixture pair and
then discarded all but one. Candidate records retain only contact key, dynamic
body name, impact centre/angle, manifold and listener event. The selected live
body receives that compact pose, while auxiliary contacts and reduced-island
position constraints borrow their live scene endpoints only long enough to
capture the solver record. Lua BeginContact remains outside the scene lock and
the subsequent auxiliary contact-edge walk still rereads live filtering and
body state after each callback.

A focused regression compares the new transform-only manifold against the old
temporary-full-body method field by field, including type, both witnesses and
feature ids. The existing fast-circle/thin-edge, two-edge bounce, simultaneous
corner, callback filter mutation, TOI position solve, fixture scaling and
ordinary narrow-phase suites pass unchanged. The new five-second symbolized
L50 physics sample contains only two residual complete `SceneObject::clone`
stacks in the whole fixed-step path and no source-level clone in either TOI
candidate selection or reduced-island construction; the preceding profile had
50 such samples dominated by those sites. Because the two samples captured
different exact gameplay phases, this is retained as hot-path elimination
evidence rather than converted into a universal frame-rate percentage.

The complete workspace passes 655 tests with one intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, doc tests and the locked workspace release build are clean.
A final isolated 1,200-frame wgpu checkpoint constructs and directly draws
Chapter01 L50 with 47 optional nil probes, zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`f2e5b5cc371fc22323843af684af7a42563c6f75872fb0680f68e65b2136c3a2`;
the release `stella-app` and `stella-headless` hashes are respectively
`6f840be84c482a292741e89b121246590a2606f218deaaa03d065c92095036f0`
and `c8cf85a88a23dbf3aa8f56eb511705d46ac4047eb3e2ec760df7eda2fad725a6`.

## Borrowed Contact::Update endpoints and scalar listener snapshots

After the TOI change, the next symbolized fixed-step sample contained only two
complete `SceneObject::clone` stacks. Both were in the ordinary contact-manager
refresh: every awake contact cloned its two scene endpoints before filtering,
fat-AABB validation, narrow phase and listener-event construction. Besides the
Box2D state used here, each clone copied fixture/material vectors and retained
the complete sprite, texture, composite, decoration, ray and Dirt ownership
graph.

IDA identifies the corresponding native member as `sub_10086373C`
(`b2Contact::Update`). At `0x100863760..0x10086377C` it copies only the old
64-byte manifold from contact `+0x78` to the stack. It loads the two stable
fixture pointers from contact `+0x60/+0x68`, reads the sensor bytes at fixture
`+0x3A`, and follows fixture `+0x10` to the two owning `b2Body` records. The
non-sensor path passes the two body transforms at body `+0x0C` directly to the
contact evaluator. When touching changes, `0x1008638B0..0x1008638E4` wakes
those same bodies in place. It then updates the touching flag in contact `+0x08`
and calls BeginContact, EndContact and PreSolve through listener vtable slots
`+0x10`, `+0x18` and `+0x20`. No body, fixture or RenderObjectData graph is
copied. Hopper independently shows the same manifold stack copy, fixture/body
pointer chain, in-place wake writes and three listener calls.

The rehost now resolves each scene endpoint once and keeps both as immutable
borrows through the awake gate, deferred-filter test, fat-AABB check, narrow
phase and event-state capture. A small `NativeContactUpdate` result crosses the
borrow/mutation boundary. Only the six mass/velocity scalars required by the
game listener are retained; the owned contact-name event is materialized after
the scene borrow ends. Missing, sleeping, retired, separated and touching
branches remain explicit, so sleeping contacts retain their dirty filter flag,
touching changes wake both live bodies, sensor contacts emit only Begin/End,
and solid contacts still yield a record for the later PostSolve impulse. Lua
callbacks remain outside the scene lock and the Collide traversal still
rereads live state before its next contact node.

The contact callback, filter, sensor, broad-phase, destruction and shipped
Luca tutorial suites pass unchanged, including the regression where an earlier
BeginContact mutates the next node in the same native list walk. The complete
workspace passes 655 tests with one intentional long-duration BirdRun audit
ignored. Formatting, diff whitespace, strict all-target/all-feature Clippy,
doc tests and the locked release build are clean. A five-second symbolized L50
physics sample contains ten fixed-step stack records and zero complete
`SceneObject::clone` records, compared with two on the immediately preceding
build.

The final isolated wgpu checkpoint directly constructs and draws Chapter01 L50
with 47 optional nil probes, zero invoked fallbacks and zero remaining
compatibility bindings. Its PNG SHA-256 is
`4f80910e0d4101395687015205710a833f0727a4fca2a61c47a6a179848de363`;
the release `stella-app` and `stella-headless` hashes are respectively
`36a6c262092c9cdfc161ec2b4fa397853379a072c4bc51b7fba6df1c4f9f02ae`
and `6ec10cec7796f165c602b701a5752a37e84b9a30d6bdd106220663ea50a837e0`.

## Borrowed world-island graph and compact joint constraint bodies

The next physics profile exposed two related Rust ownership costs that are not
present in Purple. Before every fixed step, island assembly cloned every
touching `ContactKey`, every complete physical `PhysicsJoint`, both endpoint
names into fresh adjacency maps and every awake seed name. Each joint
constraint pass then cloned the selected `PhysicsJoint` again and cloned both
complete `SceneObject` endpoints. A joint-heavy island repeats the latter work
for initialization, ten velocity passes and up to ten position passes, even
though render resources, fixture vectors, decoration, ray and Dirt payloads
are not solver inputs.

IDA's `b2World::Solve` at `sub_10086E634` establishes the native lifetime. It
clears body/contact/joint island flags at
`0x10086E6F4..0x10086E74C`, allocates only `bodyCount * 8` bytes for its DFS
pointer stack at `0x10086E750..0x10086E760`, and walks the intrusive world body
list through body `+0x68`. Contact edges start at body `+0x90`, joint edges at
body `+0x80`, and both advance through edge `+0x18` at
`0x10086E828..0x10086E8F8`. Selected stable body, contact and joint pointers
are appended to the island scratch arrays and immediately consumed by
`b2Island::Solve` at `0x10086E930`. A static endpoint is appended but traversal
does not continue through it; selected sleeping non-static bodies are woken in
place. Hopper independently shows the same intrusive-list offsets, scratch
allocation, pointer stores and immediate solve call. Neither disassembler
shows a body, joint or endpoint-name copy.

The rehost now builds the contact and joint edge topology from borrowed
world-order records and borrowed `&str` endpoint keys. The DFS follows
`native_body_world_order` directly, uses borrowed stack/visited entries, and
only owns the names that must survive in the per-island scratch record.
Sleeping bodies are woken after the immutable graph borrow ends but before the
first solve; no callback or solver operation occurs at that boundary. The
outer island and synchronized-body vectors retain capacity across fixed steps.
A focused regression covers one awake body connected through two physical
joints to a complete sleeping chain and verifies native body/joint order and
same-step wake propagation; the existing shared-static, contact-chain and
world-creation-order tests cover the remaining DFS boundaries.

Joint solving now mirrors the second native boundary as well. One compact
`JointBodyState` captures only transform, sweep centre, velocity, inverse mass
and inertia, and the four participation flags. The persistent joint map is
temporarily moved out while a constraint pass mutates scene bodies, allowing
the live `PhysicsJoint` itself to accumulate warm-start, motor and limit
impulses without cloning it. Anchor, distance, weld, revolute, prismatic and
rope helpers consume a common body-view interface, so direct solver
regressions and the compact runtime path execute identical float32 geometry.
The map is restored before deferred broken-joint destruction, preserving the
existing native teardown order.

Three alternating 3,000-frame synthetic runs with 800 moving bodies and 799
distance joints compare the preceding build at 11.98, 12.17 and 12.12 seconds
of user CPU (median 12.12) with borrowed graph assembly at 11.76, 11.81 and
11.88 seconds (median 11.81), about a 2.6 percent reduction. Isolating the
compact joint-state change over three alternating 2,000-frame runs reduces
user CPU from 7.99, 8.03 and 8.01 seconds (median 8.01) to 4.73, 4.69 and 4.71
seconds (median 4.71), about 41.2 percent in that deliberately joint-heavy
case. A settled Chapter01 L50 10,000-frame control remains effectively neutral
at medians 2.32 versus 2.34 seconds, so the stress result is not treated as a
universal frame-rate claim.

A final five-second symbol sample contains 2,047 `step_physics` samples and
repeated live `assemble_box2d_islands`/`solve_island_joints` stacks, with zero
`PhysicsJoint::clone` and zero `SceneObject::clone` stacks. The complete
workspace passes 656 tests with one intentional long-duration BirdRun audit
ignored. Formatting, diff whitespace, strict all-target/all-feature Clippy,
doc tests and the locked workspace release build are clean.

The final isolated 1,200-frame wgpu checkpoint directly constructs and draws
Chapter01 L50 with 47 optional nil probes, zero invoked fallbacks and zero
remaining compatibility bindings. Its PNG SHA-256 remains
`4f80910e0d4101395687015205710a833f0727a4fca2a61c47a6a179848de363`;
the release `stella-app` and `stella-headless` hashes are respectively
`740ea04b1496f69d652d0f1a23950e2d4ab54d114a67f3369a9bd112bf4b32c4`
and `2e246d3f92ae752bf82c29e0b43ae632ccbb941a91438fdf123839e848ec0972`.

## One stable b2Joint pointer array across every island pass

The compact joint-body state removed the largest ownership cost, but each
constraint pass still resolved every selected joint name through the
persistent `BTreeMap`. A normal island performs that lookup during constraint
initialization, ten velocity iterations and as many as ten position
iterations. This remaining indirection did not match the recovered native
island representation.

IDA's `b2Island::Solve` at `sub_10086CE84` shows that the island member at
`+0x20` is one stable `b2Joint*` array and its count is at `+0x44`. The
initialization loop at `0x10086D070..0x10086D0A8` loads a pointer from that
array and calls virtual slot `+0x30`. Every velocity iteration at
`0x10086D0D8..0x10086D10C` reloads the pointer from the same array and calls
slot `+0x38`; the position loop at `0x10086D2E4..0x10086D320` does the same
before calling slot `+0x40`. The compact body position and velocity arrays are
the adjacent island members at `+0x30/+0x38`, while the body-pointer array is
at `+0x10`. Hopper independently shows the same array loads, index order and
three virtual-call slots. Neither disassembly resolves a name or reconstructs
the joint list between passes.

The rehost now resolves an island's persistent joints once, moves the actual
records into a capacity-retaining `NativeIslandJointConstraints` owner, and
reuses that ordered array from warm-start initialization through all velocity
and position passes. The records are restored to the persistent map only after
the island finishes. Warm-start, motor, limit and distance impulses therefore
remain on the same record throughout the solve, matching the lifetime of the
native pointers without unsafe aliases. A missing endpoint follows the prior
adapter recovery boundary: its record is restored first and then removed by
the ordinary ordered native-joint destructor.

Focused regressions verify that the persistent map remains empty during all
passes, that both joint records keep their original string storage addresses,
that map order is restored afterward, and that a broken endpoint still retires
the joint through the native order. The complete workspace now passes 658
tests with one intentional long-duration BirdRun audit ignored. Formatting,
diff whitespace, strict all-target/all-feature Clippy, doc tests and the locked
release build are clean.

Three alternating 2,000-frame runs of the same 800-body, 799-distance-joint
stress scene reduce user CPU from 4.68, 4.68 and 4.67 seconds (median 4.68) on
the preceding build to 4.28, 4.26 and 4.28 seconds (median 4.28), about an 8.5
percent reduction in that deliberately joint-heavy case. Three alternating
10,000-frame settled Chapter01 L50 controls remain effectively neutral at
medians 2.33 versus 2.31 seconds. The symbolized five-second stress sample
contains the live fixed-step solver and no `PhysicsJoint::clone`,
`SceneObject::clone` or joint-map lookup stack; the constraint dispatch is
inlined into `step_physics` in the optimized binary.

The final isolated 1,200-frame wgpu checkpoint directly constructs and draws
Chapter01 L50 with 47 optional nil probes, zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`8465f7a6472e89704512df24d980de68466f92558526c65794efb818f2cc751b`;
a decoded comparison with the preceding checkpoint differs only in 50 pixels
inside one 35-by-8 animated-character region, with the scene geometry and
layout unchanged. The release `stella-app` and `stella-headless` hashes are
respectively
`269e5b0f3b9a42fbd7918d4ffe490206974abc5fa0153c6459b2cb923ff1d9e1`
and `d40e08837f36380baf7f6517fbb6d067d8c4c6a5757f04ad46dbe4dfc68a5765`.

## Constructor-owned BitmapFont atlas and shared deferred IFont value

The next symbolized stress sample exposed a non-physics ownership mismatch in
ordinary UI text. `ResourceRuntime::current_text_font_binding` called
`resolve_texture_source` for every submitted bitmap-font string. Of 3,135
samples, 286 were inside that resolver: 221 stopped in `__getattrlist` through
`realpath`, with a further 35 in `stat`, 18 in `statfs`, 23 in `lstat` and 15
in `readlink`. Each deferred command also deep-cloned the complete parsed
BitmapFont and its glyph vector. Purple performs neither operation at draw
time.

IDA's BitmapFont constructor at `sub_10042A5B0` calls its loader
`sub_10042A780` once at `0x10042A678`. The loader joins the constructor path at
`0x10042A8E4..0x10042A924`, resolves the texture through `sub_100478240` at
`0x10042A954`, constructs the retained atlas owner through `sub_10046ABB8` at
`0x10042A960`, and stores that pointer at BitmapFont `+0x50` at
`0x10042A974`. Each FONT glyph is constructed from that owner through
`sub_10046AF70` and its `AtlasSprite*` is inserted into the persistent glyph
tree at `0x10042AA38..0x10042AB40` for v1 or
`0x10042ABA8..0x10042ACAC` for v2.

The draw virtual at `sub_10042B338` follows the opposite boundary. It searches
only the existing glyph tree at object `+0x28/+0x30`; the selected node's
stored sprite pointer is read at `0x10042B620`. Width and pivot virtuals run on
that pointer, and `sub_100467A00` submits it at `0x10042B6B8`. There is no
filename, FilePath, open, stat or texture lookup in the complete 1,060-byte
draw member. Hopper independently shows the same constructor-time texture
owner store, glyph-node pointers and lookup-only draw loop.

The rehost now resolves and canonicalizes a bitmap font's atlas once, during
successful `createBitmapFont`, and retains the result beside the constructed
font until a same-name replacement or `releaseFont`. The runtime IFont map and
every deferred `TextRenderCommand` share one `Arc<BitmapFont>` instead of
copying the glyph table. System-font replacement removes both bitmap owners;
failed and duplicate constructors preserve them. A direct-runtime diagnostic
fallback can still resolve an artificially inserted parsed font, but the
shipped production path is constructor-bound and lookup-only.

The strengthened ownership regression creates a real FONT and texture,
captures its constructor binding, removes that texture and introduces a
higher-level fallback candidate before drawing. Both submitted commands keep
the original path and are `Arc::ptr_eq` to each other and to the constructed
IFont; replacing and releasing the active name then removes only the resource
map owner. Existing same-name replacement, 3D text, UTF-32 glyph, CPU renderer
and wgpu font-binding tests pass unchanged.

Three alternating runs of 5,000 direct bitmap-text submissions reduce real
time from 0.18, 0.16 and 0.21 seconds (median 0.18) to 0.06, 0.05 and 0.06
seconds (median 0.06). Median system time falls from 0.13 to 0.03 seconds and
median user time from 0.04 to 0.02 seconds. This is an intentionally
text-heavy ownership microbenchmark, not a universal frame-rate claim. In a
mixed 2,000-frame physics/UI stress run the median real time changes from 1.25
to 1.17 seconds and median system time from 0.18 to 0.09 seconds.

The follow-up five-second symbol sample contains no `resolve_texture_source`,
`realpath`, `__getattrlist`, `stat`, `lstat`, `readlink` or BitmapFont clone
stack. The complete workspace passes 658 tests with one intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace, strict
all-target/all-feature Clippy, doc tests and the locked workspace release
build are clean.

The final isolated 1,200-frame wgpu checkpoint directly constructs and draws
Chapter01 L50 with 47 optional nil probes, zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`f2e5b5cc371fc22323843af684af7a42563c6f75872fb0680f68e65b2136c3a2`;
a decoded comparison with the preceding checkpoint again differs only in 50
pixels inside one animated-character region. The release `stella-app` and
`stella-headless` hashes are respectively
`828862f62e46c8ed0291ae86d3eaa9349a793cb5a7ee0a2074652127cdc99e0d`
and `7587167fb7fa8fad59e29595d53b1dbc03a67bc4939beaf04db6d23c0e33d1cc`.

## Direct native Sprite entries and cached CompoSprite metrics

The next query profile exposed another ownership mismatch. The rehost's
active-name map already reproduced Purple's last-entry-wins resource stack,
but the selected entry retained only its resource owner. Every generic Sprite
bounds or pivot query therefore searched the owner's immutable SPRT/COMP
vector by name, and a composite query rebuilt all transformed bounds. Purple
keeps the concrete Sprite pointer and reads fields from that object instead.

IDA's four generic query adapters at `sub_10045CD14`, `sub_10045CD60`,
`sub_10045CDAC` and `sub_10045CDF8` call the central active-name lookup
`sub_10045BDDC` once. They branch on the concrete type and immediately call
the selected AtlasSprite or CompoSprite width, height and pivot getters. The
AtlasSprite getters at `sub_100467E14`, `sub_100467E1C`, `sub_100467E24` and
`sub_100467E2C` are direct signed loads from object offsets `+0x2c`, `+0x2e`,
`+0x30` and `+0x32`. They contain no vector scan or geometry reconstruction.
Hopper independently shows the same lookup, type branch and field loads.

CompoSprite uses the same cached-field contract. `sub_100436D40` walks its
retained Entry pointer vector and each Entry's AtlasSprite pointer, transforms
the four corners, applies `FCVTZS`, and stores width, height and pivot at
CompoSprite `+0x60`, `+0x64`, `+0x68` and `+0x6c`. Constructors call that
member once. The dedicated `getCompoSpriteBounds` adapter at `sub_10044913C`
also calls it explicitly before returning `-pivot` and `size - pivot`, whereas
the generic Sprite queries do not. `setCompoSprite` at `sub_100449CFC` writes
ordinary position, scale, flip, angle and visibility fields without a bounds
refresh. Its name-change exception calls `sub_1004375E8`, which replaces the
retained AtlasSprite pointer and then calls `sub_100436D40`. This distinction
is visible in both disassemblers and is important to observable Lua behavior.

The rehost now stores the direct owning-vector index and the four concrete
integer metrics on every active Sprite resource entry. Composite child-region
owners are represented by two index-aligned vectors matching the native
CompoSpriteSet and Entry arrays. Generic queries therefore perform only the
active-name stack lookup and copy the stored fields. The dedicated composite
bounds query refreshes those fields; scalar entry changes leave them stale,
and a changed child name resolves and retains the new AtlasSprite before an
immediate refresh. The existing final-priority lookup and release-to-previous
entry behavior is unchanged. A focused regression covers all three refresh
boundaries, including an instance-suffixed child name.

Three alternating 5,000,000-query runs against the last sprite in a shipped
sheet reduce median real time from 1.54 to 1.18 seconds and median user CPU
from 1.46 to 1.07 seconds, reductions of about 23.4 and 26.7 percent in this
deliberately lookup-heavy microbenchmark. A follow-up symbolized
30,000,000-query sample contains the expected active-name map lookup but no
`active_sprite_entry`, iterator-find, owning SPRT-vector scan or
`SpriteResourceEntry` scan stack.

The complete workspace now passes 659 tests with one intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace, strict
all-target/all-feature Clippy and the locked release build are clean. The
current release also directly constructs Chapter01 L50 and calls
`drawGameNative` with 14 optional nil probes, zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. A separate 1,200-frame
real-wgpu boot/map checkpoint has zero invoked fallbacks, zero compatibility
bindings and empty stderr; its PNG SHA-256 is
`f6acb966956827bc280e4031e6c82d92528a6f633980322c3d80a72644ac2045`.
The release `stella-app` and `stella-headless` hashes are respectively
`2e523c823b15780611aad70efae519c7069a3adb09ddd4823d6225cd5b9b584c`
and `fa533d3841bca561f59518311a1739af0c77be7966ee013357cb70fb7f321faa`.

## Fixed GameApp key-byte buffers and frame-published edge lifetime

The next full update/draw sample found a Rust-only per-frame cost in
`clear_input_edges`: it iterated and copied six Lua tables through mlua's
generic `TablePairs` path after every update. That also gave the retained
native key tables the wrong observable lifetime. Purple does not scan or
clear Lua tables at the end of a frame.

IDA's frame input publisher `sub_1000293C8` at
`0x1000295C8..0x100029658` walks the fixed five-entry key-code array at
`0x1009AE708`. For each entry it reads the platform press byte at
`GameApp + key + 0x590`, the release byte at `GameApp + key + 0x613`, and
the held state through `sub_1004016F4`. It publishes all three values through
the retained GameLua tables at `+0xf0`, `+0x118` and `+0x140`, then the two
`strb wzr` instructions at `0x100029648` and `0x10002964c` consume only the
platform edge bytes. The loop advances by four bytes and stops after exactly
five keys: `LBUTTON`, `KEY_BACK`, `KEY_MENU`, `VOLUME_UP` and `VOLUME_DOWN`.
Hopper independently recovers the same static array, GameLua owner at
`GameApp + 0x578`, table offsets, byte offsets and five-iteration loop.

The later GameLua update call in `sub_10005E898` is at `0x1000605A0`.
Its tail updates particles, destroys pending joints, advances AimStream and
returns at `0x1000606B0`; there is no post-callback Lua key-table scan or
clear. A shipped-script probe also confirms that gamelogic derives its compact
`g_keyPressed`/`g_keyReleased` tables from these retained native tables during
`update`, so the host must not manufacture or clear those script-owned tables.

The rehost now owns three fixed five-element native buffers. Platform events
change the held byte and deduplicate press/release edges without entering Lua.
At frame start one small snapshot is published to the three retained native
tables and only the two edge arrays are consumed. Published Lua booleans stay
visible for the complete frame and are overwritten on the next frame, exactly
matching the recovered native boundary. Application deactivation still clears
the held bytes before the lifecycle callback while preserving pending edges.

Focused regressions cover frame publication, edge lifetime, auto-repeat,
release, activation, view disappearance, retained-table identity and the
shipped compact event-table derivation. The complete workspace passes 660
tests with one intentional long-duration BirdRun audit ignored. Formatting,
diff whitespace, strict all-target/all-feature Clippy, doc tests and the locked
release build are clean.

Three 2,000,000-frame stable-map host-overhead runs reduce median real time
from 19.61 to 14.34 seconds and median user CPU from 18.79 to 13.58 seconds,
reductions of about 26.9 and 27.7 percent in this deliberately minimal
update/native-draw benchmark. A follow-up symbol sample contains no
`clear_input_edges`, `TablePairs` or mlua `GenericShunt` stack. This isolated
result is not treated as a universal gameplay frame-rate claim.

The final release opens the settings panel through a real deterministic wgpu
LBUTTON click after the map has settled, with zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`d40fba5eacb48c5738d2c4926408ce5dc374f3d3d6f3dc673fabcd02f81d8ff1`.
The same release directly constructs and draws Chapter01 L50 with 14 optional
nil probes, zero fallbacks and zero compatibility bindings. The stripped
`stella-app` and `stella-headless` hashes are respectively
`72ce04045bf490ef9fceb0e1f1efd85f415d4cad278d4f21138daa9215513a92`
and `264fb417104690ee42e39497f77d00c697ef8fed6b2510e65c3e2aa7904aea61`.

## Fixed GameLua LuaObject slots and integer registry references

The next real update/draw sample moved a visible part of the remaining Rust
adapter cost into `object_world`. Every object transform setter first resolved
the public `gamelua` global, then performed two named-registry string lookups
to recover one retained native table before doing the actual `world` and
object-name lookups. The latter two lookups are native; the preceding three
were host bookkeeping.

IDA's position member `sub_10003FA60` resolves the RenderObjectData by name,
then passes the fixed GameLua member at `a1 + 0x408` directly to
`sub_10006F8BC` with `"world"` at `0x10003FADC`. It indexes the resulting
table by the supplied object name through `sub_100009944` at `0x10003FAEC`
and writes `x`/`y` before updating the native pose fields. The rotation member
`sub_10003FB78` repeats that exact fixed-member, `world`, name and setter chain
at `0x10003FC04..0x10003FC34`. Scale member `sub_100040304` does the same at
`0x100040340..0x100040388` before resolving RenderObjectData and writing both
live and persistent scale pairs.

The helper `sub_10006F8BC` confirms the ownership distinction: its first
argument is already one `lua::LuaObject`, whose Lua state pointer is read at
object `+0x18`; it pushes that retained reference, indexes `world`, validates
the table and constructs the returned LuaObject. It never resolves `gamelua`
or a textual host registry key. `sub_100009944` then performs the necessary
name-keyed table access. Hopper independently recovers all three setter chains,
the fixed `arg0 + 0x408` owner and the same two Lua table accesses.

The rehost now represents all twelve recovered fixed GameLua LuaObject members
as a fixed Rust slot array. Each slot owns mlua's integer `RegistryKey`, so a
normal read is one integer registry access instead of a bound-flag string
lookup followed by a value string lookup. A present `LUA_REFNIL` key preserves
the native distinction between an explicitly retained nil and a member that
has not yet been initialized. Replacing nil with a table, a table with nil or
one table with another reuses the same logical member. The pre-boot fallback
still captures the public field once for isolated tests, but the ordinary
`object_world` path no longer resolves `gamelua` after the member is bound.

Three focused regressions cover first capture and global shadowing, retained
nil followed by a later public table, and repeated nil/table replacement.
Existing constructor, input, level-load, collision and draw-reference identity
tests pass unchanged. The complete workspace now passes 663 tests with one
intentional long-duration BirdRun audit ignored. Formatting, diff whitespace,
strict all-target/all-feature Clippy, doc tests and the locked release build
are clean.

Three warmed 1,000,000-iteration transform-adapter runs, each issuing
`setScale`, `setPosition` and `setAngle`, reduce median real time from 2.09 to
1.32 seconds and median user CPU from 2.00 to 1.26 seconds, reductions of about
36.8 and 37.0 percent in this deliberately setter-heavy microbenchmark. It is
not treated as a universal gameplay frame-rate claim. In the follow-up real
frame symbol sample, named-registry lookup is absent from the collapsed hot
stack; the retained-object path uses the expected integer `lua_rawgeti`.

The final stripped release opens the settings panel through a deterministic
wgpu LBUTTON click after the map settles, with zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`ed3d6aca3da4e1d0c80bb899e417fd4b1df53e5f7125442e8a4181a57ecdc0f8`.
The same binaries directly construct and draw Chapter01 L50 with 14 optional
nil probes, zero fallbacks, zero compatibility bindings and empty stderr. The
stripped `stella-app` and `stella-headless` hashes are respectively
`34af11ca17294aa92e460a756a93d2b5dea544d411275220d3555dbae55ccd4e`
and `0ccb4b4a0abd67895d0c7a009195428c022ff83b181bfaff848d4bf267a8fd57`.

## Live scene cursor, callback snapshot boundary and in-place trails

The next real update/draw sample concentrated the remaining scene adapter
cost around the Lua pre/post calls, `NativeSceneWalk::next` and the trajectory
pre-pass. The callback bodies themselves are shipped behavior and remain
untouched, but the Rust ownership surrounding them still performed work that
Purple does not: every yielded tree item cloned the render bridge `Arc`, a pre
callback received a complete draw snapshot that was immediately discarded and
rebuilt after Lua returned, and every frame deep-cloned both flight-trail point
vectors before drawing them.

IDA's 0x954-byte scene dispatcher `sub_10004BAB4` resolves each live name entry
through `sub_100070278` at `0x10004BD1C..0x10004BD28`. After one visit it
increments the vector index at `0x10004C33C`, reloads the vector begin/end pair
at `0x10004C340`, recomputes the current length and branches back at
`0x10004C34C`; therefore callback-driven removal, append and z movement remain
observable to the current walk. The pre holder is read from RenderObjectData
`+0x158` at `0x10004BFA4`, retained and invoked at
`0x10004BFAC..0x10004BFDC`. Only after it returns does the dispatcher reload
alpha, sprite/composite, scale, transform and decoration fields beginning at
`0x10004BFE0`. The post holder is not snapshotted beside pre: it is loaded
later from `+0x160` at `0x10004C300`, immediately before its own invocation at
`0x10004C308..0x10004C338`. Hopper independently recovers the same 65-block
function, live vector-length reload and the two distinct callback-holder load
sites.

The trajectory member `sub_10006D9C0` establishes the other ownership
boundary. It reads the retained two-record owner from GameLua `+0x558`, walks
the first point vector through `0x10006D9DC..0x10006DA2C`, then the second at
`0x10006DA64..0x10006DAB4`. Both loops reload begin/end from the original
records; no temporary record or point-vector copy exists. Sprite submission is
direct from those same retained records. This also agrees with Hopper's two
fixed-record assembly loops.

The rehost now retains one `Arc<RenderBridge>` for the complete scene walk and
keeps the mutable native cursor in a separate field, so each yielded z marker
or object borrows the owner instead of performing an atomic retain/release.
The initial scene lookup captures only the scalar callback context. Objects
without pre retain one ordinary draw snapshot, while pre-enabled objects defer
that snapshot until Lua returns. Post is re-read after pre, making a same-visit
post replacement observable as in the executable. Both trajectory buffers are
now borrowed in place while their existing bridge lock excludes mutators, and
the retained Lua object is inspected for its shader without another temporary
value clone.

A new regression replaces an object's post callback from its pre callback and
proves that only the replacement executes during that same visit. Existing
tests continue to cover pre-draw visual mutation, post-draw live flip, z moves,
empty z nodes, retained resource pointers and callback teardown. The complete
workspace passes 664 tests with one intentional long-duration BirdRun audit
ignored. Formatting, diff whitespace, strict all-target/all-feature Clippy,
doc tests and the locked stripped release build are clean.

Three warmed runs boot 1,200 island-map frames and issue 500 additional
`drawGameNative` calls on the settled scene. The preceding build reports real
times 0.96, 0.97 and 0.95 seconds (median 0.96) and user CPU 0.78, 0.79 and
0.78 seconds (median 0.78). The optimized build reports real times 0.87, 0.88
and 0.91 seconds (median 0.88) and user CPU 0.75, 0.75 and 0.78 seconds (median
0.75), reductions of about 8.3 and 3.8 percent in this deliberately
scene-dispatch-heavy mixed benchmark. It is not treated as a universal frame
rate claim.

The final 1,200-frame real-wgpu island checkpoint has zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`cc801333a8236a1c5caf57e8245af01bab69f35b2f7f6ed54e97df55c37b742b`.
A separate direct Chapter01 L50 construction/draw has 14 optional nil probes,
zero fallbacks, zero compatibility bindings and empty stderr. The stripped
`stella-app` and `stella-headless` hashes are respectively
`01d41b11e05099b67bed0b9cd8f9a66ab9b8f96ba8b016c415be39b7db48eb95`
and `dc58a4a8425732194b714d853a1fb234d6515eccf5a4ec4e2190aa538e7239c6`.

## Native global nil lookup and opt-in missing-key diagnostics

The next symbolized update/draw sample identified the rehost's missing-global
reporter as a host-only frame hotspot. Every absent `_G` read entered a Rust
`__index` callback. Although the public set contained only the first copy of
each name, the old callback still converted every interned Lua string into a
Rust string, acquired a mutex and walked the ordered report set. These steps
were useful while discovering the native surface, but they are not part of
Purple's runtime behavior.

IDA's complete `GameLua` aggregate constructor at `sub_10002C274` registers
the recovered native wrapper families directly into the Lua global table. Its
call/import inventory contains neither `lua_setmetatable` nor a constructor
route that installs a global `__index`; the constructor span also contains no
`__index` literal. Hopper independently recovers the same registration call
sequence and no global-metatable setup. An absent global therefore follows
the ordinary Lua 5.1 table lookup and produces nil without a native callback.
This distinguishes the earlier Rust audit hook from an executable feature.

Normal `StellaLua` and desktop construction now leave `_G` without a
metatable, matching that native path. The `--list-missing` command-line mode
selects a separate diagnostic constructor and retains the audit behavior. In
that explicitly instrumented mode, the reporter retains the first interned
Lua string object and deduplicates subsequent probes by its stable pointer,
so long audits do not repeatedly allocate, convert or lock for the same key.
ResourceManager and AnimationWrapper's separately recovered fallback tables
are unchanged.

The focused regressions prove that an ordinary host has no global metatable
and records no absent read, while the diagnostic host records the exact same
read; the existing missing-data/fallback audit continues to separate an
absent value from an invoked compatibility member. The complete workspace
passes 666 tests with one intentional long-duration BirdRun audit ignored.
Formatting, diff whitespace, strict all-target/all-feature Clippy, doc tests
and the locked stripped release build are clean.

In the same fresh-AppData 10,000-frame workload, pointer deduplication first
reduced the three warmed median real/user times from 4.47/4.24 seconds to
4.33/4.12 seconds. Removing the diagnostic callback from normal execution
then reduced them to 3.33/3.12 seconds, a further reduction of approximately
23.1 percent real time and 24.3 percent user CPU. Across both changes the
reductions are about 25.5 and 26.4 percent. This deliberately missing-read-
heavy benchmark is not treated as a universal frame-rate claim. A follow-up
six-second symbol sample contains no global compatibility callback,
`MissingStringCache`, missing-set insertion or missing-key string conversion
stack.

The final stripped 1,200-frame real-wgpu checkpoint keeps the explicit audit
available and reports 74 optional nil probes, zero invoked fallbacks, zero
remaining compatibility bindings and empty stderr. Its PNG SHA-256 is
`d5cccb8525b12447b7176a5c19df6fee8db21a957eae9016761c26278f355ceb`.
The stripped `stella-app` and `stella-headless` hashes are respectively
`af6ba685e30fbf7428f2feff1294ae21180719b3cee697285aca96bc8b82181e`
and `48570a7150ebe755b3086cd28a16b1d8a4bdb48614f7f8adf30aab1e9baa50a3`.

## Rectangular water scene submission and editor placeholder suppression

The Chapter02 water failure was not a missing texture. A direct L01 render
trace showed that the two authored `BLOCK_SENSOR_BOX_WATER` bodies still
submitted their `RED_CROSS` editor sprites, while the retained
`native_setWaterColor` RGBA vector had no rendering consumer in the rehost.
The same omission reproduced the reported L16 pool: its water fixture was
physically present, but the red placeholder occupied the pool instead of the
native translucent fill.

IDA's `sub_10004BAB4` and Hopper's independent procedure view agree on the
special branch at `0x10004BD2C..0x10004BF20`. After the visible byte at
`RenderObjectData+0x14A`, the dispatcher tests the water byte at `+0x14B` and
rejects the circle byte at `+0x147`. It reads the interpolated centre at
`+0xA4/+0xA8`, rectangular dimensions at `+0x9C/+0xA0`, camera origin at
`GameLua+0x514/+0x518`, world scale at `+0x520`, and RGBA at
`+0x540..+0x54C`. Width and height are halved with `0.5f`, converted through
the exact `1.0f / 0.05f` physics-to-world factor, and each screen origin and
negative extent is independently truncated by `FCVTZS`. The color pack is
`A<<24 | R<<16 | G<<8 | B`, with every component independently multiplied by
`255.0f` and truncated. The member resets the complete 0x9c-byte GL state,
draws the rectangle through virtual slot `+0x80`, then checks
`GameLua+0x512`. A clear byte ends the object visit before Lua pre/post
callbacks or sprite submission; only `setEditing(true)` continues into the
ordinary `RED_CROSS` path.

The scene callback snapshot now carries the five scalar water fields and
submits that rectangle at the same point in the z walk, before callback
resolution. Normal play terminates the visit, while editing preserves the
native fill-then-placeholder order. The deferred wgpu vertex-color path also
keeps the packed alpha in the straight-alpha color stream; the former host
mapping placed it in both `ALPHA_FACTOR` and blend alpha, applying translucent
rectangle opacity twice. The software and wgpu paths now perform the same
single alpha blend.

A focused regression proves the exact float32 geometry, packed color,
unclipped default state, callback suppression, and editing-mode command
order. A release-wgpu audit loaded all 45 Chapter02 levels containing
`BLOCK_SENSOR_BOX_WATER`; every level completed and none submitted a
`RED_CROSS` sprite. The fixed direct `Chapter02_L16` checkpoint matches the
reported four-porthole pool and restores the full translucent blue water
volume over its submerged structure. The complete workspace passes 667 tests
with one intentional long-duration BirdRun audit ignored; strict Clippy,
formatting, diff whitespace, doc tests and the locked stripped release build
are clean. The final L16 PNG SHA-256 is
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`;
the stripped `stella-app` and `stella-headless` hashes are respectively
`0c020a8db952ff5b481914b7d5098ef5f815735953291974756805538d52860e`
and `6a241149e9e98612a8ba66bb622d17180f984254fb7966c4f1fe1b09f516f5d6`.

## Conditional scene-resource access on Purple's retained-pointer path

The follow-up dense-scene audit found that Rust still acquired the shared
`ResourceRuntime` mutex for every ordinary scene object. Most visits used it
neither for sprite resolution nor for a shader or decoration: their atlas or
composite resource had already been retained by `RenderObjectData`. The water
implementation also entered a second render-bridge critical section for every
non-water object only to return false. Both are rehost synchronization costs,
not work performed by Purple's single-threaded dispatcher.

IDA's `sub_10006D5B4` loads the ordinary AtlasSprite directly from
`RenderObjectData+0x90` at `0x10006D8F4`; the composite branch walks the
retained owner at `+0x78` through `0x10006D758..0x10006D870`. Its per-object
Lua `shader` lookup remains at `0x10006D6D8..0x10006D754`. In the surrounding
`sub_10004BAB4`, the ordinary/ray submission finishes at `0x10004C1E0` before
the decoration byte at `+0x141` is tested. Only a present decoration with a
positive count reaches the ResourceManager pointer at `GameLua+0xE0` and the
lookup calls at `0x10004C228..0x10004C2C4`. Hopper independently exposes the
same retained `+0x78/+0x90` ordinary resources and the decoration-only
`[x19,#0xE0]` loads. The water bytes are similarly tested before the native
rectangle draw, so non-water objects never enter that branch.

Scene dispatch now reads the Lua shader field first and acquires
`ResourceRuntime` only when a shader table or an active decoration actually
requires it. Ordinary sprites and rays submit exclusively from their retained
scene snapshot. The rectangular-water bridge is likewise entered only for a
rectangular object whose water bit is set. Decoration lookup and the cached
gold shader path are otherwise unchanged.

Three alternating runs directly construct Chapter02 L16 and issue 30,000
complete `drawGameNative` submissions. The water-fix baseline reports real
times 4.40, 4.00 and 3.88 seconds (median 4.00) and user CPU 3.16, 3.07 and
2.97 seconds (median 3.07). The conditional path reports real times 3.89,
3.93 and 3.80 seconds (median 3.89) and user CPU 3.00, 3.02 and 2.94 seconds
(median 3.00), reductions of about 2.8 and 2.3 percent in this deliberately
scene-dispatch-heavy workload. This is host-overhead evidence, not a universal
frame-rate claim. Existing regressions continue to exercise live shaders,
decorations, retained sprite pointers, water order and callback mutation.

The complete workspace remains at 667 passing tests with the intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace, strict
all-target/all-feature Clippy, doc tests and the locked stripped release build
are clean. The final `stella-app` and `stella-headless` SHA-256 values are
respectively
`7126543179cbe0b1c2f68be342465ba344e0051281a0989edcca699c5e145687`
and `65f48a93277915f632ed792c6ee8f0ee7d7cb54e30e1100917b60b5da2458d44`.

## Retained composite command ownership without per-draw graph copies

The next symbolized Chapter02 L16 scene-dispatch sample exposed a remaining
deferred-host ownership mismatch. `SceneObject` and `SceneDrawObject` already
shared the bound `CompoSprite`, but `scene_object_command` converted the
retained `Arc<Vec<BoundCompositePart>>` back into a freshly allocated owned
vector. Every composite object therefore cloned all child strings, regions
and texture-source strings on every draw. In the five-second baseline sample,
the `Vec<BoundCompositePart>::clone` chain was a dominant descendant of
`push_scene_object`, and the 100,000-submission workload reached a 5.8 GiB
physical footprint while the command queue retained those duplicate graphs.

IDA's `sub_10006D5B4` tests the composite byte at `RenderObjectData+0x138`,
then repeatedly reads the retained owner at `+0x78`. At
`0x10006D760..0x10006D7F8` it obtains one part by index, retains only that part
wrapper, copies its small scalar/string draw record and submits it through
`sub_10006C838` before releasing the wrapper. The ordinary branch instead
passes the retained AtlasSprite at `+0x90` directly at `0x10006D8F4`. No path
allocates or copies the complete composite child vector per object visit.
Hopper independently shows the same `LDR [x21,#0x78]` part-count/index loop,
per-part retain/release calls and the separate `+0x90` ordinary pointer.

`RenderCommand.bound_composite` now shares the immutable retained part vector
through `Arc`, so `SceneObject`, the compact draw snapshot and the deferred
wgpu command all refer to one resource owner. Particle, theme, trajectory,
decoration and direct ResourceManager submissions wrap a newly resolved
composite once and cheaply retain it for repeated commands. The explicit
empty-vector missing-resource sentinel remains shared as well. A focused
regression proves pointer identity across scene object, draw snapshot and
final command; the existing GPU release regression still draws the frozen
child after the active catalog has released it.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The preceding retained-resource-lock build
reports median real/user/system times of 3.36/2.68/0.65 seconds and a median
maximum resident set of about 5.72 GB. The shared-command build reports
2.99/2.48/0.49 seconds and about 4.95 GB, reductions of approximately 11.0
percent real time, 7.5 percent user CPU, 24.6 percent system CPU and 13.4
percent peak resident memory in this deliberately command-retention-heavy
workload. A follow-up symbolized five-second sample contains zero
`BoundCompositePart` vector-clone stacks while the live scene submission path
remains present.

The complete workspace now passes 668 tests with the intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace, strict
all-target/all-feature Clippy, doc tests and the locked stripped release build
are clean. A final 1,200-frame real-wgpu map run reports zero invoked
fallbacks, zero remaining compatibility bindings and empty stderr; its PNG
SHA-256 is
`84659004659ac456c825489b60e6faf282ac770b628ca879222aeecce2ce3b09`.
The stripped `stella-app` and `stella-headless` hashes are respectively
`1f66574b091027391728e1323411c6730c8310a7080be5fc99780a691d316021`
and `eb5bf9886d47c001f42455de92cf2210139cfd34ed95d8902368bf25c6777bd4`.

## Retained AtlasSprite command ownership without per-draw record copies

The follow-up symbolized Chapter02 L16 sample showed that the composite graph
copy had disappeared, but ordinary scene submission still cloned each
`SpriteCatalogRegion` into every deferred command. That record contains the
resolved texture-source string and the SPRT region name, so the supposedly
small copy still allocated and copied two strings for every atlas object on
every `drawGameNative` call. The command queue then retained all of those
duplicates until the host consumed the frame.

IDA and Hopper independently show the ordinary resource lifetime in
`sub_10004C7FC`. The ResourceManager virtual lookup at
`0x10004C89C..0x10004C8B0` returns one AtlasSprite pointer; the function writes
that pointer directly to `RenderObjectData+0x90` at `0x10004C8C0`. The draw
member `sub_10006D5B4` later passes the same `+0x90` value directly into the
AtlasSprite draw path at `0x10006D8F4`. Neither boundary copies the atlas
record, texture path or region name. The composite alternative similarly
stores its retained owner at `+0x78`.

`RenderCommand.bound_region` now carries an `Arc<SpriteCatalogRegion>`.
Ordinary scene objects therefore share one immutable AtlasSprite owner across
`SceneObject`, the callback-safe draw snapshot and the final deferred wgpu
command. Particles model their native retained pointer at `ParticleData+0x20`
the same way. Theme, trajectory, decoration and immediate draw paths wrap a
newly resolved region once and cheaply retain it when producing repeated
commands. Both the software reference renderer and wgpu consume a borrowed
region, so there is no rendering or resource-shadowing semantic change. A
focused regression proves pointer identity across all three ordinary scene
ownership layers; the existing release and same-name-shadow tests continue to
prove the frozen submission-time resource behavior.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The shared-composite baseline reports median
real/user/system times of 3.05/2.49/0.53 seconds and a median maximum resident
set of about 4.96 GB. Sharing the ordinary AtlasSprite record reports
2.61/2.17/0.42 seconds and about 4.05 GB, reductions of approximately 14.4
percent real time, 12.9 percent user CPU, 20.8 percent system CPU and 18.4
percent peak resident memory in this deliberately command-retention-heavy
workload. A follow-up five-second symbolized sample keeps the live
`push_scene_object` path but contains no `SpriteCatalogRegion` or
`BoundCompositePart` clone stack.

The complete workspace remains at 668 passing tests with the intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace, strict
all-target/all-feature Clippy, doc tests and the locked release build are
clean. A final 1,200-frame real-wgpu map smoke test reports zero invoked
fallbacks, zero remaining compatibility bindings and empty stderr; its PNG
SHA-256 is
`46e405eafc2873b41adc51dce1d4ad8077f1003415e3bbc8b976b14ed6cffbde`.
The stripped `stella-app` and `stella-headless` hashes are respectively
`8f907af7dc1bae63837c17e74b6a40587a417a8cc5c2ed7da4cd5b73f65ad469`
and `a8f518953c2bb09010ec27613eb0f0f579790575471cf11a56153da6c9169f5c`.

## Copy-on-write sprite labels across deferred command submission

After retained atlas records became shared, the next symbolized Chapter02 L16
sample still showed `String::write_str`, allocation and byte-copy descendants
under `push_scene_object`. The source was the host-only conversion from the
scene object's already retained `Arc<str>` sprite label into a new owned
`String` for every deferred command. Repeated theme tiles, trajectory points,
decorations and particles had equivalent label copies.

IDA's `sub_10006D5B4` ordinary branch at
`0x10006D8B4..0x10006D8F4` loads the Lua draw object and AtlasSprite pointer
from `RenderObjectData+0x90`, prepares only scalar transforms, and calls
`sub_10006C838`; it never constructs or copies a sprite-name string. The
composite branch does copy its part label with the libstdc++ copy constructor
at `0x10006D7D0..0x10006D7D8`, but the matching release at
`0x10006D888..0x10006D8AC` atomically decrements the old `_Rep` reference
count. This executable therefore uses the old copy-on-write libstdc++ string
ABI rather than duplicating the label bytes. Hopper independently exposes the
same name-free ordinary call and reference-counted composite string lifetime.

`RenderCommand.sprite` now uses `SharedSpriteName`, a small `Arc<str>` wrapper
that preserves the renderer's string-facing API while matching that immutable
shared ownership. Ordinary scene objects, callback-safe snapshots and their
final commands retain one underlying name pointer. Particle data retains the
selected label across draws and replaces it only when a lifetime animation
changes frames. Theme tiles, trajectory particles and decorations create one
shared label before expanding repeated commands. Immediate one-shot paths
convert their owned adapter string at submission and otherwise keep their
existing behavior. A focused scene regression proves both the AtlasSprite and
label pointers remain identical through the complete deferred boundary.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The shared-AtlasSprite baseline reports median
real/user/system times of 2.45/2.06/0.38 seconds and a median maximum resident
set of about 4.03 GB. Shared labels report 2.15/1.86/0.28 seconds and about
3.88 GB, reductions of approximately 12.2 percent real time, 9.7 percent user
CPU, 26.3 percent system CPU and 3.7 percent maximum resident memory in this
deliberately command-retention-heavy workload. Retired instructions fall by
about 12.3 percent. A follow-up five-second symbolized sample keeps the live
`push_scene_object` path while reporting zero sprite-label allocation stacks
and zero AtlasSprite or CompoSprite clone stacks beneath it.

The complete workspace remains at 668 passing tests with the intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace, strict
all-target/all-feature Clippy, doc tests and the locked release build are
clean. A final 1,200-frame real-wgpu map smoke test reports zero invoked
fallbacks, zero remaining compatibility bindings and empty stderr; its PNG
SHA-256 is
`60b3e8d73365a99fbf76d52b26a7a0f091717337a05a201e994f4c4cb39627fa`.
The stripped `stella-app` and `stella-headless` hashes are respectively
`b7f07fd6648329e895f2090c19f655cd8f304693fcddabaac64e69ce4e81cfd0`
and `350fb9681ab5cca4eb1fea743f4830822f7eda6b8b9a3444ea419a75a192c5f7`.

## Per-call vertex geometry outside the copied renderer state

The symbolized Chapter02 L16 follow-up no longer contained retained-resource
or sprite-label allocation stacks, but `push_scene_object` still spent most of
its sampled time in six large `memmove` sites. The ordinary command path was
copying two four-corner arrays embedded in `RenderState` even though those
arrays are used only by `renderMaskedImageNative`, textured lines and rubber
bands. In the deferred host's deliberately retained 30,000-draw workload this
also kept the unused payload alive on every queued scene command.

IDA and Hopper expose a different ownership boundary. In
`sub_100096344`, masked-image positions and UVs are assembled as call-local
four-element vectors and passed into the renderer submission; they are not
written into the copied GL-context state. `sub_10006DB0C` computes the
textured-line vertices independently and allocates/copies one 0x20-byte
geometry block for that special call. `sub_100030EB0` similarly constructs
the rubber-band arrays on its stack and passes them in `x2`/`x3` to the draw
virtual at offset `+0x40`. The general state copied around those calls
contains the scalar transform, alpha, matrices, pivot and clipping state, not
either vertex array. Both disassemblers therefore agree that the geometry is
a draw-submission argument rather than persistent renderer state.

`RenderState` now contains only the common copied state. `RenderCommand`
owns an optional `SpriteGeometrySubmission`, whose explicit masked quad or
native atlas-corner quad is retained behind `Arc`; ordinary scene, theme,
particle, animation and resource commands carry `None`. Masked-image,
textured-line and rubber-band constructors attach their exact existing
float32-derived vertices only to the special command. The software reference
renderer and wgpu dispatcher consume that command payload directly, including
the existing non-finite rejection, triangle order, UV order and native atlas
rotation behavior. A layout regression keeps the rare optional payload at two
machine words and prevents the arrays from silently returning to the copied
state.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The shared-label baseline reports median
real/user/system times of 2.13/1.83/0.29 seconds and a median maximum resident
set of about 3.87 GB. The split-geometry build reports 1.97/1.68/0.28 seconds
and about 2.97 GB, reductions of approximately 7.5 percent real time, 8.2
percent user CPU, 3.4 percent system CPU and 23.3 percent peak resident memory
in this command-retention-heavy diagnostic. Across the corresponding six
dominant `push_scene_object` bulk-copy sites, a three-second symbolized sample
falls from 738 `memmove` samples to 325 while preserving the live scene path.

The complete workspace passes 669 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, doc tests and the locked stripped release build are clean.
A final 1,200-frame real-wgpu map smoke test reports zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr; its PNG SHA-256 is
`144b9342c4d0dff0c9a1bfc46c9ca42b2a51130e71086cfd5c1eea9b70f32832`.
The stripped `stella-app` and `stella-headless` hashes are respectively
`c73fddc0e52a4e1d536125a726e917cee139d27c59c8e65aea0a57b3d56cf10d`
and `4484889ba5e65ee03b1d0e3ff285ae838eb3e9b03886cc7de8ed0182ee53a364`.

## Retained shader and Dirt draw objects outside ordinary commands

After removing the per-call vertex arrays, the host's common
`RenderCommand` was still 616 bytes. Two absent optional values accounted for
most of the remainder: the inline `SpriteShader` alternative occupied 88
bytes and the inline `DirtRenderCommand` alternative occupied 152 bytes.
Every ordinary scene object paid for both even though Chapter02 L16 does not
submit either payload. Dirt commands also regenerated both triangulated
polygon groups and copied their texture strings on every draw.

IDA gives the ordinary native call a much smaller boundary.
`sub_10006C838` accepts four pointer arguments followed by nine scalar float
arguments. At `0x10006CA70` it tests the fourth pointer, optionally resolves
the shader object through `sub_10006CB08`, then calls the atlas draw at
`0x10006CAC0`; no shader parameter block is embedded in the scalar call.
`sub_10006D5B4` performs the live Lua `shader` lookup at
`0x10006D6D8..0x10006D744`, keeps the result in a call-local object and passes
its pointer—or null—to each `sub_10006C838` invocation. Hopper independently
shows the same local `r21`, the null branch and the pointer argument for both
ordinary and composite submissions.

Dirt has an even earlier persistent boundary. As previously recovered from
`sub_10001F98C`, DirtMechanics resolves its background/foreground images once
and constructs two retained DrawablePolygon objects. Collision cutting
replaces the foreground shape; the ordinary draw path does not rerun polygon
triangulation. The rehost now caches the corresponding `DirtRenderCommand` in
`DirtComponent`, shares it across deferred draws and uses `Arc::make_mut` only
when a cut changes the foreground. An already queued command therefore keeps
the old mesh exactly as an immediate native draw would, while later commands
observe the replacement. Shader and Dirt command fields are both retained
pointer-sized `Arc` options, reducing the common Rust command from 616 to 392
bytes. A focused regression proves pointer identity across unchanged Dirt
draws, copy-on-write separation after a cut and identity between the current
component cache and the new submission.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The split-geometry baseline reports median
real/user/system times of 1.83/1.61/0.22 seconds and a median maximum resident
set of about 2.97 GB. Retaining shader and Dirt objects reports
1.71/1.56/0.15 seconds and about 1.99 GB, reductions of approximately 6.6
percent real time, 3.1 percent user CPU, 31.8 percent system CPU and 33.0
percent peak resident memory in this command-retention-heavy diagnostic. In
the matching three-second symbol sample, the dominant `push_scene_object`
bulk-copy sites fall again from 325 `memmove` samples to 164.

The complete workspace passes 670 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, doc tests and the locked stripped release build are clean.
A final 1,200-frame real-wgpu map smoke test reports zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr; its PNG SHA-256 is
`a0e809d37e4207d1f2f51aac7633ff796da474eee7f2339e2b8ed6ab3221305a`.
The stripped `stella-app` and `stella-headless` hashes are respectively
`b1558834edc64f8a6a9efb715f5167e8d1a265e419f7af44b55fee1ff0f936c1`
and `a8c0baaa470033dd6baedeb6ed6de19b6d11d88069d452ab574acb54a06160a2`.

## Retained alpha-masked texture submission payload

The next common-command audit found three more fields that ordinary sprites
never use: an owned fill-image name, its scale and an optional resolved image
binding. They survived from the earlier masked-terrain reconstruction, but
the Rust command copied the name and binding on every scene submission even
though `SceneObject` had already retained both native-style owners. This was
also structurally different from Purple: the optional masked path is selected
by an image pointer, not by embedding an image descriptor in every draw.

IDA's decompilation of `sub_10008D428` shows the complete special-call
boundary. After viewport rejection it writes argument `a2` to renderer
`+0x28`, resolves the mask image from the AtlasSprite and writes that pointer
to `+0x30`; only then does it divide the fill-image width and height by scalar
arguments `a6/a7` and emit six position, fill-coordinate and mask-coordinate
vertices. Hopper independently shows the same `r19[5] = r21`, `r19[6] =
resolvedImage` stores and the later divisions by its stack scalars
`var_138/var_134`. Together with the previously recovered
`sub_10004BAB4` branch, `setTexture` image pointer at RenderObjectData `+0x80`
and float32 texture scale at `+0xC4`, both tools establish that this is one
rare retained pointer payload rather than common inline state.

`RenderCommand` now has one optional `Arc<SpriteTextureSubmission>` containing
the retained name, float32-derived scale and submission-time image binding.
The live `SceneObject`, its scalar draw snapshot and every unchanged deferred
command share the same allocation. `setTextureScale` updates the live payload
with `Arc::make_mut`, so a command already queued in the current immediate
stream keeps its old scale while subsequent draws observe the new one.
Selected-texturized-object calls create the same explicit payload once for
their one-shot submission; all ordinary animation, theme, particle,
trajectory and resource commands carry only `None`. The common 64-bit command
therefore falls from 392 to 360 bytes. Pointer-identity and queued-before-
setter regressions lock both ownership cases.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The retained-shader/Dirt baseline reports
median real/user/system times of 1.83/1.62/0.20 seconds and a median maximum
resident set of 1,992,785,920 bytes. The retained texture-payload build
reports 1.75/1.58/0.16 seconds and 1,763,803,136 bytes, reductions of about
4.4 percent real time, 2.5 percent user CPU, 20.0 percent system CPU and 11.5
percent peak resident memory in this deliberately retained-command-heavy
diagnostic.

The previously recovered rectangular-water branch was rechecked at the same
checkpoint rather than approximated through the texture change. A direct
real-wgpu Chapter02 L16 load renders the translucent blue volume over the
four-porthole structure, submits no red editor cross and produces empty
stderr; its PNG SHA-256 is
`d0fffab9fe0a41408e014a73bcbb840edbaefc455eef3db93d83e82dce4ce01e`.
The complete workspace passes 671 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, doc tests and the locked stripped release build are clean.
A final 1,200-frame real-wgpu map smoke test reports zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr; its PNG SHA-256 is
`95aa759f1b8fc15e6f6e8b7244b8c7420c23b3490dec0415bba08697c09b93c6`.
The stripped `stella-app` and `stella-headless` hashes are respectively
`0eefd9d1ae02b52ed774219706f7b588584208f021da9c217567e9a57e82149d`
and `18d4a660c7c4ad38cb82c045e70a795d6cb7840ccb8e7b78c327b64df72532e0`.

## DirtMechanics-only clipping and compact wgpu sprite streams

The remaining sprite pipeline still carried a host-invented analytic Dirt
hole approximation. If a collision cut had been queued before the retained
`DirtComponent` became available, every ordinary sprite command received a
heap vector of octagonal holes. The software renderer tested those holes per
pixel, while wgpu uploaded a fixed array of 64 `vec4` values for every draw
and interpolated an otherwise unused sprite-local vertex coordinate. This was
not an ownership or rendering boundary present in Purple.

IDA's `sub_1000208D4` decompilation shows the complete Dirt render method. It
first calls `sub_100024A08(a1[25])` for the retained background
`DrawablePolygon`, then walks the half-open vector at `a1[26]..a1[27]` in
16-byte steps and calls the same render function for each foreground polygon.
Hopper independently recovers the same three-block function: it renders
`arg0[0x19]`, loads the begin/end pointers from `arg0[0x1a]` and
`arg0[0x1b]`, and advances the foreground iterator by `0x10`. Neither path
submits an ordinary sprite, a hole array or a special fragment-discard
uniform. Collision cuts remain retained in `SceneObject::dirt_holes` until
they can update the real Dirt polygon; only the incorrect renderer-side
fallback has been removed.

The common `RenderCommand` is now 336 rather than 360 bytes. The wgpu
`DrawUniform` shrinks from 1,088 to the four native-style `vec4` rows (64
bytes), and `GpuVertex` shrinks from 48 to 40 bytes after removing its local
hole coordinate. A layout regression fixes all three boundaries. Dirt still
reaches wgpu exclusively as the recovered opaque background triangle stream
followed by its foreground triangle streams, and the retained texture-pointer
tests continue to cover catalog replacement.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The retained-texture baseline reports median
real/user/system times of 1.72/1.57/0.14 seconds and a median maximum resident
set of 1,763,885,056 bytes. The Dirt-only clipping build reports
1.66/1.52/0.13 seconds and 1,650,655,232 bytes, reductions of approximately
3.5 percent real time, 3.2 percent user CPU, 7.1 percent system CPU and 6.4
percent peak resident memory in this retained-command-heavy diagnostic. A
real-wgpu 1,200-frame L16 comparison remains pixel-identical while its median
retired instruction count falls by about 1.4 percent.

The earlier native rectangular-water replacement remains intact: a direct
real-wgpu Chapter02 L16 load renders the translucent blue volume and submerged
structure without the red editor cross, invokes no compatibility fallback and
has empty stderr. Its PNG SHA-256 is
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
The complete workspace passes 671 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, documentation and the locked release build are clean. A
final 1,200-frame real-wgpu map smoke test reports zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr; its PNG SHA-256 is
`d0451e60f2e15a0c04141cc4051291a96ead4ea02c78764c97f35035155c2351`.
The release `stella-app` and `stella-headless` hashes are respectively
`b4240f0d42aa4ab2b874abc81b80faeda9682e0c40618c1f8771303b78fe263b`
and `704647a723c5df5ad846aa2ce313780e9cc72080038f13761c1dafad85b50bb2`.

## Float32 renderer submission snapshots

The next retained-command audit found that the deferred host still copied the
live renderer state as doubles even though Purple closes that numeric boundary
before an ordinary sprite reaches its renderer. This doubled the storage for
translation, scale, angle, pivot, optional affine matrices and draw size in
every queued command, and postponed float narrowing until the wgpu consumer.

IDA's `sub_100044CFC` (`setRenderState`) reads the Lua-facing values through
`sub_10052859C`, narrows them to `s` registers and stores translation at GL
context offsets `+0x20/+0x24`, scale at `+0x28/+0x2c`, pivot at
`+0x30/+0x34`, alpha at `+0x40` and angle at `+0x48`. Its
`__sincosf_stret` result and four matrix members at `+0x10..+0x1c` remain
single precision as well. IDA's ordinary sprite member `sub_10006C838`
independently exposes its position and transform arguments as floats and
performs its arithmetic in float registers. Hopper confirms the same `s0/s8`
stores, offsets and float argument sequence in both procedures.

`RenderState` remains widened while Lua adapters assemble a call, preserving
their existing coercion behavior. At the recovered immediate-renderer
boundary it is now converted once into `RenderSubmissionState`, which carries
the copied native state as `f32`; the command-local X/Y arguments close at the
same boundary. The software reference renderer and wgpu preparation path both
consume that snapshot directly, so they no longer repeat conversions or
retain a host-double representation. The snapshot is 124 bytes and the common
`RenderCommand` is now 216 rather than 336 bytes.

Three alternating Chapter02 L16 runs issue 30,000 complete
`drawGameNative` submissions. The Dirt-only baseline reports median
real/user/system times of 1.65/1.52/0.13 seconds, median maximum resident set
of 1,650,491,392 bytes and 24,133,503,240 retired instructions. The float32
snapshot build reports 1.54/1.41/0.12 seconds, 1,081,327,616 bytes and
23,044,775,996 instructions, reductions of approximately 6.7 percent real
time, 7.2 percent user CPU, 7.7 percent system CPU, 34.5 percent peak resident
memory and 4.5 percent instructions in this deliberately command-retention-
heavy diagnostic.

The direct real-wgpu Chapter02 L16 checkpoint remains byte-identical to the
water-fix baseline: it renders the translucent blue pool and submerged
structure, submits no red editor cross, invokes no compatibility fallback and
has empty stderr. Its PNG SHA-256 remains
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
The complete workspace passes 671 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, documentation and the locked release build are clean. A
final 1,200-frame real-wgpu map smoke test reports zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr; its PNG SHA-256 is
`0af0eae36441a5be042e08672ac29032e71a6775ba352fce107d87d423227024`.
The release `stella-app` and `stella-headless` hashes are respectively
`fbab87a46c7d3670e67628676b2fb65273dabe999cebc28961434e272e7c4286`
and `524a6fadfc6c2dc6c1a23afe2e67b7062a10786b7ddb46d48d72471e2439fc1b`.

## Retained RenderObjectData callback slots and single scene visits

The next Chapter02 L16 symbol profile showed that the previously consolidated
`DrawCallbackRecord` was still reached through a separate Rust name tree for
every visible object. Callback-free objects also performed a second scene-name
tree lookup to build the ordinary draw snapshot. These searches were host
ownership adapters rather than work performed by Purple's scene dispatcher.

IDA's `sub_10004BAB4` resolves the current leaf name with
`sub_100070278(GameLua+0x2E0, name)` exactly once at `0x10004BD24` and retains
the returned `RenderObjectData*`. The same pointer supplies the embedded Lua
object holder at `+0x20`, the pre-draw holder at `+0x158`, the complete ordinary
visual record passed to `sub_10006D5B4`, and the post-draw holder at `+0x160`.
The pre holder is invoked around `0x10004BFA4`; the post holder is loaded at
`0x10004C300`. Hopper independently exposes `BL sub_100070278`, retains the
result in `x21`, loads `[x21,#0x158]` and `[x21,#0x160]`, forms the callback
object as `x21+0x20`, and passes that same `x21` through the ordinary draw
branch. There is no second object or callback-name lookup in this visit.

The mutation boundary is equally direct. IDA's `sub_10004E3C0` and
`sub_10004E570` each resolve the object once with `sub_10005DAF8`, then replace
the retained function pointer at decimal offsets 344 and 352 respectively.
Hopper confirms the single lookup followed by `LDR/STR [x19,#0x158]` or
`LDR/STR [x19,#0x160]`.

Each Rust `SceneObject` now retains a stable callback-record slot allocated at
construction. The name tree remains only for infrequent setter and removal
members; the draw loop follows the retained slot directly. One scene lookup
returns callback state and the initial ordinary draw snapshot together. A
pre-draw callback still forces a live visual reload after Lua returns, and its
post callback is reread from the same slot, preserving same-visit visual and
callback replacement semantics. Removed slots are not recycled for a
different name during the same level, preventing an object removed by its own
callback from observing a newly constructed object's holders. Level teardown
clears both the index and slot storage.

A focused regression removes only the host setter/removal name index after
construction and proves that native drawing still invokes the retained pre
callback. Existing tests continue to cover same-name replacement, immediate
removal, failed and successful level loads, pre-draw visual mutations,
same-visit post replacement, live z-order changes and rectangular water's
early replacement branch.

Three alternating stripped-release runs issue 30,000 complete Chapter02 L16
`drawGameNative` submissions. The float32-snapshot baseline reports median
real/user/system times of 1.48/1.37/0.10 seconds, 22,721,346,051 retired
instructions and 5,175,723,438 CPU cycles. The stable-slot build reports
1.21/1.11/0.09 seconds, 17,312,480,397 instructions and 4,260,982,056 cycles,
reductions of approximately 18.2 percent real time, 19.0 percent user CPU,
10.0 percent system CPU, 23.8 percent instructions and 17.7 percent cycles.
Median maximum resident memory is unchanged at 1,085,177,856 bytes. This is a
deliberately scene-dispatch-heavy diagnostic, not a universal frame-rate
claim.

In matching 100,000-visit symbol samples, the old callback-tree branch had
178 samples including 71 `memcmp` samples, while the second scene-snapshot
lookup had 263 samples including 75 `memcmp` samples. Both branches disappear
after the change. The direct slot access has three samples and no `memcmp`;
the one required native-style scene-name-map lookup remains visible.

The direct real-wgpu Chapter02 L16 checkpoint remains byte-identical to the
water-fix baseline: the translucent blue pool and submerged structure render
without the red editor cross, no compatibility fallback is invoked and stderr
is empty. Its PNG SHA-256 remains
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
The complete workspace passes 672 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target and
all-feature Clippy, documentation and the locked release build are clean. A
final 1,200-frame real-wgpu map smoke test reports zero invoked fallbacks,
zero remaining compatibility bindings and empty stderr; its PNG SHA-256 is
`20d62dceac610657093c4b36936f78f9e2d9b9efbc947c9e84f70d0d6831a030`.
The release `stella-app` and `stella-headless` hashes are respectively
`1c8699435a862bc860fcd3e406027d196eb3489b488a88d92e384a40f873a508`
and `4832a17e20c3e36025ca6eb5b186726084f9baa2255b117b028a0250056d5054`.

## Persistent scene context, anchor-ordered trajectories and water-state audit

The next scene-dispatch audit found that the deferred host still treated each
object callback as an isolated save/install/restore scope. Purple instead owns
one persistent `GL_Context` across the complete z-tree walk. This difference
was largely invisible to ordinary sprite commands, which snapshot their own
transform, but it changed Lua pre/post helpers, trajectory placement,
decoration rotation and the state inherited after a rectangular water draw.

IDA's `sub_10004BAB4` invokes the pre holder at
`0x10004BFA4..0x10004BFDC`, then begins installing alpha, camera translation,
scale, pivot and the composed object/sprite angle at `0x10004BFE0`. The
ordinary draw reaches `sub_10006D5B4` at `0x10004C144`; only afterward is the
post holder loaded and invoked at `0x10004C300..0x10004C338`. No context
save/restore surrounds those calls. At the end of each SpriteSheet/name
vector, `0x10004C350..0x10004C354` restores only the two scale fields to the
draw-start world-scale snapshot. Hopper independently shows the pre
`[x21,#0x158]` load before the context stores, the ordinary draw, the post
`[x21,#0x160]` load, and the two terminal scale stores in the same order.

The ordinary member mutates the live context further. Its flip/body-scale
branch divides the secondary pivot-offset translation by object scale and, on
a horizontal flip, replaces the live angle with the negated object angle.
The masked-texture branch bypasses that member and therefore leaves raw-scale
camera translation and the composed angle. Ray and flash-animation branches
leave the caller-installed base context. The Rust dispatcher now installs the
matching branch-specific post state only after pre returns, leaves it live
through post and the following entry, and resets scale only when the native
name vector ends. The pre-callback snapshot consequently shrinks to the eight
fields actually read before context installation, avoiding per-object pivot
and transform work.

The same procedure's latch at `0x10004BF28..0x10004BF9C` proves that the two
trajectory records are not a scene-wide pre-pass. They are inserted directly
before the first visible object whose `RenderObjectData+0x140` controllable or
`+0x148` level-goal byte is set, and their trajectory context remains live for
that anchor's pre callback. The trail implementation now lives in
`draw_registration/scene/trails.rs`, is invoked at that exact tree position,
and emits nothing when no visible anchor exists.

Decoration iteration at `0x10004C220..0x10004C2FC` starts from the object
angle. After each draw, Purple computes `angleIncrement * PI` followed by an
FMADD with the `1/180` float constant and normalizes with `fmodf(2*PI)`.
Authored increments are therefore degrees, not radians. The rehost now keeps
the same float32 FMUL/FMA boundaries. It also installs the recovered raw
object-times-decoration scale, divides camera and object position by that
scale, and enters the shared ResourceManager HPIVOT/VPIVOT draw instead of
reusing the ordinary body's host-screen transform. The final decoration draw
context remains visible to post. Regressions cover three successive 90-degree
draws, divided resource coordinates and the live callback ordering.

The rectangular-water path at `0x10004BDA4..0x10004BE88` was rechecked in the
same context audit. Purple constructs and copies a complete default GL context
before submitting the untextured water rectangle, clears its scissor, and
leaves that state current. Normal gameplay then terminates the visit; editing
continues into pre and the `RED_CROSS` placeholder with the default context.
The implementation and focused editing regression now preserve that full
ordering. A real `Chapter02_L16` construction finds every authored water body,
emits one translucent unclipped rectangle for each, and emits no editor-cross
sprite. The release-wgpu direct checkpoint renders the blue pool over the
complete submerged structure and remains byte-identical to the established
water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.

Three alternating stripped-release measurements each issue 30,000 complete
Chapter02 L16 `drawGameNative` submissions. The retained-callback-slot baseline
reports median real/user/system times of 1.22/1.08/0.13 seconds; the persistent
context and compact pre-snapshot build reports 1.13/0.99/0.13 seconds,
reductions of approximately 7.4 and 8.3 percent in real time and user CPU,
with median system CPU unchanged, in this intentionally scene-dispatch-heavy
diagnostic. This is not a universal frame-rate claim.

The complete workspace passes 675 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, strict
all-target/all-feature Clippy and the locked release build are clean. The
release `stella-app` and `stella-headless` hashes are respectively
`1d745f5ad3f0f5adb4f560954e5c6fbdbbf22799a351e813e5e2412bf35a216a`
and `5024a85419116ef4bf355e0790761b0ed1c53e91db45cd1adbbf190622ca815e`.
A final 1,200-frame release-wgpu run against a copied runtime save reports
zero invoked compatibility fallbacks and zero remaining compatibility
bindings.

## Static shader-key reuse in the native scene submission path

A symbolized 30,000-draw Chapter02 L16 sample placed the next avoidable host
cost in the ordinary-object `shader` lookup. The semantic lookup itself is
native and must stay live: GoldTransformer assigns and clears this field at
runtime. The excess was constructing and interning a new Lua string from the
Rust `&str` key for every visible ordinary object.

IDA shows the exact bridge sequence in `sub_10006D5B4`. After the caller's
pre-draw callback, `0x10006D6DC..0x10006D710` reads the retained Lua object at
RenderObjectData `+0x38`, passes the static `"shader"` literal at
`0x10006D6E0` to the Lua field bridge and tests the result. When present,
`0x10006D720..0x10006D74C` passes that same static literal to
`sub_1000222E4` and constructs the cached shader. Hopper independently shows
the same two references to `aShader` and the same nil branch to
`0x10006D754`.

The registered Rust native member now retains one interned Lua string for
that literal and pushes the retained key for each raw lookup. It still reads
the current object table after pre and on every visit; no shader result is
cached. The existing live-mutation regression therefore continues to prove
that assigning `2d-sprite-gold` affects the next submission and clearing the
field removes it on the following draw.

In the before symbol sample, the `mlua::Table::raw_get` instantiation for this
line accounted for 243 of 1,656 main-thread samples and included repeated Lua
string interning/comparison. In the matching after sample, that instantiation
and its key-construction stack disappear; the retained-key lookup is inlined
into the scene member. Three alternating stripped-release runs each issue
30,000 complete Chapter02 L16 draws. The baseline reports median
real/user/system times of 1.12/1.02/0.09 seconds and the retained-key build
reports 1.08/0.99/0.09 seconds, reductions of approximately 3.6 percent real
time and 2.9 percent user CPU with system CPU unchanged. This remains a
scene-dispatch-heavy diagnostic rather than a universal frame-rate claim.

The direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to
the water/state-order baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
It renders the full translucent blue pool and submerged structure without an
editor cross. The complete workspace passes 675 tests with one intentional
long-duration BirdRun audit ignored. Formatting, diff whitespace,
documentation, strict all-target/all-feature Clippy and the locked release
build are clean. The release `stella-app` and `stella-headless` hashes are
respectively
`a80daa3823279e99e5455adf424ad47e96223e09ba730d2083b5e5c35318beef`
and `46aae11615746766c9d434c56545dd4a76703590cf53d1f90dc3b36b631a0049`.
A final 1,200-frame release-wgpu run against a fresh copied AppData directory
reaches the island map with zero invoked fallbacks, zero remaining
compatibility bindings and empty stderr; its PNG SHA-256 is
`7f7804a95612a7ce2eab1ef4a193b3db351ade5db001ede0f294ba458f82dcc1`.

## Single resource-owner handoff and uninterrupted scene lookup

The next ordinary-submission audit found two remaining synchronization and
ownership adapters around the recovered scene path. `SceneDrawObject::from`
already retained the sprite label, atlas/composite pointer and optional
texture while the render lock was released for Lua, but command construction
retained all of them a second time and immediately released the snapshot's
copies. Separately, `NativeSceneWalk::next` yielded the leaf name and the
dispatcher reacquired the same bridge solely to perform the required scene
name-map lookup.

Purple has neither boundary. In `sub_10004BAB4`, `0x10004BD1C` forms the
current leaf-string address and `0x10004BD24` immediately calls
`sub_100070278`; the returned RenderObjectData pointer remains live through
the callbacks and ordinary submission. In `sub_10006D5B4`, the composite
branch reads the retained owner at `+0x78`, while the ordinary branch loads
the atlas pointer at `+0x90` and passes it directly to `sub_10006C838` at
`0x10006D8B4..0x10006D8F4`. Hopper independently shows the same contiguous
leaf lookup and direct `+0x78`/`+0x90` pointer use.

The Rust walk now resolves the compact `SceneDrawVisit` during the same bridge
acquisition that reads the live z/sheet/name vector, then releases the lock
before any Lua callback. It still reloads vector length on the following
iterator step, so callback-driven append, removal and z movement remain
observable. Ordinary submission consumes the compact draw snapshot and moves
its already-retained resource owners into the deferred wgpu command. The
command therefore keeps exactly the one host reference required past the
native immediate-draw boundary instead of performing a second retain/release
round trip.

Focused scene regressions continue to cover pointer identity, same-name
replacement, resource shadowing/release, dynamic shader assignment and clear,
pre/post mutation, live z movement, composite drawing and GoldTransformer.
Three alternating stripped-release runs each issue 30,000 complete Chapter02
L16 draws. The previous retained-shader-key build reports median
real/user/system times of 1.11/1.00/0.10 seconds and the uninterrupted
lookup/owner-transfer build reports 1.09/0.98/0.10 seconds, reductions of
approximately 1.8 percent real time and 2.0 percent user CPU with system CPU
unchanged. This is again a scene-dispatch-heavy diagnostic, not a universal
frame-rate claim.

The direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to
the established water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
The complete workspace passes 675 tests with one intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, strict
all-target/all-feature Clippy and the locked release build are clean. The
release `stella-app` and `stella-headless` hashes are respectively
`0422195c123774929f1aec507175e4f975e72f3d681fbf063f85eec4b711f397`
and `70fa5b3ca03488c382ee4cf18570f6909df717ba4c5a7592454fc0e4e6f3bbfb`.
A final 1,200-frame release-wgpu run against a fresh copied AppData directory
reaches the island map with zero invoked fallbacks, zero remaining
compatibility bindings and empty stderr; its PNG SHA-256 is
`4673877261ea2012eb5ce409f6fdb7ffb8b3b08b20fb3b22ad61cb00580af4a0`.

## Pre-frame dense-level profiling, contiguous context submission and retained atlas owners

The next performance audit moved from command-retention microbenchmarks to a
complete deterministic `GameScene:update`/`GameScene:draw` route. The
headless diagnostic executable now accepts `--pre-eval`, which runs setup Lua
after boot but before its fixed 60 Hz frame loop. The audit uses that boundary
to load `Chapter01_L50`, dispatch `EID_START`, install the shipped GameScene
callbacks and then run the same Lua update, physics, scene walk and draw calls
on every measured frame. A 1,200-frame probe constructs the full dense level
with zero invoked compatibility fallbacks.

The first symbolized 100,000-frame sample exposed one synchronization split
inside ordinary scene submission. The Rust dispatcher installed the final
per-object GL state under one render-bridge lock, released it for a raw Lua
`shader` lookup, and reacquired it to prepare the draw. That raw lookup cannot
invoke a metatable or callback, so the split was host-only.

IDA's `sub_10004BAB4` shows the uninterrupted native sequence. The pre holder
returns at `0x10004BFDC`; `0x10004BFE0..0x10004C104` writes alpha, camera,
scale, pivot and rotation to the one `GL_Context*` retained in `x20`; and
`0x10004C138..0x10004C144` immediately passes the same GameLua/object pair to
`sub_10006D5B4`. That member mutates the same live context and returns before
the post holder is loaded at `0x10004C300`. Hopper independently shows the
same `x20` context stores, direct ordinary-member branch and post-holder load,
with no ownership or synchronization boundary between state installation and
draw. The common Rust path now installs the post context and submits the
deferred command during one bridge acquisition. The flash-animation branch
similarly installs its context and derives its transform without an
intermediate unlock/relock.

The same profile also showed repeated allocation around atlas lookup. The
resource cache already froze each `SpriteCatalogRegion` when its SpriteSheet
was created, but every theme, trajectory, particle, direct-sprite and resource
draw cloned that complete region into a new `Arc`. This did not model the
executable's concrete sprite ownership.

IDA's ResourceManager draw `sub_10045C0AC` calls `sub_10045BDDC` once and,
after checking the type tag at the returned resource-stack entry, loads the
concrete sprite pointer directly from entry `+0x10` before tail-calling either
the CompoSprite or AtlasSprite draw member. The resolver itself walks the
ordered name tree and returns `last_entry - 0x18`; it performs no atlas-region
copy. Hopper independently shows `LDR x0, [x0,#0x10]` in both type branches.
The Rust SpriteSheet cache now stores shared atlas owners and every active
lookup returns a pointer clone to that same allocation. Deferred commands keep
that owner across later shadow/release exactly as before, while the lookup no
longer copies the texture-source string and sprite record or allocates a fresh
owner. A focused regression asserts that the cached entry and two consecutive
active lookups are pointer-identical.

Three alternating stripped-release runs each execute 30,000 complete L50
update/draw frames. The preceding build reports median real/user/system times
of 5.65/5.59/0.03 seconds; the contiguous-context/retained-atlas build reports
5.52/5.47/0.04 seconds, reductions of approximately 2.3 percent real time and
2.1 percent user CPU in this dense deterministic workload. The small system
time difference is within run noise. Matching five-second symbol samples move
the scene closure's collapsed top count from 244 to 213, malloc-tiny from 118
to 87, and the combined pthread mutex lock/unlock counts from 97 to 50. The
shipped Lua post callbacks and native-style scene/name tree comparisons remain
the dominant behavior and are deliberately unchanged.

The complete workspace passes 675 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, strict
all-target/all-feature Clippy and the locked release build are clean. The
direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to the
established water/state-order baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`;
the translucent blue pool and submerged structure render without an editor
cross. A separate 1,200-frame copied-AppData map run reports zero invoked
fallbacks, zero remaining compatibility bindings and empty stderr; its PNG
SHA-256 is
`54ec7817d2a6598a5c622119c47e6d1c3a1ef8241171a149bb8c90c86694fa4c`.
The release `stella-app` and `stella-headless` hashes are respectively
`40dc833d5d4a057582b79046806f089c84744ce4a489b86c10139615bd7e3d65`
and `e49c347987719bb5e86222bd29e85557c6a9b53eb8a7598ba2fde274073f4236`.

## Direct AtlasSprite owners in active resource-stack entries

The retained-region follow-up removed the per-draw allocation but exposed one
remaining ownership mismatch: `active_atlas_catalog_region` still resolved the
last active resource entry to an owner string and vector index, then searched
the owning SpriteSheet catalog again by sprite name. Purple's active resource
entry already contains the concrete Sprite pointer, so this second tree walk
has no native counterpart.

IDA's central lookup `sub_10045BDDC` walks the ordered name tree at
`Resources+0x588`, loads the selected priority-vector begin/end pair at
`0x10045BEC8`, returns `end-0x18`, and applies the optional type tag directly
to that record. Resource draw `sub_10045C0AC` calls it once at
`0x10045C0D0`; both the CompoSprite and AtlasSprite branches then load the
concrete resource pointer from returned entry `+0x10` at `0x10045C0E4` or
`0x10045C110` before tail-calling the draw member. There is no owner lookup,
sheet-vector scan or sprite-name comparison after the active entry has been
selected. Hopper independently recovers the same `Resources+0x588` tree,
`end-0x18` priority selection, type check and both `LDR X0, [X0,#0x10]`
instructions.

Each Rust atlas `SpriteResourceEntry` now receives its immutable
`Arc<SpriteCatalogRegion>` once when the successful SpriteSheet constructor
resolves texture bindings. Active lookup clones that direct owner from the
last priority entry. Replacement and release remove only their corresponding
stack records, while already deferred commands retain the prior owner as the
native immediate caller would retain its pointer. The slower owner/name
resolver remains solely for direct diagnostic fixtures that install parsed
sheet values without completing the production constructor. The existing
shadow/release regression now also proves pointer identity between the
SpriteSheet cache, active stack entry and two repeated active lookups.

Three alternating stripped-release runs each execute 30,000 complete
Chapter01 L50 update/physics/draw frames. The first preceding run had a 7.02
second wall-time scheduling outlier; the robust three-run medians are
5.54/5.45 seconds real/user for the preceding build and 5.49/5.42 seconds for
the direct-entry build. The change is a small approximately 0.9/0.6 percent
reduction in this full-frame workload; its primary purpose is exact native
ownership rather than a broad frame-rate claim.

The complete workspace passes 675 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, strict
all-target/all-feature Clippy and the locked release build are clean. The
direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to the
established water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`,
and visual inspection confirms the translucent pool and submerged objects.
A copied-AppData 1,200-frame island-map run completes with zero invoked
fallbacks, zero remaining compatibility bindings and empty stderr; its
visually checked PNG SHA-256 is
`af6384ebf585acd81fcd72cc7ebf9780b086de01c2e4e3696ff01149f7077330`.
The release `stella-app` and `stella-headless` hashes are respectively
`64da40855cc7a450f3bc1e673cd53532dad7fed738a4f5eadee0e49f7573ffff`
and `2adffe67fd351f38d255fc746135d0f36789a6ea2d120d0d0800c65efda031d0`.

## Live CompoSprite owners and submission-time child snapshots

The direct-entry audit also exposed a distinction between AtlasSprite and
CompoSprite ownership. AtlasSprite records are immutable after construction,
but Purple allows `setCompoSpriteEntry` to mutate the Entry records of an
already-retained concrete CompoSprite. The previous Rust path rebuilt a new
bound child vector during active lookup and then stored that frozen vector on
scene objects and particles. Objects created before an Entry mutation could
therefore keep drawing stale transforms even though the executable retains
the same live CompoSprite pointer.

IDA's `sub_100449CFC` resolves the concrete CompoSprite and selects its Entry
by index or name, then writes x, y, scaleX, scaleY, flipX, flipY, angle and
visible directly at Entry offsets `+0x28` through `+0x44`. A sprite-name
change enters `sub_1004375E8`, which edits the same CompoSprite's ordered Entry
map at `CompoSprite+0x30`, swaps the retained Entry/name/AtlasSprite pointer
and calls `sub_100436D40` to refresh its bounds. IDA's native `setSprite`
member `sub_10004C7FC` resolves the composite once and stores that exact
pointer into RenderObjectData `+0x78` at `0x10004C850..0x10004C86C`.
Hopper independently recovers the same in-place Entry/map update, bounds
refresh and direct RenderObjectData pointer store. Together with the active
resource-entry `+0x10` loads, this shows that resource stacks, particles and
scene components share one concrete mutable owner rather than copied child
arrays.

Rust now creates one `CompositeSpriteOwner` for each successful CompoSprite
constructor and stores that owner directly in its active resource-stack
entry. Scene components and particles retain the same `Arc`. A successful
`setCompoSpriteEntry` rebuilds the bound child snapshot only once and publishes
it through that existing owner, so previously created objects observe the
mutation without repeating part/region binding on every draw. The wgpu bridge
still freezes an immutable `Arc<Vec<BoundCompositePart>>` at each native
immediate-draw boundary: an Entry mutation later in the same frame updates
future draws but cannot rewrite a command that has already been queued. A
focused regression asserts the resource-entry/scene pointer identity, live
visibility of an x-position mutation, stability of the older queued command
and a distinct updated snapshot for the next draw.

Two alternating stripped-release pairs each execute 30,000 complete
Chapter01 L50 update/physics/draw frames. The preceding direct-Atlas build
reports 5.47/5.42/0.04-0.05 seconds real/user/system in both runs; the live
CompoSprite-owner build reports 5.18-5.20/5.14-5.15/0.04-0.05 seconds. This is
approximately a 5.1 percent real-time and 5.0 percent user-CPU reduction in
the dense deterministic route, primarily from removing repeated composite
part-vector reconstruction. It is a workload-specific diagnostic rather than
a universal frame-rate guarantee.

The complete workspace passes 675 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, doctests,
strict all-target/all-feature Clippy and the locked release build are clean.
The direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to the
established translucent-water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
A visually inspected copied-AppData 1,200-frame island-map run completes with
zero invoked fallbacks, zero remaining compatibility bindings and empty
stderr; its PNG SHA-256 is
`b1136cedded9c672f4a5818f0b4216783cba37e50fd822adc67f23fe1dcb183b`.
The release `stella-app` and `stella-headless` hashes are respectively
`128576b06b6cf8581c71c98e6800c084462ee8fccf5c5996744b7ef773194e06`
and `140b152239fb110f4adb2d7a20745fd88579124d2183a1845e912f9e5c8c9339`.

## Borrowed Lua resource names and retained direct-draw child owners

The next symbolized dense-scene sample placed avoidable host work in the
direct sprite adapters: `value_string`, UTF-8 validation and allocation were
visible below the generated Lua closures, while direct composite submission
still copied the complete part and region vectors and allocated a new region
owner for every emitted child. These copies were not needed for lifetime
safety because the active resource-stack entry and live `CompositeSpriteOwner`
already retained the data beyond the immediate Lua call.

IDA's generated STRING adapter `sub_1005285CC` calls the exact-tag checker
`sub_1005281F8(a1, a2, 4)` and then tail-branches to `sub_100508E38` with the
Lua state, slot index and a null length output. `sub_100508E38` locates the
`TValue`, verifies tag four, loads the `TString*`, and returns `TString+0x18`;
it neither constructs nor copies a C++ string. The corresponding IDA
instructions at `0x1005285CC..0x1005285FC` show the tag check followed by that
tail branch. Hopper independently recovers the same call, `LDR X0,
[X20,#0x18]`, zero length argument and branch to `sub_100508e38`. Together
with the previously recovered active-entry `+0x10` concrete sprite load and
live CompoSprite owner, this establishes a borrowed lookup key followed by
retained resource pointers, not per-call owned names and child arrays.

The strict direct-draw, `isCompoSprite`, `getSpriteBounds` and
`getSpritePivot` adapters now borrow the exact Lua STRING payload for their
synchronous lookup. Each concrete `SpriteResourceEntry` retains one shared
old-ABI-style COW label, and ordinary direct atlas commands reuse that label
and the existing atlas owner. A diagnostic `name#instance` still strips the
suffix only for resource lookup and retains the complete submitted label on
the deferred command. Each bound composite child likewise stores a shared
label and shared atlas owner. Direct composite and shader draws now take one
immutable snapshot from the live owner and clone those pointers into commands
instead of duplicating both vectors and constructing an owner for every
child. Focused shadow/release regressions prove pointer identity between the
active entry and atlas command, and between the live composite child and its
queued command, while preserving the prior frozen-resource lifetime.

Three alternating stripped-release runs each execute 200,000 deterministic
frames with 20 hot iterations per frame. The ordinary iteration contains one
direct atlas draw plus bounds, pivot and composite-type queries. The preceding
live-CompoSprite build reports median real/user/system times of
3.58/3.54/0.03 seconds; the borrowed/shared-owner build reports
2.95/2.91/0.03 seconds, reductions of approximately 17.6 and 17.8 percent in
wall time and user CPU. A separate four-million-call direct composite run
drops from median 2.06/2.02/0.02 seconds to 1.33/1.30/0.02 seconds, reductions
of approximately 35.4 and 35.6 percent. These deliberately concentrated
adapter diagnostics quantify the removed copies; they are not universal
frame-rate claims.

The complete workspace passes 675 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, strict
all-target/all-feature Clippy and the locked release build are clean. The
direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to the
established translucent-water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
A visually inspected copied-AppData 1,200-frame island-map run completes with
zero invoked fallbacks, zero remaining compatibility bindings and empty
stderr; its PNG SHA-256 is
`a8bbc33cf2a94c829cb3ad6834420ceaece6165d111b5399d26a69bdced22dd3`.
The release `stella-app` and `stella-headless` hashes are respectively
`d94cadc3c4b659147fd97c7e103ec09801adea7996a8416e4f16950a5b7a4724`
and `b044d254f865209b956fb659c13e1b5de2dc526c2f9ad481ec5eec1520898c36`.

## COW adapter strings retained by the native scene index

A symbolized 20,000-frame island-map sample next placed avoidable host work
under the `changeZOrder` Lua closure. The previous Rust adapter converted the
Lua name to an owned `String`; `NativeSceneRenderIndex::move_z` then called
`to_owned`, and conversion of that second allocation to `Arc<str>` copied the
payload once more. This was not the ownership pattern used by Purple's old
libstdc++ ABI.

The registration sequence at `0x10002EEC4..0x10002EEF4` associates
`changeZOrder` with member `sub_1000592C4` and generated STRING/NUMBER adapter
`sub_100086690`. IDA shows that the latter unpacks the member pointer through
`sub_100529B50` and enters `sub_1000866F8`. That adapter reads the exact Lua
STRING through `sub_1005285CC`, creates one COW `std::string`, reads slot two
as a float, and copy-constructs only the eight-byte COW handle for the member
call. `sub_1000592C4` resolves the RenderObject, removes its name from the old
integer-z/sheet vector, copy-constructs the same COW handle into the new
vector, writes the reflected `z_order` attribute, and finally stores the
float at RenderObjectData `+0xD4`. Hopper independently recovers the same
registration pointers, `std::string` copy construction, vector erase/insert,
attribute write and final field store.

The related `native_setSprite` adapter `sub_100089B74` creates exactly one COW
owner for each of its two exact STRING arguments. IDA's `sub_10004C7FC` and
Hopper's pseudocode both show the object-name handle moving between the
native scene-index leaves and the sprite-name handle being retained by
`std::string::assign` at RenderObjectData `+0x68`. The resource pointer and
sheet index are updated before that final assignment; neither retained label
requires another character-buffer allocation.

Rust now constructs one `Arc<str>` owner for each generated-adapter string and
passes clones of that pointer through the same scene-index and object-field
boundaries. `move_z` and `move_sheet` consume the supplied owner instead of
rebuilding it. Lookups, missing-object errors and Lua `z_order` reflection use
the same retained owner, preserving strict tags, failure order and float32
rounding. A focused regression moves one name across both z and sheet leaves
and proves with `Arc::ptr_eq` that the supplied COW-style owner is the value
retained in each destination vector.

A concentrated stripped-release diagnostic creates one non-physics object and
executes 500,000 alternating `changeZOrder` calls. Three preceding-build runs
report median real/user times of 0.23/0.20 seconds. After the ownership fix,
four warm runs report medians of 0.20/0.18 seconds, reductions of about 13.0
percent wall time and 10.0 percent user CPU in this adapter-heavy workload.
The deliberately concentrated result measures removed string copies and is
not a universal frame-rate claim.

The complete workspace passes 676 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, doctests,
strict all-target Clippy and the locked release build are clean. The direct
release-wgpu Chapter02 L16 checkpoint remains byte-identical to the established
translucent-water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`.
A visually inspected copied-AppData 1,200-frame island-map run completes with
zero invoked fallbacks, zero remaining compatibility bindings and empty
stderr; its PNG SHA-256 is
`1853b677d8c354a2cca6766e9a1505de83bf05017da2b0b1d5804124a9252d89`.
The release `stella-app` and `stella-headless` hashes are respectively
`6e1483cc508eef543122e2905f46ea1f2c66c4c383578bc587b19bb1e725a3f8`
and `06289b09b4c1ae975741ac93067993d49cdb3df27f0c121b9558f98b02887b7f`.

## Generated C-string boundaries and allocation-free Lua name borrows

A fresh symbolized profile separated startup work from steady scene work. The
startup-inclusive sample correctly placed MP3 open/decode paths near the top,
but none of those frames remained in a second five-second sample taken after a
ten-second warm-up. In that steady sample, `value_string` and the native scene
walker were the leading Rust frames. Inspecting mlua's safe `BorrowedStr` path
showed that even a temporary string view clones its `ValueRef`; the first clone
of a unique reference lazily allocates a shared counter. Purple's generated
adapters do not have that host-only retain/allocation boundary.

IDA's `drawSpriteWithoutShader` adapter `sub_100084398` reads slot one through
the exact STRING checker `sub_1005285CC` at `0x1000843E0`, calls `strlen` at
`0x1000843EC`, and assigns exactly that byte count to its old-ABI
`std::string` at `0x1000843FC`. It then reads the five exact NUMBER slots and
dispatches the member. The independent `setVisible` adapter
`sub_1000859F4` has the same sequence at `0x100085A30`, `0x100085A3C` and
`0x100085A4C`, followed by the exact BOOLEAN reader. Hopper independently
recovers both STRING-pointer, `strlen`, `std::string::assign` sequences and
the same numeric/boolean argument order. This corrects an observable ABI edge:
embedded NUL terminates every generated string, whereas the preceding Rust
conversion retained the complete Lua byte string.

The central strict adapter now obtains mlua's stable Lua string pointer, views
it through the same C-string boundary, validates only the prefix consumed by
Purple, and ties the returned Rust lifetime to the owning `MultiValue`. Lua
strings are immutable and non-moving, and the argument vector retains the
registry reference for that complete lifetime. Members that retain a name
still create one `String` or `Arc<str>` owner at the native COW ownership
boundary. Synchronous atlas/composite/resource lookups use the view directly,
so they neither fabricate an owned payload nor allocate mlua's shared
reference counter. Focused regressions verify both scene-object and direct
sprite lookup with an embedded NUL, including a non-UTF-8 suffix that Purple's
preceding `strlen` never observes.

Five stripped-release runs create one non-physics object and execute three
million `setVisible` calls. Ignoring each build's first cold-start outlier, the
preceding build reports about 0.47/0.44 seconds real/user time; the corrected
adapter reports about 0.38-0.39/0.35-0.36 seconds, an approximately 17-18
percent reduction in this deliberately concentrated string-adapter workload.
This quantifies the removed host allocation and is not a universal frame-rate
claim.

The complete workspace passes 676 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation, strict
all-target Clippy and the locked release build are clean. The final direct
release-wgpu Chapter02 L16 checkpoint remains byte-identical to the established
translucent-water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`;
it contains the complete pool and submerged structure without an editor cross.
An isolated copied-AppData 1,200-frame island-map run has empty stderr, zero
invoked fallbacks and zero remaining compatibility bindings; its visually
checked PNG SHA-256 is
`e2188201f63a5654237ee0be4ec56184bf460f4a077ae5231094b580d7839621`.
The release `stella-app` and `stella-headless` hashes are respectively
`71b922b2a7e51462aa7c827630178123b123d4f41168984d12e77f44a9c56631`
and `ed5d143aac993ea8e9b11c3d025010d8cfb31470a6b341661565ae2fed089e60`.

## Inline Box2D polygon contact scratch storage

A new steady-state symbol sample of a fully constructed Chapter01 L50 scene
placed the next avoidable host work in polygon contact creation. The preceding
Rust path allocated a `Vec` for each fixture's transformed vertices, then more
`Vec`s for float32 point copies and per-face normals inside every maximum-
separation or edge-polygon query. These allocations were an artifact of the
rehost data model; Purple's bundled Box2D shape already owns fixed vertex and
normal arrays.

IDA's `b2FindMaxSeparation` `sub_10085FB84` reads the signed face count from
polygon-shape offset `+0x98` at `0x10085FBB8`, starts its direct normal walk at
`shape+0x58` through the pointer formed at `0x10085FC34`, and advances exactly
eight bytes per face. It calls only the direct `b2EdgeSeparation` leaf
`sub_10085FD74`; neither function has an allocator call. Hopper independently
recovers the same `+0x98` count, embedded normal-array walk and sole callee.
The full polygon entry `sub_10085F648` and edge-polygon collider
`sub_10085EADC` likewise operate on the supplied shapes and stack records
without constructing a dynamic vertex container. This agrees with the
previously recovered `sub_100871F08` decomposition limit of eight convex
points.

Rust now gives contact scratch polygons the same eight-point inline capacity.
Fixture world transforms, float32 centroid inputs, computed face normals and
the edge-polygon f32/f64 boundary use `SmallVec<[T; 8]>`; malformed diagnostic
input can still spill safely, while every shape admitted by Purple's authored
decomposition stays on the stack. The edge collider also computes signed area
directly over the already-narrowed float32 points instead of allocating a
round-trip f64 vector. The whole-bundle level regression now inspects every
constructed polygon fixture in all 149 shipped levels and proves that none
exceeds the native capacity.

Five alternating, identically symbolized release pairs each execute 30,000
complete Chapter01 L50 update/physics/draw frames. The preceding build's
median real/user times are 10.39/10.33 seconds; the inline-polygon build's are
10.32/10.22 seconds, reductions of approximately 0.7 and 1.1 percent in this
physics-heavy diagnostic. Two new-build wall-time outliers do not affect the
more stable user-CPU comparison. A follow-up five-second symbol sample still
shows polygon computation itself, as expected, but no allocator appears below
`collision_fixture_geometry` or `polygon_normals`.

The complete workspace passes 677 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation tests,
strict all-target/all-feature Clippy and the locked release build are clean.
The direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to the
established translucent-water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`;
visual inspection confirms the complete pool and submerged structure. An
isolated copied-AppData 1,200-frame island-map run has empty stderr, zero
invoked fallbacks and zero remaining compatibility bindings; its visually
checked PNG SHA-256 is
`bfe0d993856684e98e22556a96b9da13c7259d680aafda3f2c7b4516ee6e7425`.
The release `stella-app` and `stella-headless` hashes are respectively
`4af6d5b67b3274047e1a1229332d273da309c44e397bbd049054a42a8be8452f`
and `7a9ce9e19458f3b9d4aacc7b26fcbc9703a32b34d1488a5b24d53481ec10478c`.

## Persistent Box2D body-edge topology during island assembly

The follow-up steady L50 profile placed the next avoidable ownership cost in
`assemble_box2d_islands`. The preceding bridge retained the world contact and
joint records, but reconstructed two `BTreeMap<&str, Vec<usize>>` adjacency
graphs on every fixed step and used endpoint names as the island visited
identity. That work is not present in Purple's Box2D world.

IDA's `b2World::Solve` at `sub_10086E634` clears the body island bit while
following `b2Body+0x68` at `0x10086E6F4..0x10086E704`, clears contact flags
through `b2Contact+0x18` at `0x10086E71C..0x10086E72C`, and clears joint
flags through `b2Joint+0x18` at `0x10086E744..0x10086E74C`. It allocates only
the `bodyCount * 8` pointer DFS stack at `0x10086E750..0x10086E760`. The DFS
then reads the body's persistent contact-edge head at `+0x90`, advances each
edge through `+0x18`, reads the persistent joint-edge head at `+0x80`, and
again advances through `+0x18` at `0x10086E828..0x10086E8F8`. Static bodies
terminate traversal before the optional controller list at `+0x88`.
Hopper independently recovers the same three intrusive list clears, the one
pointer-stack allocation, both body edge heads and both `edge+0x18` walks.

The Rust world now retains contact and physical-joint allocation tokens on
both endpoint bodies from creation until destruction. Each per-body vector is
append-only in creation order while live, so reverse iteration exactly models
Box2D's head insertion. Contact and joint replacement, fixture/body teardown,
deferred breakable-joint destruction and complete level clearing unlink both
endpoints at the native lifecycle boundaries. Island traversal uses the
stable body allocation token for visited/static-island identity and borrows
the endpoint name only for the existing scene lookup. It therefore no longer
rebuilds or sorts an adjacency graph on every solve. Focused regressions cover
the retained contact endpoints, two-joint chain edge order, island wake order
and reverse frame-tail joint unlinking.

Five alternating stripped-release pairs each execute 30,000 complete
Chapter01 L50 update/physics/draw frames. The preceding build reports median
real/user times of 11.80/11.46 seconds; the persistent-edge build reports
11.45/11.08 seconds, reductions of approximately 3.0 and 3.3 percent. The
first new-build wall-time outlier is excluded naturally by the median. A
follow-up symbol sample no longer has a standalone
`assemble_box2d_islands` frame or the per-step adjacency-map entry allocation
below it; LLVM folds the smaller persistent-edge walk into the discrete island
solver.

The complete workspace passes 677 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, documentation tests,
strict all-target/all-feature Clippy and the locked release build are clean.
The direct release-wgpu Chapter02 L16 checkpoint remains byte-identical to the
established translucent-water baseline, SHA-256
`63b5da87b1a55a57d0e5d3559336f604d35817b5651cf8cf7347f7def189d4d7`;
visual inspection confirms the complete pool and submerged structure. A
fresh copied-AppData 1,200-frame island-map run has zero invoked fallbacks,
zero remaining compatibility bindings and a visually checked PNG SHA-256 of
`324f959b1eebaaf0192cacfd8d31ecab57c8e9eb93573245e5357fee9ba59e44`.
The release `stella-app` and `stella-headless` hashes are respectively
`d0184ef3b8ef182dacea5bc681dc88ac8aae79f7fbee59260cf62285fc64cde4`
and `beb417c969d76675e3eb0c75ffe176015e204f673041028f315c74ded2fe1128`.

## Native unit angular damping and Chapter02 L56 vehicle settling

The upper-right Chapter02 L56 vehicle is authored as chassis
`BLOCK_ROCK_1X10_1_6`, wheels `BLOCK_WOOD_ROUND_4X4_1_11` and `_12`, and pig
`pig_medium_8`. Its two type-three revolute joints and the three-piece static
platform match the shipped level data. With the preceding Rust constructor
default of zero angular damping, both wheels remained above Box2D's angular
sleep tolerance: after 600 display updates the left wheel had reached the
platform edge with angular velocity about `-0.145`, so the vehicle eventually
fell.

IDA shows that this zero was not a native default. The circle constructor
`sub_100034FB0` builds its `b2BodyDef` at `var_128`; the sequence at
`0x100035088..0x1000350E0` writes `1.0f` to structure offset `+0x20`, before
the `CreateBody` call `sub_10086DF90` at `0x1000350E0`. The box constructor
`sub_100034740` has the same layout: linear damping at `+0x1c` is zero,
angular damping at `+0x20` is `1.0f`, and gravity scale at `+0x38` is
`1.0f`. The polygon and line constructor paths initialize the same definition.
Hopper independently recovers the circle store to `var_108` and the following
`CreateBody` call. Finally, `b2Island::Solve` `sub_10086CE84` reads body
offsets `+0xa8/+0xac` and multiplies linear/angular velocity by
`clamp(1 - dt * damping, 0, 1)` before its recovered half-second sleep test.

All native physics constructors now start with unit angular damping; the
non-physics render constructor retains its inert zero. The real shipped L56
regression loads and starts the complete level, requires the vehicle to roll
instead of starting asleep, waits for the chassis and both wheels to enter
Box2D sleep, and then advances another 600 frames. The chassis moves about
`0.344` world units, settles at `x=12.716`, and remains unchanged with both
wheels and the pig still present. This reproduces the original brief roll and
stop without a level-specific constraint or coordinate override.

The complete workspace passes 678 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target/all-
feature Clippy and the locked release build are clean. Direct release-wgpu
checkpoints at 60 and 1,800 display frames have SHA-256 values
`3a804b15be45db786705b56bae55336ef44b1c2b46e06341a672255e9a4f5a8c` and
`398357769f817a2ccae7f557b67fee4beb61188a25e34d78f9c87af1b50c16ec`.
Visual inspection shows the authored initial roll and the complete vehicle
resting on the upper-right platform at the later checkpoint.

## Telepods capability publication before the original UI snapshot

The native QR and wallet members were complete, but the desktop command line
originally advertised its virtual scanner only after `game.lua` returned.
Decompiling the shipped `characters.lua` exposes why that made every ordinary
game entry disappear: when `telepod_configuration.json` is accepted it stores
`g_showTelepodButtons = showTelepodButtons and (not releaseBuild or
Telepods.areSupported())` exactly once. `Scrapbook:onEntry`, `GameHud:onEntry`
and the dynamically constructed `ExtraBirdBar` then consume that stored flag;
they do not poll the camera predicate again.

IDA places the QrScanner constructor thunk `sub_1000DE2A4` inside the large
native service-graph constructor `sub_10002C274`. Its sole call site is
`0x10002FAA8`, where the completed QrScanner is immediately retained at owner
offset `+0x648` (`1608`). This is the same construction phase that publishes
the five-member `QrScanner` Lua object, before the application boots its game
scripts. Hopper independently reports `sub_10002C274` as the thunk's only
caller and recovers the same object construction and publication sequence.
The correct boundary is therefore capability publication before script boot,
not mutation after the first menu has already been built.

The desktop application now announces its virtual scanner immediately after
constructing `StellaLua` and before booting `scripts/game.lua`. An optional
`--telepod-code` is queued at that same point and remains pending until the
shipped `TelepodPage` calls `start` and installs its recognition callback. The
headless executable preserves its default no-camera branch, but when a code is
requested it uses the same preboot ordering. No replacement menu was added:
the visible UI remains the shipped `TelepodPage.layout.lua`, including both
scan animations, electric logo, placement text, help and close buttons, while
the original Scrapbook, in-level, last-chance and reward-wheel callers retain
their own input and animation behavior.

A complete-data regression enables the scanner before boot, proves
`g_showTelepodButtons` is true, constructs all seven shipped scan-page children
and advances the original page lifecycle for 180 frames. It requires both the
electric Telepods logo and `TELEPOD_SCAN_*` animation sprites to reach the
native render queue. A deterministic wgpu screenshot of the same page also
confirms that the original board, text, logo and animated placement artwork are
present rather than a host-side substitute; the settled release readback has
SHA-256 `fc0302df52fdfc2dc1b5d3825e721b01659ba228a603d345c2c0da714ee7d2a0`.

The complete workspace passes 682 tests with the intentional long-duration
BirdRun audit ignored. Formatting, diff whitespace, strict all-target/all-
feature Clippy and the locked release build are clean. A 600-frame release
headless boot with a prequeued Telepod code proves the original capability
snapshot, Telepods facade and initialized wallet remain live with zero invoked
fallbacks and zero remaining compatibility bindings. The release `stella-app`
and `stella-headless` hashes are respectively
`7e0e3ac33414ed9b812a0795b241558ead9e7f1b50a1e237d10e95d4caf8962e` and
`278a278bb10de1d68fd8acb64eec85dd722f9f64bad3ae6ad44d7b77abd2f4af`.

## Live-game Telepods delivery coverage

The functional-completeness pass now follows the original Telepods route past
the scan-page lifecycle and wallet callbacks. A first artificial probe opened
`TelepodPage` in the same host call that constructed Chapter02 L11. That is not
a valid game route: the original `GameHud` and its extra-bird listeners have
not completed their entry lifecycle yet, so it can prove skin delivery but not
the later in-level bird animation.

The permanent regression instead starts the complete shipped level and block
component graph, advances sixty display frames, and only then opens the
original `TelepodPage` with a prequeued `hasbro.telepod.020` scanner payload.
It observes the unmodified Lua chain
`onQRRecognized -> IAP.redeemCode -> native_fetchWallet -> onPurchaseDone ->
EID_WILL_ADD_EXTRA_BIRD -> EID_TELEPOD_BIRD_ADD_FINISHED`. The scan unlocks
`Piano Willow`, publishes that exact bird name on the completion event and
increments the live `birdsCounter` by one. This closes the distinction between
merely showing the recovered Telepods UI and actually delivering its bird into
an active level.

The complete workspace now contains 683 tests with the intentional
long-duration BirdRun audit ignored. The functional priority remains on
unexercised menu/gameplay/service routes; pure performance work is deferred
until those routes are complete.

## Resource URL and App Store host dispatch

The next platform audit found an actual host-consumption gap rather than a
Lua binding gap. IDA resolves the `openURL` registration at `0x100446D98` to
`game::LuaResources::openURL` (`sub_10044AAF8`). That member constructs the
iOS platform adapter, forwards the required string to `sub_10053CB90`, returns
its boolean result and destroys the adapter. Decompiling `sub_10053CB90`
recovers the complete Objective-C chain `UIApplication.sharedApplication ->
NSURL.URLWithString -> UIApplication.openURL:`. Hopper independently recovers
the same wrapper/callee sequence. The earlier Rust member validated the string
but always returned false, so every original EULA, privacy-policy, Telepods
help and redirect path was suppressed.

The StoreKit side has an equally explicit boundary. IDA and Hopper both
recover `ForceUpdate::native_launchAppStore` as `sub_100026238`, which builds
the literal product id `875251011` and calls `sub_1005340D0` with type `3`.
The cross-promotion launcher's non-installed branch uses that same product/type
service, while its installed branch opens the cached launch URL directly.

Rust now emits a public, ordered `PlatformActionRequest` stream. `res.openURL`
accepts and returns true after preserving its strict string contract;
ForceUpdate and cross-promotion retain their exact product ids and type values.
The interactive desktop host drains the stream at the next native frame and
uses `/usr/bin/open`, `rundll32` or `xdg-open` on macOS, Windows or Linux. A
platform-launch failure remains non-fatal, matching the advisory native API.
`native_startURLThread` is intentionally not routed into this stream: its
recovered member `sub_100032688` owns an asynchronous network callback object,
not a system-browser launch. `playVideo` also has no shipped Lua caller and the
original application bundle contains no movie resource, so this change does
not invent an external-player behavior for that unreachable service.

A regression invokes `res.openURL` and ForceUpdate from the Lua VM, checks the
strict argument failures and return ABI, and proves that URL then product
requests reach the host in native call order without being coalesced.

## Asynchronous URL response worker and main-thread event handoff

`native_startURLThread` was registered but previously stopped after retaining
the requested URL and a Lua callback that no later code could reach. IDA
recovers the adapter at `sub_100032688`: argument one is an exact generated-
adapter string, argument two is a Lua function, and argument three is read as
a boolean only when the Lua stack count is exactly three. The callback wrapper
replaces `GameLua+0x50`; a callable containing the `GameLua*` and URL is then
installed in a newly constructed 0x30-byte thread object at `GameLua+0x660`.
`sub_100586430` starts that worker with `pthread_create`, and Hopper recovers
the same callback, owner, URL and optional-thread-policy layout.

The worker body `sub_10006DDAC` constructs `net::HttpFileInputStream` through
`sub_10052C7F4`, copies the entire stream to memory and posts the original URL
plus the binary response body to event `dword_100C2A028`. The HTTP stream
constructor accepts only status 200; any other status throws before a response
event can be made. A negative stream length also skips the event. Both IDA and
Hopper recover the constructor's exact `status == 0xC8` comparison and the
two-string event payload. GameLua's constructor binds that event to
`sub_10005BD5C`, which pushes the saved function followed by URL and response
strings, then performs a protected Lua call with exactly two arguments and no
results.

The event is deliberately not invoked on the network thread. The template
instantiation at `0x100074654` copies both strings into a zero-delay functor and
queues it through `sub_10057C1BC` under the process-global scheduler mutex.
`sub_10057C418` moves and executes those queued functors, and its application
call site at `-[AppController update]+0x264` (`0x1004045D8`) precedes the app
update virtual at `0x1004045FC`. This establishes the observable boundary:
successful URL callbacks run on the next scheduler frame before GameLua's
ordinary update callback.

The Rust host now mirrors that structure with a named one-slot Lua callback, a
pure-Rust HTTP/HTTPS worker thread, a binary-safe completion queue and a queue
drain at the head of `StellaLua::update`. It requires final status 200, reads
the response without a size or UTF-8 conversion boundary, preserves the
original `callback(url, responseBody)` ABI, and produces no callback on request
or stream failure. A loopback regression returns a response containing both a
NUL and an invalid UTF-8 byte, proves the body survives byte-for-byte, proves
the callback precedes that frame's Lua update and covers the exact optional-
boolean stack-count rule.

The complete workspace now passes 687 tests with the intentional long-duration
BirdRun audit ignored. Strict all-target/all-feature Clippy, formatting and
locked dependency checks are clean; CI also runs the two synthetic URL-thread
regressions without requiring the separately distributed game data.

## Installed-application HTTP worker and frame-tail delivery

The adjacent `checkInstalledAppsOnline` binding still validated its URL and
returned zero values without executing the native service. IDA resolves its
generated one-string adapter `sub_100089E6C` to member `sub_1000505D8`. That
member copies the URL into `GameLua+0xA8`, constructs a 0x30-byte worker at
`+0xC0`, and starts callable `sub_10006DFD4`. The callable constructs the same
`net::HttpFileInputStream` used by the generic URL worker, copies the complete
response into the `std::string` at `GameLua+0xB0`, then stores byte one at
`+0xB8`. Hopper independently recovers the shared URL field, worker owner and
response/flag stores. Unlike `native_startURLThread`, this path posts no event
to the process-global scheduler.

The completion byte is consumed inside the monolithic GameLua frame rather
than at the frame head. IDA decompiles the branch at
`0x10005FD58..0x100060588`: it follows body/joint export and the three rolling-
material audio loops, parses the raw response, and calls
`setInstalledApps(installedNames, ttl, rawResponse)` immediately before Lua's
ordinary `update(scaledDelta, rawDelta)`. Assembly from Hopper confirms the
`LDRB [GameLua,#0xB8]`, response address `+0xB0`, integer address `+0xC8`,
three-argument Lua call at `0x10005FE78`, and the final `STRB WZR` at
`0x100060588`. Because that clear occurs after both parsing and the Lua call,
a malformed response or callback error leaves the completion visible on the
next frame.

The shared parser is `sub_100060CBC`. It requires an object document, reads
integer `ttl` into `GameLua+0xC8`, reads integer `gameCount`, and validates
each non-empty `game_N` object plus its string `name` and `scheme`. Negative
or zero counts simply skip the loop. On iOS it tests every authored
`scheme://` through the platform application adapter and comma-joins the names
whose schemes are installed; the cross-platform desktop host has no iOS
application registry and therefore returns the native empty list while still
validating the complete response. `checkInstalledAppsOffline` uses this same
parser but retains its distinct one-argument `setInstalledAppsOffline(names)`
callback.

Rust now mirrors the online member with a named worker, the shared exact-200
HTTP reader, one overwriteable response slot and a completion check at the
recovered frame-tail position. The raw JSON remains byte-preserving when
passed back to Lua. Loopback regressions prove zero-result/asynchronous
behavior, the `(names, ttl, rawResponse)` values, delivery before ordinary Lua
update, one-shot clearing after success, and repeated failure when malformed
JSON prevents the native completion flag from clearing. CI runs these together
with the binary-safe generic URL-worker regressions without needing game data.

## Game-server native transport and retained facade split

The native `GameServerConnection` constructor at `sub_1000D25D8` registers
exactly `_G.GameServerConnection.native_getAsync` and `native_postAsync`, stores
`https://stella-stage.appspot.com/api/v1` at owner offset `+0x28`, and loads
`scripts_common/network/GameServerConnection.lua`. Its generated adapters at
`sub_1000D8310` and `sub_1000D7F1C` recover the exact argument contracts: GET
takes a numeric message ID and string route; POST takes a numeric message ID,
string route, boolean encryption flag, string seed and Lua table. The numeric
ID follows the original float narrowing and ARM `FCVTZS` conversion rather
than an ordinary Rust integer cast.

The GET and POST builders at `sub_1000D3FC0` and `sub_1000D44CC` append the
route to that base URL, apply the native 30-second timeout and send the current
`g_currentLocale` through `Accept-Language`. GET uses method zero. POST uses
method one, adds `Content-Type: application/json`, and first serializes its Lua
table through the recovered util::JSON path. Serialization failure is caught
by the original adapter and submits no request.

Encrypted POSTs take at most the first eight seed bytes, append the recovered
literal `RAOzTXzh`, and pass the resulting 16-byte key through the AES code at
`0x100557E5C..0x100558684`. The implementation is AES-128-CBC with an all-zero
IV and PKCS#7 padding. `sub_100559560` then uses the alphabet initialized by
`InitFunc_321`, `ABCDEFGHIJKLMNOPQRSTUVWXYZ234567`, producing unpadded RFC 4648
base32 inside compact JSON field `data`. The regression vector for seed
`12345678` and payload `{"a":"x","z":2}` is
`{"data":"ATS5BS2VEJJELN5ULPWIRRDPC4"}`.

Both request closures converge on `sub_1000D4A64`. Transport status `-1`
invokes `onAsyncRequestTimedOut(messageId, -1)`. Every other status invokes
`onAsyncRequestCompleted(messageId, status, responseTable)`; only status 200
attempts JSON parsing, while non-200 or malformed-success bodies preserve the
initial empty table. The shipped chunk creates its public high-level
`GameServerConnection` inside the retained GameLua environment, but attaches
these completion callbacks to the original root table. Purple's native
LuaObject therefore retains `_G.GameServerConnection`; replacing or dispatching
through the local facade would be observably wrong.

The Rust host now preserves that split, loads the shipped facade at the
constructor-equivalent bootstrap point, executes HTTP work off-thread and
drains callbacks on the application thread before the ordinary Lua update.
The shipped 1.1.6 facade still deliberately asserts `GAMESERVER-DISABLED`, so
the later offline challenge facade remains in place for playable local replay
while the complete native ABI and transport stay available. Loopback tests
cover GET/POST method, path and headers, JSON canonicalization, status and
timeout callback arity, malformed 200 handling, strict generated-adapter types,
the encryption vector and the root-versus-local retained-table identity.

The complete workspace now passes 696 tests with the intentional long-duration
BirdRun audit ignored. Strict all-target/all-feature Clippy, formatting, diff
validation and the locked release build are clean. The release headless host
also boots the shipped data and advances 120 update/draw frames with zero
invoked fallbacks and zero remaining compatibility bindings.

## Game Center completion events and shipped subsystem translation

The earlier `FusionGamerServices` table covered its generated direct-call ABI,
but discarded all three asynchronous GameKit result paths. IDA and Hopper both
recover the constructor at `sub_1000C9D70`: after publishing the seven native
members it subscribes to the process GamerService authentication-status event.
The retained completions enter `sub_1000CAA94`, `sub_1000CA884` and
`sub_1000CB010`, respectively. Each constructs a Lua table and calls the
retained GameLua member `notifyEventManager` through `sub_100065D18` with
exactly two arguments rather than directly addressing the script event-manager
object.

The achievement table contains `achievementId` and `success` and is published
as `EID_GS_POST_ACHIEVEMENT_FINISHED`. The score table contains
`leaderboardId` and `success` and uses `EID_GS_POST_SCORE_FINISHED`. The
authentication table contains `isSignedIn`, calculated as status exactly equal
to one, and uses `EID_GS_AUTHENTICATION_STATUS_CHANGED`. The generated
achievement adapter `sub_1000CC2F4` still requires only an exact string in slot
one; score adapter `sub_1000CC0C4` requires an exact string followed by an
exact number and narrows that score to the native float member boundary. Both
ignore trailing Lua arguments and return zero results.

`sub_1003BF240` submits an achievement at 100 percent and `sub_1003BF5A8`
submits the authored score. Their Game Center completion blocks at
`sub_10054CC94` and `sub_10054D164` report true for a nil error and, because of
their legacy compatibility test, also for errors on every iOS version at least
5.0. Stella 1.1.6 cannot run on an older target, so the portable no-GameKit
completion surrogate correctly reports true. The authenticate handler is
installed asynchronously; without a platform GameKit account its initial
portable state is the recovered signed-out value.

The Rust owner now retains a FIFO completion queue alongside GameServer state.
It keeps the initial authentication callback pending until the shipped
`notifyEventManager` exists, then drains authentication, achievement and score
events on the application thread before ordinary Lua update. Strict synthetic
tests prove asynchronous/one-shot delivery, exact field and event names,
ordering, the pre-update boundary and the early-bootstrap pending state. A
shipped-data integration test also lets `scripts_common/subsystems/
FusionGamerServices.lua` consume the low-level achievement completion and
observes its translated `EID_GAMERSERVICES_ACHIEVEMENT_POSTED` event.

The complete workspace now passes 699 tests with the intentional long-duration
BirdRun audit ignored. Repository-authored source remains explicitly licensed
under `AGPL-3.0-or-later`; the full GNU Affero GPL v3 text, Cargo SPDX metadata
and README notice agree, while Rovio game data remains outside that grant.

## Assets request scheduling and retained callback ownership

The previous local-cache surrogate reproduced the values returned by
`Assets.loadFiles`, but invoked its Lua callback before the native member
returned. Both disassemblers show that this timing is impossible in Purple.
`sub_1000AC25C` first copies every exact-string table value and constructs two
completion functors retaining the native Assets LuaObject. It then submits a
0x90-byte `lang::Func5` request job through `sub_1006EC360 -> sub_1005865AC`
and starts it through `sub_100586644`; neither completion is called from the
submission stack.

The success functor at `sub_1000AC964` constructs the request-to-filename map
and calls retained member `onLoadSuccess` with exactly that one table. Failure
functor `sub_1000ACA0C` builds a dense one-based failed-filename array and calls
`onLoadError(array, ErrorCode, message)`. The owner is the native root object
created by `sub_1000AC118`, not the separate facade later installed into the
GameLua environment. This matches the shipped `Assets.lua`, whose `_G.Assets`
assignments publish callbacks onto the retained native table while its local
table owns `getAssetFilename` and `haveBeenDownloaded`.

Rust now retains an Assets runtime with a FIFO completion queue and the
successfully resolved filename map. `loadFiles` only validates, resolves and
queues; frame-head dispatch updates the completed map and calls the root
native callback before ordinary Lua update. Dispatch takes a snapshot of the
pending queue, so a callback that issues another load cannot complete
recursively in the same frame and instead observes a fresh asynchronous
boundary. Regression coverage proves zero-result submission, no callback on
the calling stack, success/error FIFO order, pre-update delivery, retained
root callback lookup and next-frame deferral of a nested request. The complete
workspace remains at 699 passing tests with the intentional long-duration
BirdRun audit ignored.

## Rovio Channel asynchronous open and cancellation state

The first Channel reconstruction retained the seven-member ABI and guaranteed
that a retired endpoint could not leave its connection overlay stuck, but it
called `onChannelLoadingFailed` from inside `openChannelView`. IDA and Hopper
both show a different scheduling boundary. `sub_1000AD7D8` forwards the seven
decoded arguments to `sub_1005DF1D4`; when the SDK is not prepared,
`sub_1005DF2A0` copies them into a 0x48-byte callable, wraps that callable with
`sub_1005865AC` and starts the job through `sub_100586644`. The retained SDK
listener later reaches the no-argument Lua callback through
`sub_1000AE41C -> sub_1005278E8`.

Cancellation is not an unconditional sink. Native member `sub_1000ADB70`
delegates to `sub_1005E0C04`, which acts only while SDK state equals loading,
releases the outstanding request pointer, changes the state to cancelled and
does not emit the loading-failed callback. This also explains why simply
queuing an inevitable failure without a cancellation handle is observably
wrong for the shipped connection screen.

The portable Channel owner now retains its pending retired-request state.
Opening an enabled Channel validates the recovered ABI and schedules the
failure for application-thread frame-head dispatch; cancellation clears that
pending state. The dispatcher addresses the retained root `RovioChannel`
table and invokes `onChannelLoadingFailed()` before ordinary Lua update. A
shipped-data regression proves call-stack deferral, pre-update delivery,
one-shot completion and cancellation suppression while preserving the SDK's
unavailable-view state.

## IAP provider, redeem and wallet asynchronous chain

The earlier offline IAP bridge preserved the callback values and wallet
delivery order, but collapsed all provider work into the Lua call stack. The
native lifecycle is asynchronous at three separate boundaries. Initialization
at `sub_1000CE2A0` first changes manager state from zero to one, installs the
provider continuations and calls the shipped `registerPaymentCallbacks` once.
Provider success reaches `sub_1000CE5C4`: it starts wallet retrieval through
`sub_1000CEDAC`, calls the retained root `onPaymentInitialized(bundleId)`, and
only after that callback changes manager state to two.

Redeem member `sub_1000CDD5C` enters `sub_1006B3B3C -> sub_1006B3140`.
`sub_1006B3140` constructs a 0x68-byte `lang::Func4`, submits it through
`sub_1005865AC`, and starts it through `sub_100586644`; consequently neither
`CODE_OK` nor a mapped failure can occur before `native_redeemCode` returns.
Success continuation `sub_1000CF178` calls retained root member
`onRedeemResponse(code, "CODE_OK", productId)`. Failure continuation
`sub_1000CF278` supplies the two-argument `(code, status)` form and maps
provider statuses -31 through -37 and -101 to the recovered `CODE_*` names.
The deterministic retired-provider substitute continues to use -31's
`CODE_NOT_FOUND` result for an unknown local Telepod code.

Wallet retrieval has its own retained job. `sub_1000CEDAC` refuses a second
request while its processing byte is set; provider path `sub_1006AB4C8`
constructs a 0xA0-byte `lang::Func3` and submits it to the same scheduler.
Wallet processor `sub_1000CF4E4` keeps the processing byte set while calling
`deliverItem(productId)` and then
`onWalletProcessVoucher(voucherProductId, productId, source)`, clearing it only
after the batch. This guard is observable because shipped `iap.lua` calls
`native_fetchWallet` from every successful redeem callback: a burst of voucher
responses coalesces into one wallet read instead of recursively processing
each item.

Rust now models the manager's 0/1/2 initialization field, wallet-processing
guard, voucher queue and completion FIFO under one native owner. Frame-head
dispatch snapshots that FIFO before entering Lua. Initialization therefore
appears initialized only after its next-frame callback; a local redeem returns
on the following frame; the wallet request created by that response cannot be
seen until one further frame. Regression coverage exercises the shipped
`iap.lua` rather than a synthetic facade and proves all 24 configured Telepod
products, unknown-code mapping, listener transfer from code to product,
coalesced wallet delivery, callback order and the absence of same-stack or
same-snapshot completion.

## Skynest key-provider request retention and frame delivery

The retired Skynest key substitute previously invoked the callback supplied to
`native_setKey`, `native_getKey` and `native_getKeyForAccountIds` before the
native member returned. IDA recovers all seven storage registrations in
constructor `sub_1000B9B38`; Hopper independently confirms the three key
members at `sub_1000BA5B8`, `sub_1000BA714` and `sub_1000BA85C`. None of these
members calls its LuaFunction directly.

Each member increments the request counter at owner offset `+0x90`, inserts the
supplied LuaFunction into the red-black tree rooted at `+0x60` under that
request ID, constructs distinct provider success and failure functors carrying
`(owner, requestId)`, and then calls the provider interface at
`sub_100705DE8`, `sub_100706118` or `sub_100706390`. The later continuations
look the request ID up again, call the retained function and erase its tree
node. Set-key success and failure (`sub_1000BBEFC` and `sub_1000BBD90`) both
call with zero arguments. Get-key success `sub_1000BBC14` supplies one string,
whereas failure `sub_1000BBAA8` supplies none. Batch success
`sub_1000BB8B0` converts the provider string map to one Lua table; its failure
`sub_1000BB744` again supplies no arguments.

Rust now retains those callback functions through Lua registry keys in a
storage-owner FIFO. Submission validates the recovered ABI and returns zero
results without mutating the local provider state or entering Lua. Frame-head
dispatch snapshots the queue, applies successful offline writes in request
order, resolves reads against the state reached by earlier requests, calls the
exact success/failure argument shape and releases each registry reference. A
callback-issued nested read remains pending until the following frame instead
of re-entering the current provider completion. The regression covers missing
and found keys, write/read ordering, empty account maps, callback arity,
strict generated-adapter tags and nested-request deferral.

## Skynest account provider completion timing

The adjacent identity bridge had the same collapsed provider boundary. IDA
recovers the eleven-member `SkynestAccount` constructor at `sub_1000A744C`;
Hopper confirms that `native_login` enters `sub_1000A3E04`, which sets the
in-progress byte at account-manager offset `+0x41` and selects one of three
provider login paths from its strict boolean arguments. The automatic-login
branch `sub_1000A4AB0` and social branch `sub_1000A4978` likewise set provider
state before constructing separate success and failure functors and calling
the identity provider. Neither branch can invoke Lua from the submission
stack.

Success continuation `sub_1000A3F64` and failure continuation
`sub_1000A4578` clear the in-progress byte before calling retained root members
`onLoginSuccess` or `onLoginFailure`. Failure maps the provider code through
the manager's error table and supplies `(errorName, message)` when no account
details exist. The retired-provider substitute follows the recovered
`ERROR_OTHER` branch and keeps its explanatory local message, but now delivers
it only from a later application-thread completion.

Nickname validation is independently retained by request ID.
`sub_1000A79B8` increments owner offset `+0xB0`, stores the supplied
LuaFunction in the tree rooted at `+0x80`, and gives paired functors to
`sub_10074B9A0`. Success continuation `sub_1000A7EEC` calls the function with
two booleans `(true, isValid)`; failure `sub_1000A7D80` calls it with one false
value; both erase the request entry afterward. Hopper recovers the same owner
offsets, functor allocation and provider call.

Rust now gives the account owner a completion FIFO alongside its shared
signed-out state. Explicit, social and constructor-equivalent automatic login
remain in progress until frame-head dispatch; nickname callbacks are held by
Lua registry keys and also wait for that boundary. Queue snapshots prevent a
validation callback from completing a nested validation recursively. Tests
prove the pre-completion state byte, deferred startup loading-screen release,
two-value validation result, two-argument login failure, strict adapters,
one-shot registry release and next-frame nested deferral.

The nearby synchronous `native_hasNickname` query is intentionally inverted
but not a storage lookup. `sub_1000A79B0` forwards to `sub_1000A3D68`, which
asks the identity provider for its profile nickname and returns whether that
string is empty. Both decompilers show no access to Skynest Storage or its key
map. The signed-out offline provider therefore returns true even after a
separate storage key literally named `nickname` is written; Rust no longer
couples those unrelated service states.

## Native joint-definition defaults and float narrowing

The initial `createJoint` reconstruction treated an omitted distance-joint
frequency and damping ratio as zero. That creates a rigid Box2D distance
constraint, but it is not Purple's definition. IDA's type-one branch in
`sub_100037374` loads `4.0f` at `0x1000382D0` when `frequency` is absent and
`0.5f` at `0x100038388` when `dampingRatio` is absent. Hopper independently
shows the corresponding `fmov s0, #4.0` and `fmov s0, #0.5` instructions.
After native joint creation, the same branch publishes both resolved values,
`collideConnected` and the calculated length back into the Lua descriptor.

The neighboring revolute branch also has a nonzero implicit upper limit. Its
fallback loads the four bytes `DB 0F 49 40` from `0x1009F74B0`, which are
single-precision pi; the lower limit remains `0.0f`. The prismatic branch keeps
its recovered `0.0f`/`5.0f` bounds, enabled motor and limits, zero speed and
`10000.0f` maximum motor force. Every supplied joint scalar in these branches
passes through `sub_10052A014` and is narrowed to float32 before Box2D sees it.

Rust now performs that same narrowing for creation-time motor, limit, spring,
break-force and destruction-delay values, installs the native distance and
revolute defaults, and mirrors the resolved distance fields into the retained
Lua descriptor. Regression coverage proves the `4.0f`/`0.5f` spring defaults,
single-precision pi limit, explicit-number rounding and descriptor publication.
