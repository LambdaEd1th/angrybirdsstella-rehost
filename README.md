# Stella Rehost

An independent, cross-platform Rust rehost of the offline Angry Birds Stella
1.1.6 client. The original iOS executable is treated as a behavioral
specification and the locally supplied app bundle remains the asset source.

The working implementation currently includes:

- the complete encrypted-resource extraction and verification pipeline;
- on-the-fly conversion of Purple's custom Lua 5.1 bytecode ABI;
- the original `gamelogic.lua` → `game.lua` startup and fixed 60 Hz update loop;
- original level loading, definition-pack merging and native scene mirrors;
- KA3D/RVIO sprite sheets and both observed COMP v1/v2 layouts;
- PNG, WebP and PVR v2 RGBA4444/RGBA8888 textures;
- a native `wgpu` atlas/composite renderer in a resizable `winit` window;
- reverse-aligned premultiplied/straight-alpha pipelines, shaders, masks and
  bitmap text on Metal, Vulkan and Direct3D 12 backends;
- mouse, keyboard, wheel and native touchscreen-to-original-input bridging, including
  Purple's pinch/smooth-zoom state machine, and deterministic GPU PNG capture.

Visual fidelity is checked against original-game evidence. Deterministic Rust
screenshots are regression artifacts and are not treated as original-game
references.

The resource pipeline recovered from the ARM64 binary with IDA is:

1. AES-256-CBC, zero IV, PKCS#7 unpadding.
2. A single-file 7z archive.
3. Lua 5.1 bytecode for scripts and levels, JSON for animation/config data.

## Extract the game data

```sh
cargo run -p stella-tool -- extract \
  --source "angry birds stella v1.1.6/Payload/Purple.app/data" \
  --output runtime/data
python3 .github/scripts/runtime_native_assets.py stage \
  --bundle "angry birds stella v1.1.6/Payload/Purple.app" \
  --output runtime/data
```

The staging step includes native Skynest assets and hash-verified OpenSans
account fonts located outside the original `data` subtree. See
[`runtime/README.md`](runtime/README.md) for verification and plain-data export.

Inspect or decode individual files:

```sh
cargo run -p stella-tool -- inspect \
  "angry birds stella v1.1.6/Payload/Purple.app/data/scripts/game.lua"

cargo run -p stella-tool -- decode \
  "angry birds stella v1.1.6/Payload/Purple.app/data/scripts/game.lua" \
  --output extracted/single
```

Convert a PVR v2 RGBA texture to PNG:

```sh
cargo run -p stella-tool -- pvr-to-png input.pvr output.png
```

Convert Purple's 32-bit-float/32-bit-size Lua chunks to the current host's Lua
5.1 binary representation:

```sh
cargo run -p stella-tool -- transcode-lua input.lua output.lua
```

## Run the desktop rehost

With the data already extracted under `runtime/data` (the desktop app uses this
location by default):

```sh
cargo run --release -p stella-app
```

The desktop host enables a persistent local replacement for the retired
identity, cloud-save, Game Center and social providers by default. It keeps the
original asynchronous Lua callback boundary and stores only rehost-owned state
under `runtime/appdata`:

- `stella-device-id`: stable installation UUID published as Purple's global
  `uniqueDeviceId` string and used by the shipped per-device save keys;
- `stella-services.json`: local account identity, cloud settings and key/value
  data;
- `stella-gamer-services.json`: achievements and per-board high scores;
- `stella-social.json`: local social progress and leaderboard scores.

The original achievements and leaderboards buttons remain functional with the
local provider. Since desktop platforms do not expose Apple's Game Center
controller, the host opens a read-only local view generated from
`stella-gamer-services.json` at the same native presentation boundary.

The local provider also exposes the six coin bundles from the shipped economy
configuration as zero-price rehost products. Buying one follows the recovered
StoreKit transaction-status -> server-delivery -> wallet-voucher sequence and
therefore exercises the original `iap.lua` listeners without a platform store
or real-money charge. An unknown product retains the original immediate
`PURCHASE_FAILED` result.

Use `--offline-services` to exercise the original unavailable-provider branch
without creating or loading that local state.

The shipped GameServer facade and all of its original `/api/v1` routes can be
connected to a compatible replacement endpoint explicitly:

```sh
cargo run --release -p stella-app -- \
  --game-server-url http://127.0.0.1:8080/api/v1
```

This restores the release-disabled request closures in
`GameServerConnection.lua`; it does not redirect traffic to, impersonate or
claim to restore Rovio's retired production service. Without the option, the
challenge flow uses the deterministic local completion provider.

Server-time synchronization has a separate compatible-provider boundary. Pass
the full replacement endpoint URL; a successful HTTP 200 response may be a
JSON Unix timestamp or an object containing `time`, `serverTime`, `timestamp`
or `epoch`:

```sh
cargo run --release -p stella-app -- \
  --server-time-url http://127.0.0.1:8080/identity/2.0/time
```

Without this option, `ServerTime` uses the host clock and still preserves the
original asynchronous synchronization event.

Downloadable Assets has its own compatible-provider boundary. Pass the full
manifest endpoint corresponding to the original
`apdrive/1/apps/<app-id>/assets` route:

