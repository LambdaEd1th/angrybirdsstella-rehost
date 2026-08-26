//! Deferred bitmap-font object/texture ownership at the wgpu boundary.

use super::*;
use stella_assets::ka3d::FontGlyph;

fn font(texture: &str, width: i16) -> BitmapFont {
    BitmapFont {
        texture: texture.to_owned(),
        leading: 0,
        tracking: 0,
        glyphs: vec![FontGlyph {
            codepoint: u32::from(b'A'),
            x: 0,
            y: 0,
            width,
            height: 6,
            pivot_y: 0,
        }],
    }
}

#[test]
fn submitted_text_uses_bound_font_geometry_and_texture_not_active_name() {
    let bound_texture = "<bound-font-texture>".to_owned();
    let active_texture = "<active-font-texture>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::from([("FONT".to_owned(), font(&active_texture, 1))]),
        textures: HashMap::from([
            (bound_texture.clone(), alpha_texture(16, 16)),
            (active_texture, alpha_texture(8, 8)),
        ]),
        system_labels: SystemLabelPool::default(),
    };
    let command = TextRenderCommand {
        order: 0,
        text: "A".to_owned(),
        font: "FONT".to_owned(),
        font_binding: Some(TextFontBinding::Bitmap {
            font: font("ignored-relative-name.pvr", 4).into(),
            texture_source: bound_texture.clone(),
        }),
        x: 10.0,
        y: 20.0,
        native_system_origin: None,
        scale_x: 1.0,
        scale_y: 1.0,
        angle: 0.0,
        matrix: None,
        alpha: 1.0,
        horizontal_anchor: "LEFT".to_owned(),
        vertical_anchor: "TOP".to_owned(),
        projection_3d: None,
        clip_rect: None,
    };

    let frame = assets.prepare_gpu_frame(&[], &[command], &[], &[]).unwrap();
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.draw_texture_pair(0).0, bound_texture);
    assert_eq!(frame.vertices.len(), 6);
    let min_x = frame
        .vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .reduce(f32::min)
        .unwrap();
    let max_x = frame
        .vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .reduce(f32::max)
        .unwrap();
    assert_eq!(max_x - min_x, 4.0);
    assert!(frame.vertices.iter().any(|vertex| vertex.uv[0] == 0.25));
}

#[test]
fn font_v2_utf32_glyphs_reach_the_wgpu_quad_path() {
    let texture_source = "<utf32-font-texture>".to_owned();
    let mut utf32_font = font("ignored.pvr", 5);
    utf32_font.glyphs[0].codepoint = 0x1f600;
    utf32_font.glyphs[0].pivot_y = -2;
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_source.clone(), alpha_texture(8, 8))]),
        system_labels: SystemLabelPool::default(),
    };
    let command = TextRenderCommand {
        order: 0,
        text: "😀".to_owned(),
        font: "UTF32".to_owned(),
        font_binding: Some(TextFontBinding::Bitmap {
            font: utf32_font.into(),
            texture_source: texture_source.clone(),
        }),
        x: 0.0,
        y: 0.0,
        native_system_origin: None,
        scale_x: 1.0,
        scale_y: 1.0,
        angle: 0.0,
        matrix: None,
        alpha: 1.0,
        horizontal_anchor: "LEFT".to_owned(),
        vertical_anchor: "TOP".to_owned(),
        projection_3d: None,
        clip_rect: None,
    };

    let frame = assets.prepare_gpu_frame(&[], &[command], &[], &[]).unwrap();
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.draw_texture_pair(0).0, texture_source);
    assert_eq!(frame.vertices.len(), 6);
}

