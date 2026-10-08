//! FONT's raw glyph pointers must not follow a later private SpriteSheet.

use super::*;

struct Files(PathBuf);

impl Files {
    fn new() -> Self {
        let serial = NEXT_TEST_SPRITE_SHEET_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("stella-font-atlas-{}-{serial}", std::process::id()));
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join("appdata")).unwrap();
        for name in ["first.pvr", "last.pvr"] {
            fs::write(root.join("data").join(name), test_rgba_pvr(16, 8, [255; 4])).unwrap();
        }
        Self(root)
    }

    fn font(&self, same_image: bool, final_glyph: Option<u16>) {
        let mut bytes = font_record("first.pvr", Some(u16::from(b'A')), 3, 7, 5, 4, 3);
        let second = font_record(
            if same_image { "first.pvr" } else { "last.pvr" },
            final_glyph,
            2,
            4,
            1,
            2,
            1,
        );
        bytes.extend_from_slice(&second[8..]);
        let size = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&size.to_be_bytes());
        fs::write(self.0.join("data/F.dat"), bytes).unwrap();
    }

    fn runtime(&self) -> StellaLua {
        let runtime = StellaLua::new(self.0.join("data")).unwrap();
        runtime
            .execute_source("res.createBitmapFont('F.dat'); res.useFont('F')")
            .unwrap();
        runtime
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn font_record(
    texture: &str,
    character: Option<u16>,
    width: i16,
    height: i16,
    pivot: i16,
    leading: i16,
    tracking: i16,
) -> Vec<u8> {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string(texture));
    payload.extend_from_slice(&leading.to_be_bytes());
    payload.extend_from_slice(&tracking.to_be_bytes());
    payload.extend_from_slice(&u16::from(character.is_some()).to_be_bytes());
    if let Some(character) = character {
        payload.extend_from_slice(&character.to_be_bytes());
        for field in [0_i16, 0, width, height, pivot] {
            payload.extend_from_slice(&field.to_be_bytes());
        }
    }
    test_ka3d(b"FONT", &payload)
}

fn assert_retired(error: impl std::fmt::Display) {
    let message = error.to_string();
    assert!(
        message.contains("released") && message.contains("glyph"),
        "{message}"
    );
}

#[test]
fn later_font_record_retires_old_glyph_but_keeps_cached_metrics_and_current_font() {
    let files = Files::new();
    files.font(false, Some(u16::from(b'B')));
    let runtime = files.runtime();
    assert_retired(
        runtime
            .execute_source("res.getStringWidth('A')")
            .unwrap_err(),
    );
    runtime.execute_source("assert(res.getStringWidth('B')==2); assert(res.getStringWidth('')==0); assert(res.getStringWidth('?')==0); assert(res.getFontMaxAscending()==5); assert(res.getFontMaxDescending()==3); assert(res.getFontHeight()==8); assert(res.getFontLeading()==2); assert(res.getFontTracking()==1)").unwrap();
    assert!(
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .current_native_font()
            .unwrap()
            .is_some()
    );
}

#[test]
fn repeated_filename_does_not_resurrect_first_private_atlas_glyph() {
    let files = Files::new();
    files.font(true, Some(u16::from(b'B')));
    let runtime = files.runtime();
    assert_retired(
        runtime
            .execute_source("res.getStringWidth('A')")
            .unwrap_err(),
    );
    runtime
        .execute_source("assert(res.getStringWidth('B')==2)")
        .unwrap();
}

#[test]
fn zero_glyph_final_font_record_still_releases_the_previous_private_atlas() {
    let files = Files::new();
    files.font(false, None);
    let runtime = files.runtime();
    assert_retired(
        runtime
            .execute_source("res.drawString('','A',0,0)")
            .unwrap_err(),
    );
    assert!(runtime.take_text_commands().is_empty());
    runtime.execute_source("assert(res.getFontMaxAscending()==5); assert(res.getFontHeight()==7); assert(res.getFontLeading()==2); assert(res.getStringWidth('?')==0)").unwrap();
}

#[test]
fn left_draw_keeps_live_prefix_and_last_glyph_pivot_before_retired_pointer_error() {
    let files = Files::new();
    files.font(false, Some(u16::from(b'B')));
    let runtime = files.runtime();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.state.pivot_x = 13.0;
        bridge.state.pivot_y = 19.0;
    }
    assert_retired(
        runtime
            .execute_source("res.drawString('','BB?A',14,17)")
            .unwrap_err(),
    );
    let commands = runtime.take_text_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].text, "BB?");
    assert_eq!(commands[0].order, 0);
    let bridge = runtime.render.lock().unwrap();
    assert_eq!((bridge.state.pivot_x, bridge.state.pivot_y), (10.0, 15.0));
    drop(bridge);
    runtime
        .execute_source("res.drawString('','B',0,0)")
        .unwrap();
    assert_eq!(runtime.take_text_commands()[0].order, 1);
}

