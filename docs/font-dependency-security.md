# Font dependency security

The rehost no longer depends on the unmaintained `ttf-parser`, `rustybuzz`, or
`ab_glyph` crates.  The SystemFont path now uses the maintained Fontations
`skrifa` parser/outline/bitmap APIs and HarfBuzz's maintained `harfrust`
shaper.  This keeps OpenType GSUB/GPOS shaping, COLR/SBIX rendering, fallback
font selection and the existing CoreText-compatible metric boundaries in the
Rust implementation while removing the RustSec unmaintained-crate alerts.

`winit` is configured without its optional Adwaita CSD feature.  That feature
pulled `sctk-adwaita -> ab_glyph -> owned_ttf_parser -> ttf-parser` into Linux
builds even though the game does not use the optional client-side decorations.
The X11, Wayland, dynamic-Wayland and raw-window-handle features remain enabled
for the supported desktop targets.

The migration is intentionally parser-compatible at the renderer boundary:
font bytes and face indices stay retained in `SystemFontLayoutFace`, and all
shaped glyph positions remain in font units until the existing native scaling
and compositing stages.  This avoids treating a dependency replacement as a
visual-layout change.
