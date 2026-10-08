//! Real glyph pixels after native font selection and UI draw boundaries.

use super::*;

#[test]
fn selected_font_release_does_not_keep_selection_alive_via_queued_glyph_data() {
    let files = Files::new();
    files.font();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('', 'A', 0, 0); res.releaseFont('F')").unwrap();
    files.image(NEW);
    runtime
        .execute_source("res.createBitmapFont('F.dat')")
        .unwrap();
    let error = runtime
        .execute_source("res.drawString('', 'A', 2, 0)")
        .unwrap_err();
    assert!(error.to_string().contains("released IFont"));
    runtime
        .execute_source("res.useFont('F'); res.drawString('', 'A', 2, 0)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, NEW);
}

#[test]
fn selected_font_ui_ignores_inherited_font_and_preserves_current_glyph_pixels() {
    let files = Files::new();
    files.font();
    std::fs::copy(files.data().join("F.dat"), files.data().join("ALT.dat")).unwrap();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createBitmapFont('F.dat'); res.useFont('F')")
        .unwrap();
    files.image(NEW);
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('ALT.dat')
        local text=setmetatable({visible=true,x=0,y=0,scaleX=1,scaleY=1,width=2,
            hanchor='LEFT',vanchor='TOP',group='',text='A'}, {
            __index=function(_, key) if key=='font' then return 'ALT' end end
        })
        drawUITextNative(text, 0, 0)
        text.font='ALT'; text.x=2
        drawUITextNative(text, 0, 0)
    "#,
        )
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, NEW);
}

#[test]
fn selected_font_ui_clipped_error_preserves_alpha_in_the_next_glyph_pixels() {
    let files = Files::new();
    files.font();
    let color = [200, 31, 8, 255];
    files.image(color);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('', 'A', 0, 0)
        assert(not pcall(drawUITextNative, {
            visible=true,x=0,y=0,scaleX=1,scaleY=1,font='F',clipped=true,
            lines={{draw=function() error('stop') end}}
        }, 0, 0, 1, 1, 0, 0.5))
        res.drawString('', 'A', 2, 0)
    "#,
        )
        .unwrap();
    assert_halves(
        &pixels(&runtime, &mut assets(&files)),
        color,
        [100, 143, 4, 255],
    );
}

#[test]
fn selected_font_ui_raw_lua51_fields_draw_the_requested_glyph_position() {
    let files = Files::new();
    files.font();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('', 'A', 0, 0)
        drawUITextNative({visible=0,x='2',y=false,scaleX='1',scaleY=1,
            font='F',group='',text='A'}, 0, 0)
    "#,
        )
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, OLD);
}
