# Native window presentation color

GPU byte-path verification:2026-10-01 on macOS ARM64 / Apple M2 Max,
Metal, wgpu30.0.1. Display color-management correction:2026-10-09 below.
The full recreation goal remains ACTIVE and unproven.

## Native drawable boundary

Official IDA MCP rechecked the `MyEAGLView` constructor (`0x100408228`)
on2026-10-09. It compares `withBits` with32 at `0x100408300`. The32-bit
branch loads `kEAGLColorFormatRGBA8` at `0x100408308..0x10040830C`; the
other branch loads `kEAGLColorFormatRGB565` at `0x100408314..0x100408318`.
The selected format is installed as a drawable property at `0x10040835C`.
This selection has no sRGB renderbuffer branch. The same method marks the
layer opaque at `0x1004082C4..0x1004082C8`. Raw assembly is retained in
`target/audits/window-color-native.json`.
The corresponding compare, conditional branch, both format-slot loads and
opaque argument words also match the ARM64 slice of the local original
`Purple.app/Purple`; byte offsets are in
`target/audits/window-color-native-words.json`.
The new official IDA pseudocode and instructions are retained in
`.ida-mcp/native-display-color-space.json`; no new Hopper analysis was used.

This evidence supports preserving the ordinary normalized-channel game
framebuffer. It does not establish old UIKit's blend space, fractional text
metrics, display color management, or exact original-device pixels. The
non-retained drawable limit in `native-frame-capture-boundary.md` still applies.

## Reproduced host difference and correction

The host prefers a non-sRGB surface format but falls back to the first
advertised format. Previously both the game blit and native account compositor
used that surface format directly. With an sRGB attachment, already-encoded
UNORM texture samples were encoded again on output. The destination's sRGB
interpretation also changed the existing host UI blend arithmetic.

Real GPU readback in `target/audits/window-color-before.log` reproduces both
differences with the production pipelines. A game sample `[128,127,128]`
becomes `[188,187,188]`. A half-opacity UI sample over the game changes from
`[160,128,128]` to `[207,188,188]`; over black letterboxing, `[128,64,32]`
becomes `[188,137,99]`. Both new boundary tests fail before the correction.

The host now registers the non-sRGB alias in `SurfaceConfiguration.view_formats`
when the selected surface is sRGB. Both window pipelines and the acquired
texture view use the same alias. Surface allocation and presentation still
use the actual advertised format. The existing game/capture texture format,
sampling shader, and encoded-channel interpolation are preserved. Resizing
retains the view-format declaration and rebinds the replacement game texture.

