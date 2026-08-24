//! Native program identity, blend-state and surface-format selection.

use super::*;
use crate::gpu::{
    frame::shader_uniform,
    resources::{premultiplied_blend, straight_blend},
};

#[test]
fn pipelines_match_recovered_gl_blend_factors() {
    let premultiplied = premultiplied_blend();
    assert_eq!(premultiplied.color.src_factor, wgpu::BlendFactor::One);
    assert_eq!(
        premultiplied.color.dst_factor,
        wgpu::BlendFactor::OneMinusSrcAlpha
    );
    assert_eq!(premultiplied.alpha, premultiplied.color);

    let straight = straight_blend();
    assert_eq!(straight.color.src_factor, wgpu::BlendFactor::SrcAlpha);
    assert_eq!(
        straight.color.dst_factor,
        wgpu::BlendFactor::OneMinusSrcAlpha
    );
    assert_eq!(straight.alpha, straight.color);
}

#[test]
fn prepared_draws_preserve_native_gl_context_program_identity() {
    let mut frame = PreparedFrame::default();
    let positions = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
    for program in [
        NativeProgram::Plain,
        NativeProgram::PlainAlpha,
        NativeProgram::Sprite,
        NativeProgram::SpriteAlpha,
        NativeProgram::SpriteAlphaMasked,
    ] {
        frame.push_quad(
            positions,
            [[0.0, 0.0]; 4],
            [[0.0, 0.0]; 4],
            [[0.0, 0.0]; 4],
            DrawUniform::default(),
            WHITE_TEXTURE.to_owned(),
            WHITE_TEXTURE.to_owned(),
            program,
        );
    }
    assert_eq!(
        frame
            .draws
            .iter()
            .map(|draw| draw.program)
            .collect::<Vec<_>>(),
        [
            NativeProgram::Plain,
            NativeProgram::PlainAlpha,
            NativeProgram::Sprite,
            NativeProgram::SpriteAlpha,
            NativeProgram::SpriteAlphaMasked,
        ]
    );
}

#[test]
fn sprite_program_uses_native_surface_format_and_state_alpha() {
    assert_eq!(
        native_sprite_program(SurfaceFormat::B8G8R8, 1.0),
        NativeProgram::Sprite
    );
    assert_eq!(
        native_sprite_program(SurfaceFormat::B8G8R8, f32::from_bits(0x3f7f_ffff)),
        NativeProgram::SpriteAlpha
    );
    assert_eq!(
        native_sprite_program(SurfaceFormat::A8B8G8R8, 1.0),
        NativeProgram::SpriteAlpha
    );
    // The recovered FCMP branch is strictly `< 1.0`; it does not treat a
    // value above one as requiring the alpha program.
    assert_eq!(
        native_sprite_program(SurfaceFormat::B8G8R8, f32::from_bits(0x3f80_0001)),
        NativeProgram::Sprite
    );
}

#[test]
fn ordinary_atlas_submission_preserves_the_surface_format_program_branch() {
    let texture_name = "<opaque-program-test>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "OPAQUE_SPRITE".to_owned(),
            AtlasRegion {
                texture: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "OPAQUE_SPRITE".to_owned(),
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                    pivot_x: 0,
                    pivot_y: 0,
                    atlas_rotation: 0,
                },
            },
        )]),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(
            texture_name,
            TextureAsset::new(
                RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
                SurfaceFormat::B8G8R8,
            ),
        )]),
        system_labels: SystemLabelPool::default(),
    };
    let command = |order, alpha| RenderCommand {
        order,
        sprite: "OPAQUE_SPRITE".to_owned(),
        texture: None,
        texture_scale: 1.0,
        masked_texture_binding: None,
        bound_region: None,
        bound_composite: None,
        shader: None,
        clip_holes: Vec::new(),
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState {
            alpha,
            ..stella_script::RenderState::default()
        },
        world_space: true,
    };
    let frame = assets
        .prepare_gpu_frame(&[command(0, 1.0), command(1, 0.5)], &[], &[], &[])
        .unwrap();
    assert_eq!(frame.draws[0].program, NativeProgram::Sprite);
    assert_eq!(frame.draws[1].program, NativeProgram::SpriteAlpha);
}

#[test]
fn indexed_reader_layout_uses_the_normalized_gl_texture_format() {
    let texture = TextureAsset::with_native_layout(
        RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
        stella_assets::native_image::ImageSurfaceLayout {
            pixels: SurfaceFormat::P8,
            palette: Some(SurfaceFormat::A8R8G8B8),
        },
    );
    assert_eq!(texture.source_layout.pixels, SurfaceFormat::P8);
    assert_eq!(texture.source_layout.palette, Some(SurfaceFormat::A8R8G8B8));
    assert_eq!(texture.upload_surface_format(), SurfaceFormat::A8B8G8R8);
    assert_eq!(
        native_sprite_program(texture.upload_surface_format(), 1.0),
        NativeProgram::SpriteAlpha
    );
}

#[test]
fn shader_prefixes_select_the_bundled_pixel_program_variants() {
    let mode = |name: &str| {
        shader_uniform(Some(&SpriteShader {
            name: name.to_owned(),
            ..SpriteShader::default()
        }))
        .header[3]
    };
    assert_eq!(mode("2d-sprite-colorize4"), 1.0);
    assert_eq!(mode("2d-sprite-silhouette2"), 2.0);
    assert_eq!(mode("2d-sprite-gold7"), 3.0);
    assert_eq!(mode("2d-sprite-diffuse-modulate9"), 4.0);
    assert_eq!(mode("2d-sprite-alpha"), 0.0);
}