```sh
cargo run --release -p stella-app -- \
  --assets-url http://127.0.0.1:8080/apdrive/1/apps/purple/assets
```

The client sends one repeated `name` query parameter per requested asset. A
successful JSON response contains `assets` entries with `name`, `cdnURL` (or
legacy `url`), `hash` and `size`, plus a `failedAssets` array. Downloads are
stored under `runtime/appdata/assets_service`, validated against the exact
declared size and reused only while their persisted hash and size still match.
Without this option, the same Lua API reads only the existing AppData cache.

Rovio Account/Identity Level 2 also has an explicit compatible-provider
boundary. Pass a replacement `identity/2.0` or `identity/3.0` service root:

```sh
cargo run --release -p stella-app -- \
  --identity-url http://127.0.0.1:8080/identity/2.0
```

The client preserves the selected server and any prefix before `/identity`.
Following Purple's per-operation routing, access and nickname validation use
`identity/2.0`, while `profile/own` uses `identity/3.0`, regardless of the
configured root version. Arbitrary non-identity roots, URL credentials,
queries, fragments and ambiguous/traversing paths are rejected; redirects
are not followed. Active login uses the recovered nested app-session response;
the parent identity's access request preserves Purple's 16-field form body.
Interactive sign-in, registration, password reset and delayed validation use
their recovered provider layers and native-layout forms; callbacks return only
at the application frame head. A replacement operator may additionally pass
`--identity-client-id`, `--identity-client-signature` and
`--identity-client-salt`. These are explicit compatible-service values—the
rehost never embeds or derives the retired application's production secrets.
Use `--identity-client-key-file` instead of literal signature/salt to generate
the recovered per-request signature from an explicitly supplied replacement key.
The file is read as exact bytes, including any newline; it is not hex-decoded.
Without `--identity-url`, the persistent local guest identity remains active.
Session renewal and one first-401 replay are implemented, with refresh/profile
persistence isolated by provider and client. Native social/unregistration flows
and all session-event consumers are not yet complete.

Cloud settings and Skynest key/value requests can likewise target an
independently operated compatible `storage/1.0` service:

```sh
cargo run --release -p stella-app -- \
  --storage-url http://127.0.0.1:8080/storage/1.0
```

The client appends the recovered `state` and `states/query` routes, escapes
keys under `[my]/[client]/`, preserves optimistic hashes and encodes `SDKv2`
values as LZMA plus padded URL-safe Base64. Single reads/writes require HTTP
200 and a single-element JSON response array. Completions return to Lua at
the frame head. When identity is configured, storage acquires/renews its active
Level2 session and replays the first 401 once using fresh `X-Access-Token` and
token-derived `Rovio-Sgs` headers. Alternatively, explicit
`--storage-access-token` / `--storage-signature` options select a frozen-header
mode without automatic renewal; they override the corresponding inherited
header, not the identity client signature. No retired account credential is
embedded or inferred. Account/provider changes invalidate pending results and
the online cache without replacing the independent local-provider document.
The decoded `PurpleState` value is native Lua assignment text, not JSON. On a
409 write conflict, the client retrieves the remote state once and gives cloud
data to the original Lua merge callback; it never silently force-overwrites it.
Without `--storage-url`, cloud data stays in the persistent local provider
described above.

The native social manager has a separate compatible-provider boundary. The
original client delegated this layer to the platform Facebook/iOS SDK rather
than a stable public HTTP route, so the rehost exposes a small explicit JSON
operation endpoint while preserving the recovered native method and callback
ABI:

```sh
cargo run --release -p stella-app -- \
  --social-url http://127.0.0.1:8080/stella/social
```

The client POSTs `connect`, `getFriendsProgress`, `postScore`,
`fetchLeaderboard` and `setProgress` operations on background workers.
Connection results populate the synchronous friend/account lookup cache; all
original async result callbacks are delivered only at the application frame
head. This endpoint is operated by the user and is not a Facebook login or a
restoration of Rovio's retired service. Without it, the persistent local
provider supports editable friends, progress, per-level scores, ranked mixed
leaderboards and avatar resource lifetimes in `stella-social.json`.

Local and compatible-endpoint providers do not restore the original third-party
services or authorize real-money purchases.

The original camera/provider services are no longer available on desktop, but
the reverse-matched Telepods flow can be exercised with any product identifier
from `runtime/data/config/telepod_configuration.json`:

```sh
cargo run --release -p stella-app -- \
  --telepod-code hasbro.telepod.020
```

This exposes a virtual QR scanner, queues the payload until the original scan
page opens, and then runs the shipped validation, wallet and unlock callbacks.
Recognition is delivered through the native-style application event queue, not
inside scanner start or callback registration. Stopping capture does not cancel
an already recognized result; the original page clears its callback on exit.
This is decoded-code injection, not physical camera capture or QR decoding.

Mobile cross-promotion metadata and installed-app responses both use Purple's
recovered `canOpenURL` check. Desktop defaults to an empty application-scheme
registry. A host integration or deterministic test can explicitly advertise
one or more handlers; matching installed-app names are returned in authored
order and `AppStoreLauncher` opens the configured launch URL instead of its
StoreKit fallback:

