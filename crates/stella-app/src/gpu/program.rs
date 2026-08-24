//! Recovered `GL_Context` program identities and selection policy.

use stella_assets::surface_format::SurfaceFormat;

/// Native `GL_Context` program identity. Several programs share a wgpu blend
/// state, but Purple caches and selects them independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeProgram {
    Plain,
    PlainAlpha,
    Sprite,
    SpriteAlpha,
    SpriteAlphaMasked,
}

/// `sub_10059E254` and `sub_10059E68C` select SpriteAlpha when the texture's
/// native surface format has alpha or the current draw-state alpha at `+0x40`
/// is below one.
pub(super) const fn native_sprite_program(
    surface_format: SurfaceFormat,
    state_alpha: f32,
) -> NativeProgram {
    if surface_format.has_alpha() || state_alpha < 1.0 {
        NativeProgram::SpriteAlpha
    } else {
        NativeProgram::Sprite
    }
}