#[test]
fn system_text_builds_premultiplied_label_and_uses_native_stroke_anchor_geometry() {
    let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSystemFontWithStroke(
                    "SYSTEM", "Arial", 24,
                    255, 200, 50, 10, 0, 2, 255, 5, 100, 200
                )
                res.useFont("SYSTEM")
                res.drawString("MISSING_GROUP", "A", 100.75, 200.75, "RIGHT", "BOTTOM")
            "#,
        )
        .unwrap();
    let command = runtime.take_text_commands().remove(0);
    let TextFontBinding::System(binding) = command.font_binding.as_ref().unwrap() else {
        panic!("system font submission lost its native kind");
    };
    assert_eq!(binding.stroke_width, 2);

    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
    };
    let frame = assets
        .prepare_gpu_frame(&[], std::slice::from_ref(&command), &[], &[])
        .unwrap();
    assert_eq!(frame.draws.len(), 1);
    let texture_key = frame.draw_texture_pair(0).0.to_owned();
    assert!(texture_key.starts_with("<system-font:"));
    let texture = frame.transient_textures.get(&texture_key).unwrap();
    assert_eq!(texture.height(), (binding.label_line_height + 4) as u32);
    let multiline = rasterize_system_label(binding, "A\nB", "LEFT", "TOP")
        .unwrap()
        .unwrap();
    assert_eq!(
        multiline.image.height(),
        (binding.label_line_height * 2 + 4) as u32
    );
    for text in [
        "A\rB",
        "A\r\nB",
        "A\u{000C}B",
        "A\u{0085}B",
        "A\u{2028}B",
        "A\u{2029}B",
    ] {
        let cocoa_multiline = rasterize_system_label(binding, text, "LEFT", "TOP")
            .unwrap()
            .unwrap();
        assert_eq!(
            cocoa_multiline.image.height(),
            (binding.label_line_height * 2 + 4) as u32,
            "separator in {text:?}"
        );
    }
    assert_eq!(multiline.vertical_anchor, 0);
    let baseline = rasterize_system_label(binding, "A", "LEFT", "BASELINE")
        .unwrap()
        .unwrap();
    assert_eq!(baseline.vertical_anchor, binding.ascending);
    assert_eq!(texture.upload_surface_format(), SurfaceFormat::A8B8G8R8);
    assert!(
        texture
            .image
            .pixels()
            .any(|pixel| pixel[3] > 0 && pixel[0] > pixel[1] && pixel[1] > pixel[2])
    );
    assert!(
        texture
            .image
            .pixels()
            .any(|pixel| pixel[3] > 0 && pixel[2] > pixel[1] && pixel[1] > pixel[0])
    );
    // CGBitmapContext is premultiplied-last; no stored color channel may
    // exceed the stored alpha after coverage is applied.
    assert!(
        texture
            .image
            .pixels()
            .all(|pixel| pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3])
    );

    let min_x = frame
        .vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .reduce(f32::min)
        .unwrap();
    let min_y = frame
        .vertices
        .iter()
        .map(|vertex| vertex.position[1])
        .reduce(f32::min)
        .unwrap();
    let max_x = frame
        .vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .reduce(f32::max)
        .unwrap();
    let max_y = frame
        .vertices
        .iter()
        .map(|vertex| vertex.position[1])
        .reduce(f32::max)
        .unwrap();
    assert_eq!(max_x - min_x, texture.width() as f32);
    assert_eq!(max_y - min_y, texture.height() as f32);
    // RIGHT subtracts the un-stroked NSString width. The leading stroke
    // margin therefore leaves x = origin + stroke - textureWidth.
    assert_eq!(min_x, 100.0 + 2.0 - texture.width() as f32);
    assert_eq!(
        min_y,
        200.0 - 2.0 - (binding.ascending + binding.descending) as f32
    );

    // The label is already pooled, so both commands below exercise the hit
    // branch. TOP=0 is unshifted; BASELINE=3 subtracts the ascender.
    let mut top_command = command.clone();
    top_command.horizontal_anchor = "LEFT".to_owned();
    top_command.vertical_anchor = "TOP".to_owned();
    let mut baseline_command = top_command.clone();
    baseline_command.order = baseline_command.order.wrapping_add(1);
    baseline_command.vertical_anchor = "BASELINE".to_owned();
    let anchored = assets
        .prepare_gpu_frame(&[], &[top_command, baseline_command], &[], &[])
        .unwrap();
    // Both pooled-label hits share texture/program/clip and therefore remain
    // one adjacent native GL batch, while their per-vertex draw indices keep
    // the distinct TOP and BASELINE transforms.
    assert_eq!(anchored.draws.len(), 1);
    assert_eq!(anchored.uniforms.len(), 2);
    let minimum_y = anchored
        .vertices
        .chunks_exact(6)
        .map(|vertices| {
            vertices
                .iter()
                .map(|vertex| vertex.position[1])
                .reduce(f32::min)
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(minimum_y[0] - minimum_y[1], binding.ascending as f32);
}

#[test]
fn system_text_rasterizes_each_coretext_fallback_face_with_its_own_em_scale() {
    let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    if runtime
        .execute_source(
            r#"
                res.createSystemFont(
                    "SYSTEM_FALLBACK", "ArialRoundedMTBold", 40,
                    255, 255, 255, 255, 0
                )
                res.useFont("SYSTEM_FALLBACK")
                res.drawString("MISSING_GROUP", "abc אבג 123", 10, 20)
            "#,
        )
        .is_err()
    {
        // Match UIKit's named-face dependency on non-Apple CI hosts.
        return;
    }
    let command = runtime.take_text_commands().remove(0);
    let TextFontBinding::System(binding) = command.font_binding.as_ref().unwrap() else {
        panic!("system font submission lost its native kind");
    };
    let layout = binding.native_system_font_layout("abc אבג 123").unwrap();
    assert_eq!(layout.faces.len(), 2);
    assert_eq!(layout.faces[0].family, "ArialRoundedMTBold");
    assert_eq!(layout.faces[1].family, "LucidaGrande");
    let fallback_left = layout.lines[0]
        .glyphs
        .iter()
        .filter(|glyph| glyph.face_slot == 1)
        .map(|glyph| glyph.x)
        .reduce(f64::min)
        .unwrap()
        .floor()
        .max(0.0) as u32;

    let label = rasterize_system_label(binding, "abc אבג 123", "LEFT", "TOP")
        .unwrap()
        .unwrap();
    assert_eq!(label.image.width(), layout.width as u32);
    assert_eq!(label.image.height(), binding.label_line_height as u32);
    assert!(
        label
            .image
            .enumerate_pixels()
            .any(|(x, _, pixel)| x >= fallback_left && pixel[3] != 0),
        "fallback run produced no vector coverage"
    );
}

#[test]
fn last_system_font_release_separates_same_hash_deferred_label_lifetimes() {
    let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSystemFont("SYSTEM", "Arial", 24, 255, 10, 20, 30)
                res.useFont("SYSTEM")
                res.drawString("MISSING_GROUP", "Same", 10, 20)
                res.releaseFont("SYSTEM")
                res.createSystemFont("SYSTEM", "Arial", 24, 255, 10, 20, 30)
                res.useFont("SYSTEM")
                res.drawString("MISSING_GROUP", "Same", 10, 20)
            "#,
        )
        .unwrap();
    let commands = runtime.take_text_commands();
    assert_eq!(commands.len(), 2);
    let epochs = commands
        .iter()
        .map(|command| match command.font_binding.as_ref().unwrap() {
            TextFontBinding::System(binding) => binding.label_pool_epoch,
            TextFontBinding::Bitmap { .. } => panic!("test font is SystemFont"),
        })
        .collect::<Vec<_>>();
    assert_eq!(epochs, [0, 1]);

    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
    };
    let frame = assets.prepare_gpu_frame(&[], &commands, &[], &[]).unwrap();
    assert_eq!(frame.draws.len(), 2);
    assert_ne!(frame.draw_texture_pair(0).0, frame.draw_texture_pair(1).0);
    let old_texture = frame.draw_texture_pair(0).0.to_owned();
    let active_texture = frame.draw_texture_pair(1).0.to_owned();
    assert!(frame.retired_textures.contains(&old_texture));
    assert_eq!(
        frame
            .transient_textures
            .keys()
            .filter(|key| key.starts_with("<system-font:"))
            .count(),
        2
    );

    // A texture retired later in the same native-order frame must remain
    // uploadable until every earlier deferred wgpu draw has consumed it.
    let mut renderer = GpuRenderer::headless(GameResolution::default()).unwrap();
    renderer
        .render_offscreen(&assets, &frame, [0, 0, 0])
        .unwrap();
    assert!(renderer.textures.contains_key(&old_texture));
    assert!(renderer.textures.contains_key(&active_texture));

    let next_frame = assets
        .prepare_gpu_frame(&[], std::slice::from_ref(&commands[1]), &[], &[])
        .unwrap();
    renderer
        .render_offscreen(&assets, &next_frame, [0, 0, 0])
        .unwrap();
    assert!(!renderer.textures.contains_key(&old_texture));
    assert!(renderer.textures.contains_key(&active_texture));
}