```sh
cargo run --release -p stella-app -- \
  --installed-url-scheme angrybirds \
  --installed-url-scheme badpiggies
```

Generate a deterministic render without opening a window:

```sh
cargo run -p stella-app -- \
  --data runtime/data \
  --screenshot build/stella-island-map.png \
  --screenshot-frames 60
```

Long-run the original script state machine without graphics:

```sh
cargo run -p stella-script --bin stella-headless -- \
  --data runtime/data --local-services --frames 3600 --dump-render
```

`stella-headless` leaves local providers disabled unless `--local-services` is
passed. It accepts the same `--game-server-url`, `--server-time-url`,
`--assets-url`, `--identity-url`, `--storage-url`, `--social-url` and repeatable
`--installed-url-scheme` options for transport and script contract tests.

## CI and releases

`.github/workflows/ci.yml` checks formatting, strict Clippy, portable tests and
compilation for macOS ARM64, Windows x86_64/ARM64 and Linux x86_64/ARM64. The
complete local test suite additionally requires the locally extracted original
game data, which is intentionally absent from GitHub Actions.

Create a release by pushing a semantic version tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The Release workflow can also be started manually with the same tag in the
GitHub Actions interface. It builds the four workspace executables for all five
targets, publishes `.tar.gz` archives for macOS/Linux and `.zip` archives for
Windows, injects the verified `runtime/data` payload into every archive,
and attaches a shared `SHA256SUMS` file. Each package therefore runs without a
separate extraction step and contains its runtime instructions in
`README.md`. `BUILD-INFO.txt` records the release version, target, source commit
and compiler version.

## Workspace layout

- `runtime/data`: canonical, locally extracted game resources; intentionally
  ignored by Git and never uploaded by CI.
- `runtime/appdata`: writable saves, stable installation identity, settings and
  downloaded-asset state;
  intentionally ignored by Git.

- `stella-app`: resizable desktop host and `wgpu` atlas/composite renderer,
  restricted to `Backends::PRIMARY` (Metal, DX12, Vulkan and Browser WebGPU)
  rather than the secondary GL backend.
- `stella-assets`: resource crypto, 7z, Lua, PVR v2 and KA3D/RVIO formats.
  Its `ka3d.rs` facade preserves the public parser API while `ka3d/envelope.rs`,
  `reader.rs`, `sprite.rs`, `composite.rs`, `font.rs` and `localization.rs`
  follow the recovered container, SPRT/COMP, FONT and TEXT loader ownership;
  `native_image.rs` preserves the PNG/WebP reader pixel and palette layouts,
  while `surface_format.rs` owns reader-to-GL-upload normalization.
- `stella-core`: platform-independent simulation contracts and fixed-step state.
- `stella-script`: Lua 5.1 runtime, native compatibility ABI and scene bridge.
- `stella-tool`: parallel extraction, inspection, verification and texture conversion CLI.

`stella-app/src/gpu.rs` is the small shared render ABI and test boundary.
Its recovered backend stages live in `gpu/frame.rs` (ordered immediate-command
facade), `gpu/frame/batch.rs` (immediate mesh batching, scissor and native
viewport FMADD), `gpu/frame/commands.rs` (ordered immediate-command/capture coordination),
`gpu/frame/quads.rs` (native and explicit quads), `gpu/frame/sprites.rs`
(atlas/composite/Dirt dispatch), `gpu/frame/text.rs` (bitmap/3D glyph dispatch)
and `gpu/frame/text/system.rs` (deferred UIKit SystemFont label rasterization),
`gpu/frame/geometry.rs` (native geometry facade), with atlas-region,
colored-rectangle, DrawablePolygon/Dirt and shader-uniform expansion in its
`region.rs`, `rect.rs`, `dirt.rs` and `shader.rs` children,
`gpu/renderer.rs` (renderer facade), with device/pipeline setup, fixed-target
passes/captures, window/readback presentation and texture binding in its
`initialization.rs`, `pass.rs`, `presentation.rs` and `textures.rs` modules;
the initialization coordinator delegates fixed-target/bind-layout ownership,
the native program family and platform-surface construction to its `target.rs`,
`programs.rs` and `window.rs` children. `gpu/resources.rs` owns texture upload,
pipeline construction and native blend states, while `gpu/program.rs` owns the
recovered `GL_Context` program identities and SurfaceFormat/state-alpha
selection predicate. Prepared draws preserve the
distinct plain, plain-alpha, sprite, sprite-alpha and sprite-alpha-masked
program identities instead of collapsing them to blend factors. This follows
the original GL command/state/resource ownership instead
of dividing the wgpu implementation by line count. Boundary regressions live
in the `gpu/tests.rs` facade and its `batch.rs`, `program.rs`, `sprites.rs` and
`geometry.rs` children, leaving `gpu.rs` as the compact production storage ABI.
The executable host is similarly separated into `assets.rs` (KA3D catalog and
native `TextureAsset` facade), its `catalog.rs`, `texture.rs`, `sprite.rs`, `text.rs`,
`system_font.rs` and `transform.rs` resource/draw stages. `system_font.rs` owns
retained-face glyph masks, native label hashing, stroke dilation and premultiplied
label pixels. `texture.rs` is the cache/reader dispatcher;
its `texture/pvr_reader.rs` and `texture/raster_reader.rs` children mirror the
native PVR and PNG/WebP decode paths. `app.rs` is the window, input and fixed-step driver,
`audio.rs` is the cross-platform physical mixer/decoder bridge, and `cli.rs`
(deterministic interaction/screenshot front end). `main.rs` keeps
only the shared imports, native-size constants and tiny entry point;
`reference_renderer.rs` is the facade for the independent test-only CPU
rasterizer. Command traversal, colored meshes, sprite/quad rasterization,
texture sampling, pixel shaders, letterbox presentation and pixel-level tests
live in focused submodules that mirror the recovered native GL responsibilities.

