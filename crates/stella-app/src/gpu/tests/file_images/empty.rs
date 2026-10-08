//! Glyph-free native draws never dereference or resolve an atlas Image.

use super::*;

fn command(text: &str) -> TextRenderCommand {
    TextRenderCommand {
        order: 0,
        text: text.to_owned(),
        font: "F".to_owned(),
        font_binding: None,
        x: 0.0,
        y: 0.0,
        native_system_origin: None,
        scale_x: 1.0,
        scale_y: 1.0,
        angle: 0.0,
        matrix: None,
        position_matrix: None,
        alpha: 1.0,
        horizontal_anchor: "RIGHT".to_owned(),
        vertical_anchor: "TOP".to_owned(),
        projection_3d: None,
        clip_rect: None,
    }
}

fn metadata_font(files: &Files) -> AssetCatalog {
    files.font();
    let mut catalog = assets(files);
    catalog.fonts.insert(
        "F".to_owned(),
        stella_assets::ka3d::BitmapFont::parse(&std::fs::read(files.data().join("F.dat")).unwrap())
            .unwrap(),
    );
    catalog
}

#[test]
fn missing_glyphs_and_empty_text_do_not_resolve_a_missing_atlas_on_the_gpu() {
    let files = Files::new();
    let mut catalog = metadata_font(&files);
    let commands = [command(""), command("?"), command("???")];
    let size = GameResolution {
        width: 4,
        height: 2,
    };
    let frame = catalog
        .prepare_gpu_frame_at_resolution(size, &[], &commands, &[], &[])
        .unwrap();
    assert!(frame.vertices.is_empty());
    assert!(!catalog.textures.contains_key("same.png"));
    let mut renderer = GpuRenderer::headless(size).unwrap();
    assert_eq!(
        renderer
            .render_to_rgba(&catalog, &frame, [0, 255, 0])
            .unwrap(),
        [0, 255, 0, 255].repeat(8)
    );
}

#[test]
fn glyph_free_reference_draw_keeps_pixels_without_resolving_an_atlas() {
    let files = Files::new();
    let mut catalog = metadata_font(&files);
    let mut pixels = vec![0x123456_u32; 8];
    for text in ["", "?", "???"] {
        catalog.draw_text(&command(text), &mut pixels).unwrap();
    }
    assert_eq!(pixels, [0x123456; 8]);
    assert!(!catalog.textures.contains_key("same.png"));
}

#[test]
fn empty_native_font_draws_leave_the_actual_framebuffer_unchanged() {
    let files = Files::new();
    std::fs::write(
        files.data().join("F.dat"),
        envelope(b"JUNK", b"ignored".to_vec()),
    )
    .unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','missing',0,0,'LEFT','TOP'); res.drawString('','',0,0,'RIGHT','TOP'); res.drawString('','x',0,0,'RIGHT','TOP'); res.drawString('','x',0,0,'HCENTER','TOP'); drawString3D('','missing',1,2,3,0.25,0,0,0.5)").unwrap();
    assert_eq!(
        pixels(&runtime, &mut assets(&files)),
        [0, 255, 0, 255].repeat(8)
    );
}

#[test]
fn empty_sheet_reload_preserves_queued_pixels_and_restores_shadowed_sprites() {
    let files = Files::new();
    files.sheet("B");
    std::fs::copy(files.data().join("B.dat"), files.data().join("A.dat")).unwrap();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('B.dat')")
        .unwrap();
    files.image(NEW);
    runtime
        .execute_source("res.createSpriteSheet('A.dat'); res.drawSprite('B',0,0)")
        .unwrap();
    std::fs::write(
        files.data().join("A.dat"),
        envelope(b"JUNK", b"ignored".to_vec()),
    )
    .unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat',true); res.drawSprite('B',2,0)")
        .unwrap();
    std::fs::remove_file(files.data().join("same.png")).unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), NEW, OLD);
}
