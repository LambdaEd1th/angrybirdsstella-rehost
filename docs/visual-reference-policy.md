# Visual reference policy

The rehost must not treat its own deterministic screenshots as evidence of
fidelity to the original game. At most they prove that a particular execution
reached the capture point, produced an image and did not change relative to a
previous rehost run. A plausible-looking image cannot close a layout, asset,
timing, shader or lifecycle gap, and a difference from an unmatched screenshot
cannot by itself select the correct behavior.

Visual acceptance therefore requires matching provenance and native evidence.
Without a Tier 1 capture at the same state and time, positions, texture
selection, draw order, clipping, transforms and animation timing must be
validated from the iOS 1.1.6 executable plus bundle data and by numeric or
lifecycle assertions. Rehost screenshots remain useful for smoke testing and
regression detection only, including when they look subjectively complete.
Likewise, a still image from an original build is not a complete description
of the running game: transient animation state, save data, random state,
platform composition and capture timing can all select a different legitimate
frame. A screenshot must never override directly recovered runtime behavior or
be used to tune the rehost merely until that one frame looks similar.

## Evidence tiers

For the current iOS 1.1.6 reconstruction, Tier 2 is the operative source of
truth: the shipped `Purple` executable and bundle resources, recovered through
IDA Pro and Hopper. No Android device, APK or capture is required for new
implementation or acceptance work.

1. An untouched capture from the target build on original hardware or a
   compatible device, with version, display geometry, state and capture time.
2. Numeric layout/resource data shipped in the matching app bundle and native
   behavior recovered independently with IDA Pro and Hopper.
3. A capture from a nearby original build on another platform. This can prove
   shared scene structure, sprite selection and state flow, but platform or
   version differences must be recorded rather than classified as bugs.
4. Public screenshots whose build, platform or save state is uncertain. These
   are structural hints only.
5. Rehost-generated screenshots. These are regression artifacts, never an
   original-game reference.

## Archived Android reference (non-authoritative)

The material below records earlier comparison work only. Following the move to
an iOS-binary-only workflow, it must not be consulted to choose behavior,
placement, timing or assets, and no new ADB capture is part of the validation
loop. Existing files are retained as provenance rather than silently deleted.

The formerly attached device provided an original Android 1.1.5 installation:

- package: `com.rovio.angrybirdsstella`
- activity: `com.rovio.fusion.App`
- device: Xiaomi 23054RA19C, Android 15
- physical display while landscape: 2460 x 1080
- Android compatibility window: x=287..2295, y=0..1079 (2009 x 1080)
- SurfaceView buffer: 2009 x 1080 after rotation

Raw ADB `screencap` PNG files under `build/reference/android-1.1.5/` are Tier 3.
Files whose names start with `derived-` are crops or resamples for inspection
and must not be used as pixel goldens. The 1.1.5 installation's save state must
also be recorded for every scene-specific comparison. Capture hashes and the
current save/flow notes live in `build/reference/android-1.1.5/manifest.md`.

On this MIUI/Android 15 device, `screenrecord` does not reliably composite the
legacy Fusion/OpenGL popup layer even though an immediate ADB `screencap` sees
it. Video therefore cannot prove that a popup was absent, and screen-recorded
popup frames are excluded from visual comparison. ADB `screencap` itself takes
roughly 0.6--0.8 seconds here, so it can preserve settled popup geometry but
cannot sample BasePopup's 0.7-second opening tween. Animation timing must come
from matching shipped Lua plus IDA/Hopper native draw evidence unless a faster
hardware capture path is available.

The first main-menu capture shows that the Android game renders into a
2009 x 1080 wide framebuffer and anchors edge UI to its horizontal extent.
Normalizing only the height to the shipped 768-pixel reference gives an
approximately 1429 x 768 field, but this does **not** prove that Android's Lua
globals use those normalized numbers. Purple 1.1.6 native routines
`sub_10006E1F4` and `sub_10006E990` independently show the relevant contract:
they read render-device width/height through virtual members and publish them
as Lua `screenWidth` and `screenHeight`, including on resolution changes. A
fixed 1024 x 768 host is therefore not a complete model of original viewport
behavior.

The Android APK can be pulled with `adb shell pm path`/`adb pull` without
changing application data. Decrypted shared assets should be hashed against
the iOS bundle before a cross-platform screenshot is used to diagnose
rendering. A visual match must still yield to instruction-level evidence. A
previous page-one comic comparison suggested a center-based animation pivot.
The complete SpriteComponent/SpriteComponentCustom path later showed the more
specific contract: the base component builds its quad around the authored SPRT
pivot, then the derived draw member appends `pivot - size/2` to its world
matrix. Together those stages centre the raw quad while preserving the native
float32 operation order. The old normalized correlation was therefore useful
only for locating a discrepancy, not for selecting either native stage in
isolation.

## Comparison requirements

- Keep the raw full-display image and its SHA-256 digest.
- Record the app version, package, foreground activity, physical display size,
  app content rectangle and game/save state.
- Mask or exclude Android system overlays such as Game Turbo; never attribute
  their dimming or controls to the game.
- Record the capture API (`screencap`, `screenrecord`, camera or framebuffer
  readback). Do not assume two Android capture APIs include the same layers.
- Normalize only after retaining the raw capture. State exactly how a derived
  image was cropped or scaled.
- Prefer semantic assertions (sprite identity, anchors, z-order, visibility,
  state and timing) until both captures share build, platform, aspect ratio,
  save state and animation time.