This follows the [wgpu30.0.1 surface-view contract](https://docs.rs/wgpu/30.0.1/wgpu/type.SurfaceConfiguration.html#structfield.view_formats),
which permits the sRGB interpretation of a surface view to differ from its
allocation. Applying an inverse transfer function in the shared blit shader
would not by itself preserve the account layer's destination blending.

## Current verification

Three focused GPU regressions pass3/3 in35.21s. They use the production
format selection, pipeline creation, game presentation pass and UI compositor
on copyable substitute surfaces:

- All256 channel values survive presentation to RGBA/BGRA UNORM and sRGB
  allocations. Black letterboxing remains black. A resized game texture remains
  correctly bound; twofold black/white sampling yields0/64/191/255 in every
  format, using the original encoded-channel linear interpolation.
- Premultiplied UI alpha0/1/16/64/128/192/254/255 composites over both the game
  and letterboxing. An independent integer blend oracle allows one byte for
  quantization. The game framebuffer and a subsequent capture remain unchanged.
- The original-artwork SignIn raster, original OpenSans fonts and synthetic
  email render at1024x768 over a generated diagnostic color background. The
  complete composited RGBA pixels are identical across all four formats.
  These are host comparison images, not screenshots from an original iPad.

The four PNGs in `target/audits/window-color-pixels/` have the same SHA256:
`47d60a65b4362845bac4aa2bb26325eb2a945f3f85183f0a083d73049b3e0fa5`.
The sRGB/BGRA image was also visually inspected.

A separate main-thread winit audit creates a real, render-only Metal window.
It extracts and compiles the current production color/view/configuration
helpers, forces the advertised `Bgra8UnormSrgb` format, declares a
`Bgra8Unorm` view and presents a clear pass. It successfully presents at512x256
and again after a real window resize to640x320, then closes automatically.
The log records one initial `Occluded` deferral. This verifies actual native
surface configuration and view compatibility; the full game blit and account
pixels are verified separately by the production GPU regressions. The probe
does not measure original-device or monitor pixels.

Final-source app tests pass236/236 in47.93s. Strict all-target/all-feature
workspace Clippy passes; native app build, fmt and diff checks pass. The source,
native evidence, probe extraction, gate logs, selected original artwork and PNG
hashes are mapped in `target/audits/window-color-verification.json`.

The initial account-fixture import/visibility compile failure, probe's mixed
compiler-cache failure and first-redraw `Occluded` abort are retained as
historical logs. The final probe uses the compiler and dependency paths from
the successful Cargo build and handles transient acquisition states before
counting a presentation. Those fixture corrections did not change the
production color correction.

Windows/DX12 and Linux/Vulkan runtime/window parity, original-device visual
comparison, display profile behavior and the remaining full-goal requirements
are still open. No full cross-platform or native pixel-equivalence claim is
made by these local gates.

## macOS compositor color management,2026-10-09

The reported macOS/web difference persists even when the window texture's
bytes are correct. The locked wgpu-hal30.0.1 Metal implementation maps
`SurfaceColorSpace::Srgb` to `CAMetalLayer.colorspace=nil`. `Auto` resolves to
`Srgb` for the ordinary8-bit surface, so explicitly selecting that enum alone
does not fix this version. Apple documents that a nil color space disables
color matching. The raw sRGB content is consequently interpreted in the
display's native color space, oversaturating colors on a wide-gamut display.
This can appear brighter; it is not a uniform increase in grayscale brightness.

The [Apple layer contract](https://developer.apple.com/documentation/quartzcore/cametallayer/colorspace)
and [upstream wgpu correction](https://github.com/gfx-rs/wgpu/commit/6726fa78baaca510d187bdbebd1637257fe9e465)
support setting `kCGColorSpaceSRGB`. Upstream merged this correction on
2026-09-29; the locked30.0.1 source still contains the nil assignment.
DX12's sRGB path uses `RGB_FULL_G22_NONE_P709`, and Vulkan uses
`SRGB_NONLINEAR`; neither shares this Metal-layer defect. This source comparison
does not establish live Windows or Linux display parity.

The desktop now declares sRGB independently of its UNORM attachment view.
A shared surface-configuration helper sets the actual Metal layer's sRGB
metadata after configuration. Initial creation, resize, reconfiguration and
lost-surface recreation all reach this helper. The typed HAL guard and layer
mutex retain existing ownership; Core Animation retains the color space.
Allocation happens only during surface configuration, never per frame. Failure
to create the system sRGB color space is propagated. The macOS dependency is
already present transitively at the same version; other targets receive no
CoreGraphics dependency. No game shader, texture, capture or blend arithmetic
was changed, and no performance-improvement claim is made.

### Local compositor measurement

Render-only main-thread winit windows compile the actual production format,
view and game-blit helpers. They render eight opaque128x128 patches on this
Mac's Apple M2 Max / macOS27.2, read back the Metal surface and capture only
their own window with `screencapture -l`. The test never boots Lua or opens
player data. Both captures contain the same3376-byte display ICC profile,
SHA256 `0083fd1288901643f352efe4a0d97d14a4251491175c4d587371ca619a87ce36`.

Before and after tagging, the complete512x256 raw BGRA surface is identical:
SHA256 `915f8285af3cae6283aa61796a8c471e7ca4ea9ad980bf237dae24d2fe1646a0`.
Before tagging, all eight captured display samples equal the raw source RGB.
After tagging, every sample exactly matches an independent LittleCMS
sRGB-to-capture-profile conversion. Representative values in the screenshot's
display profile are:

| Patch/source sRGB | Unmanaged display sample | Tagged display sample and independent reference |
| --- | --- | --- |
| Yellow250,199,51 | 250,199,51 | 230,202,87 |
| Red219,61,77 | 219,61,77 | 181,86,81 |
| Purple133,92,219 | 133,92,219 | 127,98,210 |
| Green32,180,92 | 32,180,92 | 113,174,103 |
| Gray128,128,128 | 128,128,128 | 128,128,128 |

These are ICC-tagged display-domain samples, not replacement game colors.
The same production helper also restores sRGB after a raw wgpu reconfiguration
resets the tag to nil. A replacement surface and real512x256-to640x320 window
resize retain sRGB and present successfully. The resized screenshot again
matches all eight independent reference samples; its raw initial surface
remains byte-identical to the two control runs.

The unchanged `web/renderer.js` was loaded in the actual in-app Chromium browser
with a synthetic prepared packet. It reports `drawingBufferColorSpace=srgb`,
GL error0, and exactly the same eight RGBA readback samples. It uses RGBA8 for
the game attachment and browser-managed sRGB for canvas presentation. This
browser check verifies the renderer's default declaration and numerical pixels;
it does not measure the browser's OS display-profile conversion.

### Validation and limits

Workspace development tests pass1988/1988 with2 existing ignored audits.
This includes the all256-channel, resized-binding, premultiplied-alpha,
capture-boundary and original-artwork account-overlay GPU regressions above.
Strict Clippy passes on macOS ARM64, Web/Emscripten, Linux ARM64/x86_64 and
Windows MSVC ARM64/x86_64. macOS also passes release-mode strict Clippy.
Cross checks prove compilation and linting, not foreign-platform display pixels.
The native debug build, formatting and diff checks pass. Normal player files
are unchanged. Exact evidence, hashes and logs are under
`target/audits/macos-web-color-20261009/verification.json`.

The historical optimized signed-zero physics failure is outside this color
change; no optimized full-suite success is claimed. No original-device ICC
or screenshot equivalence, full-game macOS/web visual equivalence, or complete
recreation-goal completion is claimed. This correction closes the reproduced
local Metal color-matching defect.