`stella-script` is being split along native ownership boundaries recovered
from IDA/Hopper. `src/game_lua/text_files.rs` is the text-resource facade;
`text_files/imports.rs`, `pipeline.rs` and `paths.rs` separately own the
Lua/JSON adapters, recovered raw/AES/7z byte pipeline and extracted-resource
path resolution;
`src/game_lua/persistence.rs` owns AppData table serialization and encryption;
`src/game_lua/platform.rs` owns the small platform/registration adapters;
`src/game_lua/simple_random.rs` owns the two recovered native PRNGs; and
`src/game_lua/object_api.rs` coordinates scene-object inspection with
transform, physics, feature and render-object visual property registration
modules. RenderObject/Box2D getters retain native registration order in the
19-line `object_query_registration.rs` facade; appearance, nullable body
motion/sleep, physics-lock state, and direct point-transform members live in
`object_query_registration/{appearance,motion,world,points}.rs`. The former transform block is now an
order-only coordinator: strict position/rotation/scale members live in
`object_pose_registration.rs`, while fixture reconstruction and its Lua/native
write order live behind the 25-line `object_physics_scale_registration.rs`
facade. Its `arguments.rs`, `member.rs` and `fixture_rebuild/` children mirror
the recovered adapter, `sub_10004050C`, polygon member `sub_100067CE8`, circle
branch, and shared Box2D fixture lifecycle. Ordinary visual scale is shared by
direct `setScale` and `setPhysicsScale` through `object_scale_member.rs`, the
Rust counterpart of `sub_100040304`.
`native_setDensity` stays with fixture/body state in
`object_body_registration/density.rs`; its 19-line native-order facade also
delegates coefficient, flag, and active/contact lifecycle members to
`scalars.rs`, `flags.rs`, and `activity.rs`. `native_setSprite` stays with
native visual state in `object_visual_registration.rs`. The physics cluster likewise
preserves its native install order through a small coordinator and separates
body motion and forces, body/fixture flags, and RenderObject
material/texture/water fields into `object_motion_registration.rs`,
`object_body_registration.rs`, and `object_material_registration.rs`. The
former mixed feature installer is also an order-only coordinator: RenderObject
parameters/gravity fields, pivot and decoration construction, joint motor
mutation, and object/flash lifecycle live in
`object_parameter_registration.rs`, `object_decoration_registration.rs`,
`object_joint_registration.rs`, and `object_lifecycle_registration.rs`;
the feature façade interleaves their phase installers by recovered constructor
address, while the large `setObjectParameter` switch remains one cohesive leaf;
`src/game_lua/render_api.rs` coordinates theme, immediate primitive, UI text,
textured/masked and direct sprite/composite registration modules;
`textured_render_registration/box_draw.rs` is the small `drawBoxNative`
facade; its `arguments`, `layout`, `resource` and `background` children mirror
the hand-written Lua stack reader, `sub_100051BBC` placement flow,
ResourceManager width/height/draw helpers and packed-color branch rather than
splitting the native member by source line count;
`theme_render_registration.rs` keeps the six theme members in native relative
order, delegating replacement, offsets, selection and the two render passes to
the adjacent `theme_render_registration/` leaves;
`direct_sprite_registration.rs` preserves the executable's four-member
registration order while `direct_sprite_registration/composite.rs`,
`shader.rs`, `plain.rs` and `lookup.rs` own the separate native members;
`primitive_render_registration.rs` is a native-order façade over the adjacent
`rectangle.rs`, `polygon.rs` and `lines.rs` leaves, preserving the distant
`drawRect`/`drawPolygon` and `drawLine2D`/`drawRectLines` registration sites;
`audio_registration.rs` preserves channel-limit, handle play, clip volume and
handle stop order across its `channel`, `playback` and `volume` leaves;
`particle_registration.rs` separates the early draw/clear/enable cluster from
the late particle-table spawn member while publishing Lua containers first;
`trajectory_registration.rs` interleaves raw simulation-vector, one-body
predictor, AimStream, double-buffered trail and sprite-slot phase installers by
their recovered `sub_10002C274` addresses; its nested `simulation/`,
`aim_stream/` and `trail_buffers/` leaves each correspond to a concrete native
member or store boundary;
`ui_text_registration.rs` retains the strict `drawUITextNative` ABI, early
visibility/font ordering and parent transform, then follows the recovered
native branch into `ui_text_registration/clipped.rs` for Lua line callbacks or
`ordinary.rs` for localized bitmap-font submission and persistent GL state;
`src/game_lua/theme_objects.rs` is the order-only theme-object coordinator;
theme runtime records, generated ThemeSprite adapters, direct-Lua
ThemeAnimation parsing, argument coercions and authored layer decoding live in
`theme_state.rs`, `theme_sprite_registration.rs`,
`theme_animation_registration.rs`, `theme_arguments.rs` and
`theme_layer_parser.rs`; `theme_sprite_registration.rs` is an order-only
facade over the adjacent create/remove/modify/rotate member leaves. Matching
the native list/record boundary, its
`theme_layer_parser/layer.rs` child owns construction of one authored layer.
The per-frame theme chain follows `sub_10005E898` as well:
`frame_update/theme.rs` calls the background and foreground
`sub_10009B8B4` passes in `theme/layers.rs`, followed by GameLua's
`sub_1000607E8` layer-position and ThemeSpriteData traversal in
`theme/sprite_data.rs`.
`src/game_lua/level_files.rs` is the order-only
coordinator for the native level-file cluster, with editor-pack loading,
Bundle/AppData loading, fixed-schema saving, recursive Lua-table copying and
the late failure callback split across the adjacent `level_*` modules;
`loader_registration.rs` retains only generic script/object loading and
persistent-data registration;
`src/game_lua/render_primitives.rs` is the facade for the recovered immediate
geometry cluster; `render_primitives/color.rs`, `transform.rs`, `sprite.rs`,
`rect.rs`, `line.rs`, `polygon.rs` and `software.rs` follow the independent
native color, GL-state, direct-sprite, rectangle, line, DrawablePolygon and
host-only software paths;
`src/game_lua/particles.rs` owns deterministic particle query parsing and spawn;
`src/game_lua/script_runtime.rs` is the facade for `script_runtime/chunks.rs`,
`environment.rs`, `definitions.rs` and `paths.rs`, which separately own chunk
preparation/execution, object environments, definition indexing and safe
bundle/AppData routing;
`src/resource_manager/fonts.rs` owns system/bitmap font loading, metrics and
text clipping. `src/resource_manager/geometry.rs` is the facade for the native
resource-geometry cluster: `geometry/assets.rs`, `composite.rs`, `draw.rs`,
`lines.rs` and `model.rs` separately own KA3D discovery, composite Lua records,
`drawSprite` overloads, float32 line quads and bounds;
`src/resource_manager/registration.rs` preserves the complete `res` and
`ResourceManager` installation order recovered from `game::LuaResources`,
while lifecycle, locale/font, sprite-query, audio setup/playback, draw and
legacy-manager adapters live in focused registration modules;
`src/resource_manager/audio_playback_registration.rs` is now a 26-line facade
over the recovered `LuaResources` playback and master/track-volume members.
The separately owned handle APIs live in `src/game_lua/audio_registration.rs`
at their `GameLua` constructor boundary. AudioManager state keeps the native
eight float volumes, eight signed channel limits and a wrapping `u32` handle
counter beginning at zero;
`src/resource_manager/lifecycle_registration.rs` is a 23-line facade over
creation and release leaves. The resource coordinator calls those leaves in
the exact interleaved order of `sub_100446570`, rather than grouping every
lifecycle member ahead of audio, locale, draw and query publication;
`src/resource_manager/query_registration.rs` is an 18-line facade over
sprite/composite geometry queries, clip-rectangle state and current-font
selection. `draw_registration.rs` is a 20-line ordered facade over direct
sprite/composite submission, localized text submission, capture, and the final
offline `openURL`/`res` publication tail;
`src/resource_manager/localization.rs` owns localization-table discovery and
loading; `src/game_lua/time.rs` owns the native date/time table conversions;
`src/game_lua/input.rs` owns per-frame input queries and edge-buffer lifecycle;
`src/game_lua/arguments.rs` is the coercion façade: its `arguments/strict.rs`
leaf owns generated-adapter type guards, `lua51.rs` owns only the hand-written
C-API coercions, and `value.rs`, `table.rs` and `diagnostics.rs` keep raw
access, field contracts and tracing separate;
`src/game_lua/trajectory.rs` owns BirdSimulation stepping and AimStream spline
math. `src/game_lua/runtime_state.rs` contains the recovered aggregate GameLua
state; `host.rs` now contains only the public VM/filesystem facade, while
native startup, frame/draw dispatch, desktop input injection, output draining
and fixed-step physics live in `host_startup.rs`, `host_frame.rs`,
`host_input.rs`, `host_output.rs` and `host_physics.rs`; scene ownership and
live Lua collision-material synchronization live separately in
`host_scene_sync.rs`; `frame_update.rs` is the per-frame facade over the
separate particle integration, particle drawing, SceneObject reporting and
theme-update members, with the theme leaf split again at its recovered
ThemeManager/GameLua call boundaries; `textured_render_registration.rs` is the ordered facade
over the separately recovered textured-rect, selected-texture, masked-quad,
3D-text and nine-slice members; `trajectory_registration.rs` preserves the
constructor order across separate trail-buffer, BirdSimulation predictor and
AimStream modules; `scene_render.rs` separates theme repetition from object
submission, and `scene_render/objects.rs` is a small façade over ordinary
object state/submission (`sub_10006D5B4`) and the independent flash-animation
transform (`sub_10006794C`); `particles.rs` separates packed state, native CMWC randomness and
emission; `persistence.rs` separates AppData file members from the executable
Lua serializer family; `draw_registration.rs` is the ordered facade over
platform/Z-range/callback adapters, the scene dispatcher and its live
`scene/walk.rs` plus anchor-triggered `scene/trails.rs` leaves, and the
distinct textured-line/rubber-band ABIs in `draw_registration/misc.rs`,
`scene.rs` and `lines.rs`; and the ordered native constructor is decomposed into
the 132-line `registration.rs` facade plus its focused `registration/clip_text.rs`,
`notifications.rs` and `compatibility.rs` members, `bootstrap.rs`,
`platform_services.rs`, `loader_registration.rs`, `particle_registration.rs`,
the ordered `world_registration.rs` façade over
`world_transform_registration.rs`, `world_environment_registration.rs` and
`world_physics_camera_registration.rs`; that façade now preserves the
cross-family `sub_10002C274` registration order, while the adjacent
`world_*_registration/` leaves separate coordinate conversion, GL context,
water, lifecycle, device, physics, camera, framebuffer and locale members;
`draw_registration.rs`, and
`trajectory_registration.rs`. `src/animation_wrapper/model.rs` owns the shared
animation runtime/data model; `model/asset.rs`, `timeline.rs`, `tracks.rs` and
`shader.rs` own JSON/skin loading, event crossing, typed sampling and shader
table decoding respectively. The nine-line `model/asset.rs` facade follows the
two native load entries and their shared scene constructor: `asset/loading.rs`
owns animation JSON/hierarchy recovery, `asset/skins.rs` owns the fixed-length
`.anim.json` to `.skins.json` companion lookup and transform decoding, and
`asset/runtime.rs` owns installation into the live wrapper state. Meanwhile,
`src/animation_wrapper/transform.rs` is the hierarchy/draw facade;
`transform/hierarchy.rs`, `affine.rs`, `skin.rs`, `queries.rs` and `render.rs`
own track transforms, full matrices, attachment resolution, entity bounds and
render-command generation respectively. `src/tests.rs`
keeps compatibility regressions outside the production facade.
`src/animation_wrapper/registration.rs` is the native-order Lua method-table
coordinator; resource lifetime, playback, scene mutation/draw, entity queries
and missing-method auditing live in its focused `registration/` modules.
The playback group is itself a small facade over native-aligned `controls`,
frame `update`/event drain, and callback-registration leaves, corresponding to
the separate start/speed/seek/event members referenced by the original
AnimationWrapper constructor.
`src/game_lua/platform_services.rs` is likewise only the original publication-
order facade: ForceUpdate, Analytics, FusionGamerServices, downloadable Assets
and Align are separate service-owner modules, while AnimationWrapper and
SimpleRandom remain delegated to their existing implementations.
`src/game_lua/platform.rs` is now a 35-line native-order coordinator too.
Device registration/checksum, date/epoch conversion, installed-app checks,
filesystem/no-op platform calls, URL/screenshot sharing and SHA-1 live in
focused `platform/` leaves matching the members recovered from
`sub_10002C274`; the split preserves Lua names and their relative publication
positions instead of introducing a new host-side abstraction.
ResourceManager font ownership is similarly explicit: `fonts.rs` is a small
facade over shipped bitmap FONT loading/metrics, native clipText splitting and
cross-platform SystemFont implementation modules.
`src/physics_world/narrow_phase.rs` is the recovered narrow-phase
facade; circle-circle/polygon-circle, polygon SAT/clipping and shared float32
geometry live in `narrow_phase/circle.rs`, `polygon.rs` and `geometry.rs`.
The polygon unit follows the recovered Box2D call graph: `polygon.rs` owns
`b2CollidePolygons`, `polygon/separation.rs` owns centroid-seeded directional
`b2FindMaxSeparation` plus `b2EdgeSeparation`, and `polygon/clipping.rs` owns
the shared two-vertex `b2ClipSegmentToLine` leaf.
`narrow_phase/edge.rs` is a seven-line edge facade over the distinct
`edge/circle.rs` and `edge/polygon.rs` collision leaves, while
`src/physics_world/continuous.rs` is the recovered continuous-collision data
facade; `continuous/simplex.rs`, `distance.rs` and `separation.rs` mirror the
native `b2Simplex`, `b2Distance`, and `b2SeparationFunction`/`b2TimeOfImpact`
boundaries. The six-line separation facade delegates the three separation
members to `separation/function.rs` and conservative advancement to
`separation/toi.rs`. `src/physics_world/continuous_solver.rs` is the matching execution
facade: `continuous_solver/world.rs`, `position.rs` and `island.rs` preserve
the recovered `b2World::SolveTOI`, TOI position-constraint and reduced
`b2Island::SolveTOI` boundaries. `src/physics_world/broad_phase.rs` owns AABB proxy storage,
querying and dynamic-tree balancing through a small facade; `broad_phase/model.rs`,
`allocation.rs`, `proxy.rs`, `insertion.rs`, `removal.rs` and `balance.rs`
mirror the native node model, free list, proxy lifecycle, leaf splice and
rotation members. The world-facing wrapper is likewise a small
`broad_phase_bridge.rs` facade over allocation order, body/fixture proxy
lifecycle, swept-AABB synchronization and sorted UpdatePairs modules.
`src/physics_world/contact_velocity_solver.rs` is the contact-solver facade;
constraint initialization, warm start, impulse application, tangent plus
single/two-point normal solves, and final manifold-cache publication live in
its `initialization.rs`, `warm_start.rs`, `impulses.rs`, `solve.rs` and
`storage.rs` modules. Solver-local impulses remain separate throughout all
velocity iterations and reach the contact manifold only through the recovered
`StoreImpulses` phase.
`src/physics_world/contact_manager.rs` is the lifecycle facade; `filtering.rs`,
`refresh.rs`, `islands.rs` and `solve.rs` isolate game collision policy,
contact refresh, world-island assembly, and focused solver adapters without
changing intrusive contact order. Contact refresh is a module facade over
`refresh/{traversal,update,events}.rs`, matching `b2ContactManager::Collide`,
`b2Contact::Update` and listener-record ownership. World island assembly stays
in `islands.rs`, while `islands/sleep.rs` owns the distinct `b2Island::Solve`
sleep-time tail.
`src/physics_world/polygon_decomposition.rs` is the native polygon facade;
`polygon_decomposition/triangulation.rs`, `geometry.rs` and `merge.rs` own
float32 winding/repeated-point ear cutting, shared predicates, and convex
fixture merging respectively, while
`src/physics_world/registration.rs` is the ordered GameLua/PhysicsWorld
registration facade. Its `registration/scalars.rs`, `construction.rs`,
`joints.rs`, `tracks.rs`, `vertices.rs` and `queries.rs` modules own force/time
state, scene construction, joint lifetime, tracks, shape staging and native
intersection/ray-cast bindings respectively. The constructor facade follows
the recovered binding layers: `construction/adapters.rs` owns the five strict
Lua adapters, `shape.rs` owns native shape/body preparation, and
`object/{lua_mirror,scene}.rs` isolate the Lua-world mirror from SceneObject
and broad-phase installation;
`src/physics_world/extensions.rs` preserves the later extension installation
order; track/joint, object/sensor and native-block/Dirt method groups live in
focused registration modules. Its final mixed implementation is now a 28-line
facade over `extensions/{polygon,ray,light_beam,gravity_visuals}.rs`, matching
the independent decomposition, DrawablePolygon, LightBeam and sensor-debug
members.
Its former mixed track/joint aggregate is now a 20-line native-order facade:
`track_joint_registration/track.rs`, `joints.rs`, `flags.rs` and `vertices.rs`
separately own chain queries, Box2D-type-dispatched mutation, strict native-only
object bytes and fixture-list publication. The joint group is itself a
17-line order facade over `joints/parameters.rs`, `removal.rs` and `limits.rs`,
matching the four independent native members rather than their former shared
Rust registration file.
`src/physics_world/joints.rs` is the joint facade; `joints/model.rs`,
`geometry.rs` and `matrix.rs` separately own persistent state,
anchor/prismatic geometry and Box2D Solve22/Solve33 math. Joint construction
now follows the recovered bridge layers as well: the seven-line
`joints/construction.rs` facade delegates Lua descriptor publication plus
`createCustomJoint` dispatch to `construction/lua_bridge.rs`, and native
joint-definition decoding plus `b2World::CreateJoint` state insertion to
the `construction/physics.rs` facade. Its `physics/anchors.rs`,
`parameters.rs`, `model.rs` and `insertion.rs` children separate the recovered
coordinate/class switch, per-class defaults, decoded record and independent
world insertion/contact-filter member;
`src/physics_world/contacts.rs` owns contact keys, cached impulses and callback
payloads; `contacts/prepare.rs`, `damage.rs` and `dispatch.rs` separately own
synchronous listener decisions, collision damage/score propagation, and Lua
enter/exit plus gravity-sensor state; `src/physics_world/dirt.rs` is the Dirt
facade, with component state, Clipper difference, triangle rebuilding and Lua
definition reconstruction in `dirt/model.rs`, `clipper.rs`, `render.rs` and
`component.rs`. Its Lua-facing extension registration is a small ordered
facade over `native_block_registration/{factory,collision,rebuild,queries}.rs`,
matching the recovered DirtMechanics factory and four bound members;
`src/physics_world/sensors.rs` is now the recovered eight-line sensor-force
facade: `sensors/force_dispatch.rs` owns the signed-mask gate and gravity path
in `GameLua::applySensorForces`, while `sensors/water.rs` owns its separate
buoyancy/drag callee. The late object-extension registration is likewise a
small native-order facade over independent RenderObjectData mutation,
radius/fixture replacement and renderer/theme/platform runtime members in
`object_extension_registration/{object_state,radius,runtime}.rs`;
`src/physics_world/ray_cast.rs` owns
fixture ray tests; and `src/physics_world/tracks.rs` owns chain projection.
The `RenderBridge` world solver is split further into broad-phase, contact
lifecycle/manager, velocity/position constraint, joint, TOI, island motion and
position-integration modules. The position-constraint unit is a seven-line
facade over `position_constraints/model.rs`, `construction.rs` and
`world_manifold.rs`, separating stored local witnesses, their contact-manifold
construction and `b2PositionSolverManifold::Initialize` world evaluation.
The recovered `sub_10005E898` host boundary now
keeps fixed-time accumulation and post-step Lua ordering in `host_physics.rs`,
while `host_physics/contact_manager.rs`, `islands.rs` and `toi.rs` own the
corresponding `b2ContactManager::Collide`, `b2World::Solve`/`b2Island::Solve`
and `b2World::SolveTOI` phases. Joint island traversal stays in
`joint_solver.rs`; cached/body impulse writes live in
`joint_solver/impulses.rs`, and distance, prismatic, rope, weld and revolute
initialization plus velocity/position constraints live in per-type solver
modules matching the five recovered Box2D vtables. Prismatic and Revolute use
tiny facades over one file per vtable member; Rope and Weld remain separate
classes. `scene_object.rs`,
`scene_object_body.rs` and `scene_object_motion.rs` preserve the native body
and sweep layout; the five-line body facade delegates intrusive fixture
lifecycle, float32 shape-mass aggregation and Reset/SetMassData state to
`scene_object_body/{fixtures,mass,state}.rs`;
the seven-line motion facade mirrors the independent `b2Body`/game members in
`scene_object_motion/{body_type,bounce,forces,sweep,transform}.rs`. Native
body-local joint/contact points now use an unscaled float32 `b2Transform`;
`physicsScale` remains confined to fixture rebuild and shape projection.
`SetType` also flags every attached contact for deferred filtering after its
mass/velocity/force transition;
`scene_object_collision.rs` is the facade for shape projection, distance
proxies/AABBs, fixture manifolds and ray queries in four focused submodules.
Its former 245-line mixed `proxies.rs` unit is now a six-line facade over
`proxies/distance_proxy.rs`, `overlap.rs`, `aabb.rs` and `track.rs`, matching
the independent `b2DistanceProxy::Set`, `b2TestOverlap`, shape `ComputeAABB`
and `objectAndTrackOverlap` call chains. Fixture skin bounds now retain the
native float32 min/max and radius-addition order instead of using an f64
intermediate.
`contact_lifecycle.rs` is likewise only the facade for native teardown and
post-contact work: its `joints.rs`, `removals.rs`, `contacts.rs`, `sensors.rs`
and `forces.rs` modules follow the recovered Box2D joint/body destruction,
GameLua sensor bookkeeping and post-step collision-force boundaries.
Scene-table creation and lookup live with the native mutation bindings in
`src/game_lua/object_api.rs`. Public render commands live in
`src/render_types.rs`. `lib.rs` is now a 54-line public facade.