#[test]
fn centered_and_right_draw_fail_at_width_before_submitting_any_prefix() {
    for anchor in ["HCENTER", "RIGHT"] {
        let files = Files::new();
        files.font(false, Some(u16::from(b'B')));
        let runtime = files.runtime();
        {
            let mut bridge = runtime.render.lock().unwrap();
            bridge.state.pivot_x = 13.0;
            bridge.state.pivot_y = 19.0;
        }
        assert_retired(
            runtime
                .execute_source(&format!("res.drawString('','BA',0,0,'{anchor}')"))
                .unwrap_err(),
        );
        assert!(runtime.take_text_commands().is_empty());
        let bridge = runtime.render.lock().unwrap();
        assert_eq!((bridge.state.pivot_x, bridge.state.pivot_y), (13.0, 19.0));
    }
}

#[test]
fn clipped_result_is_unchanged_when_bitmap_width_dereferences_retired_glyph() {
    let files = Files::new();
    files.font(false, Some(u16::from(b'B')));
    let runtime = files.runtime();
    runtime
        .execute_source(
            "clippedText.lines={'sentinel'}; clippedText.widestLine=77; oldLines=clippedText.lines",
        )
        .unwrap();
    assert_retired(runtime.execute_source("clipText('','A',100)").unwrap_err());
    runtime.execute_source("assert(clippedText.lines==oldLines and clippedText.lines[1]=='sentinel' and clippedText.widestLine==77)").unwrap();
}

#[test]
fn ordinary_ui_retired_glyph_error_keeps_installed_alpha_and_live_prefix() {
    let files = Files::new();
    files.font(false, Some(u16::from(b'B')));
    let runtime = files.runtime();
    assert_retired(runtime.execute_source("drawUITextNative({visible=true,x=0,y=0,scaleX=2,scaleY=3,width=8,font='F',hanchor='LEFT',vanchor='TOP',group='',text='BA'},0,0,1,1,0,0.5)").unwrap_err());
    let commands = runtime.take_text_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].text, "B");
    assert_eq!(commands[0].alpha, 0.5);
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        (
            bridge.state.alpha,
            bridge.state.scale_x,
            bridge.state.scale_y,
            bridge.state.pivot_y
        ),
        (0.5, 2.0, 3.0, -4.0)
    );
}

#[test]
fn projected_retired_glyph_error_keeps_custom_model_and_live_prefix() {
    let files = Files::new();
    files.font(false, Some(u16::from(b'B')));
    let runtime = files.runtime();
    assert_retired(
        runtime
            .execute_source("drawString3D('','BA',1,2,3,0.25,0,0,0.5)")
            .unwrap_err(),
    );
    let commands = runtime.take_text_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].text, "B");
    assert!(commands[0].projection_3d.unwrap().custom_model);
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.perspective_projection);
    assert_eq!(bridge.state.custom_model.unwrap().x, 1.0);
    assert_eq!(bridge.state.alpha, 0.5);
}

#[test]
fn later_replacement_of_same_codepoint_is_live_and_preserves_metric_history() {
    let files = Files::new();
    files.font(false, Some(u16::from(b'A')));
    let runtime = files.runtime();
    runtime.execute_source("assert(res.getStringWidth('A')==2); assert(res.getFontMaxAscending()==5); res.drawString('','AA',0,0)").unwrap();
    let commands = runtime.take_text_commands();
    assert_eq!(commands[0].text, "AA");
    let Some(TextFontBinding::Bitmap { texture_source, .. }) = commands[0].font_binding.as_ref()
    else {
        panic!("bitmap font expected")
    };
    assert!(texture_source.ends_with("last.pvr"));
}

#[test]
fn missing_only_prefix_does_not_submit_a_draw_or_change_the_pivot() {
    let files = Files::new();
    files.font(false, Some(u16::from(b'B')));
    let runtime = files.runtime();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.state.pivot_x = 13.0;
        bridge.state.pivot_y = 19.0;
    }
    assert_retired(
        runtime
            .execute_source("res.drawString('','??A',14,17)")
            .unwrap_err(),
    );
    assert!(runtime.take_text_commands().is_empty());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!((bridge.state.pivot_x, bridge.state.pivot_y), (13.0, 19.0));
    drop(bridge);
    runtime
        .execute_source("res.drawString('','B',0,0)")
        .unwrap();
    assert_eq!(runtime.take_text_commands()[0].order, 0);
}