Regression evidence follows those same recovered boundaries. The former
12,664-line `stella-script/src/tests.rs` aggregate is now a small module map
over focused loader, discrete/continuous world, broad/narrow phase, contact,
joint-family, sensor/track, Dirt, resource/platform, render/scene, theme and
animation suites. The formerly 782-line theme aggregate is itself an eight-line
facade over lifecycle, render, repeat/cull, motion, sprite-vector and animation
adapter suites. Hopper's 6,476-byte/237-block view of `sub_10005E898`
confirms that fixed-step orchestration itself remains one entry while its
Box2D and GameLua callees stay independently testable.

The cross-platform host follows explicit boundaries too. `app.rs` is a small
state/constructor facade; `app/runtime.rs`, `audio.rs`, `input.rs`,
`screenshot.rs` and `window.rs` separately own fixed-step GameLua draw
submission, native audio-state synchronization and physical mixing,
letterbox pointer mapping, scripted offscreen capture and winit lifecycle
dispatch.

Retired 2014 services retain their recovered Lua/native ABI instead of being
silently removed. Account/cloud, achievements, scores and social progress have
persistent local providers; Telepods uses the host scanner and local wallet;
downloadable content consumes the recovered on-disk cache; challenge requests
use either the deterministic local provider or the shipped route/payload
facade against an explicitly configured compatible endpoint. URL, App Store,
video and screenshot-share requests cross the platform-action boundary to the
desktop host. Facebook/Game Center/StoreKit/Zappar SDK credentials, copyrighted
server-side data and Rovio's retired service implementations are not present in
the client binary and are therefore not fabricated by the rehost.

The recovered Box2D simulation, Lua/native ABI, skeletal animation, pure-Rust
Clipper-compatible dirt geometry and OpenGL-to-`wgpu` render paths are
implemented, including the recovered float32/FMA transform boundary. Physical
audio output follows the recovered output lifecycle and decodes/mixes the
shipped WAV, MP3 and Vorbis assets through a cross-platform host. Remaining
platform/driver-dependent ordering, resampling or subpixel details stay bounded
compatibility surfaces and are not claims of full native equivalence.

## License

Unless otherwise noted, repository-authored source code is licensed under the
[GNU Affero General Public License, version 3 or later](LICENSE)
(`AGPL-3.0-or-later`). Third-party components remain under their respective
licenses.

The original Angry Birds Stella application, `runtime/data`, and all other
Rovio-owned game resources are not covered or relicensed by this license.
