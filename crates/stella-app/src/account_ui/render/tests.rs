//! Real raster/element-selection regressions using read-only native artwork.
//! No OS window, network endpoint, system clipboard, or player data is used.

use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use stella_script::{AccountFieldError, AccountUiSnapshot};

struct Fixture {
    root: PathBuf,
    runtime: StellaLua,
    painter: AccountPainter,
    ui: AccountUi,
}

impl Fixture {
    fn new() -> Option<Self> {
        let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
        if !shipped
            .join("skynestdata/fonts/OpenSans-Regular.ttf")
            .is_file()
        {
            eprintln!("native account raster regression requires runtime/data artwork; skipped");
            return None;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "stella-account-raster-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let data = root.join("data");
        let fonts = data.join("skynestdata/fonts");
        fs::create_dir_all(&fonts).unwrap();
        for name in ["OpenSans-Regular.ttf", "OpenSans-CondBold.ttf"] {
            fs::copy(
                shipped.join("skynestdata/fonts").join(name),
                fonts.join(name),
            )
            .unwrap();
        }
        let runtime = StellaLua::new(data).unwrap();
        runtime
            .execute_source("_G.SkynestAccount.native_login(true, false, false)")
            .unwrap();
        let mut ui = AccountUi::default();
        ui.sync(runtime.account_ui());
        let painter = AccountPainter::new(shipped, &runtime);
        Some(Self {
            root,
            runtime,
            painter,
            ui,
        })
    }

    fn paint(&mut self) -> RgbaImage {
        self.painter
            .paint(&self.runtime, &self.ui, 1024, 768, 0.0)
            .unwrap()
            .expect("UI changed before this render")
    }

    fn view(&mut self, view: AccountView) {
        self.ui.sync(Some(AccountUiSnapshot {
            id: self.ui.owner_id().unwrap(),
            view,
            busy: false,
            field_error: None,
        }));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn native_element(name: &str) -> &'static Element {
    layout::layout_for(AccountView::SignIn)
        .unwrap()
        .elements
        .iter()
        .find(|element| element.name == name)
        .unwrap()
}

fn pixels(image: &RgbaImage, rect: Rect) -> RgbaImage {
    image::imageops::crop_imm(
        image,
        rect.x as u32,
        rect.y as u32,
        rect.width as u32,
        rect.height as u32,
    )
    .to_image()
}

#[test]
fn delayed_password_error_renders_icon_with_normal_caps_then_submit_renders_red_caps_and_popup() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    fixture.ui.focus(Some(Field::Password));
    fixture.ui.text("short");
    fixture.ui.focus(None);
    let before = fixture.paint();
    let left = native_element("passwordTextFieldleftbox");
    let icon = native_element("passwordErrorButton");
    let textbox = native_element("passwordTextField");
    assert!(
        !fixture
            .painter
            .hit_regions
            .iter()
            .any(|(name, _)| *name == "passwordErrorButton")
    );
    assert!(
        !fixture
            .painter
            .images
            .contains_key(left.error_image.unwrap())
    );
    assert!(
        !fixture
            .painter
            .images
            .contains_key(textbox.error_image.unwrap())
    );

    fixture.ui.set_validation_clock(Duration::from_secs(2));
    fixture.ui.advance_validation(&fixture.runtime).unwrap();
    let delayed = fixture.paint();
    assert_eq!(pixels(&before, left.rect), pixels(&delayed, left.rect));
    assert_ne!(pixels(&before, icon.rect), pixels(&delayed, icon.rect));
    assert!(
        fixture
            .painter
            .hit_regions
            .iter()
            .any(|(name, _)| *name == "passwordErrorButton")
    );
    assert!(
        !fixture
            .painter
            .hit_regions
            .iter()
            .any(|(name, _)| *name == "errorPopup")
    );
    assert!(
        !fixture
            .painter
            .images
            .contains_key(left.error_image.unwrap())
    );
    assert!(
        !fixture
            .painter
            .images
            .contains_key(textbox.error_image.unwrap())
    );

    let mut submitted = fixture.ui.snapshot.clone().unwrap();
    submitted.field_error = Some(AccountFieldError {
        field: 19,
        message: 6,
    });
    fixture.ui.sync(Some(submitted));
    let submitted = fixture.paint();
    assert_ne!(pixels(&delayed, left.rect), pixels(&submitted, left.rect));
    assert!(
        fixture
            .painter
            .images
            .contains_key(left.error_image.unwrap())
    );
    assert!(
        fixture
            .painter
            .images
            .contains_key(textbox.error_image.unwrap())
    );
    assert!(
        fixture
            .painter
            .hit_regions
            .iter()
            .any(|(name, _)| *name == "passwordErrorButton")
    );
    assert!(
        fixture
            .painter
            .hit_regions
            .iter()
            .any(|(name, _)| *name == "errorPopup")
    );
}

#[test]
fn pointer_placement_in_already_focused_field_cancels_marked_text_without_stale_cache() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    fixture.ui.focus(Some(Field::Password));
    fixture.ui.text("committed");
    fixture.ui.preedit("撤销", Some((0, 6)));
    fixture.paint();
    // Mirrors a click in the existing focus, for which focus() is a no-op.
    fixture.ui.focus(Some(Field::Password));
    fixture
        .painter
        .place_cursor(
            &fixture.runtime,
            &mut fixture.ui,
            Field::Password,
            400.0,
            false,
        )
        .unwrap();
    assert!(fixture.ui.password.preedit.is_empty());
    fixture.view(AccountView::Help1);
    fixture.view(AccountView::SignIn);
    assert_eq!(fixture.ui.password.text(), "committed");
}

#[test]
fn diagnostic_opensans_leading_ascii_glyphs_at_984_by_738() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    // Only explicitly requested diagnostics write these synthetic, non-secret
    // specimens. Ordinary tests neither create screenshots nor inspect input.
    let destination = std::env::var_os("STELLA_ACCOUNT_GLYPH_DIAGNOSTIC").map(PathBuf::from);
    for size in [17, 18] {
        let font = fixture
            .runtime
            .platform_ui_font("OpenSans", size, [0, 0, 0, 255])
            .unwrap();
        for text in ["i", "u", "W", "invalid", "unused@example.invalid", "WXYZ"] {
            let layout = font.native_system_font_layout(text).unwrap();
            let raw = crate::assets::rasterize_system_label(&font, text, "LEFT", "TOP")
                .unwrap()
                .unwrap();
            let mut rendered = Pixmap::new(layout.width as u32, 38).unwrap();
            text_lines(
                &mut rendered,
                &font,
                text,
                Rect::new(0.0, 0.0, layout.width as f32, 38.0),
                0,
                1,
            )
            .unwrap();
            let raw_coverage: u64 = raw.image.pixels().map(|p| u64::from(p[3])).sum();
            let line_coverage: u64 = rendered.pixels().iter().map(|p| u64::from(p.alpha())).sum();
            assert_eq!(
                raw_coverage, line_coverage,
                "text_lines clipping: {size} {text}"
            );
            if let Some(path) = &destination {
                eprintln!(
                    "glyph diagnostic size={size} text={text:?} first={:?} raw={}x{} ink={raw_coverage}",
                    layout.lines[0].glyphs.first(),
                    raw.image.width(),
                    raw.image.height()
                );
                raw.image
                    .save(path.join(format!("{size}-{text}-raw.png")))
                    .unwrap();
                rendered
                    .save_png(path.join(format!("{size}-{text}-line.png")))
                    .unwrap();
            }
        }
    }
    for text in ["invalid", "unused@example.invalid", "WXYZ"] {
        fixture.ui.focus(Some(Field::Email));
        fixture.ui.email.select_all();
        fixture.ui.text(text);
        fixture.ui.focus(None);
        let image = fixture
            .painter
            .paint(&fixture.runtime, &fixture.ui, 984, 738, 0.0)
            .unwrap()
            .unwrap();
        let rect = fixture
            .painter
            .canvas
            .rect(native_element("emailTextField").rect);
        let field = pixels(&image, rect);
        if let Some(path) = &destination {
            image
                .save(path.join(format!("984-{text}-full.png")))
                .unwrap();
            let zoom = image::imageops::resize(
                &field,
                field.width() * 6,
                field.height() * 6,
                image::imageops::FilterType::Nearest,
            );
            zoom.save(path.join(format!("984-{text}-field-6x.png")))
                .unwrap();
        }
    }
}

#[test]
fn bordered_field_content_keeps_complete_leading_ascii_ink_above_the_caps() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    let destination = std::env::var_os("STELLA_ACCOUNT_GLYPH_DIAGNOSTIC").map(PathBuf::from);
    let element = native_element("emailTextField");
    for (width, height) in [(1024, 768), (984, 738)] {
        fixture.ui.focus(Some(Field::Email));
        fixture.ui.email.select_all();
        fixture.ui.text(" "); // No placeholder or ink: independent artwork-only background.
        fixture.ui.focus(None);
        let background = fixture
            .painter
            .paint(&fixture.runtime, &fixture.ui, width, height, 0.0)
            .unwrap()
            .unwrap();
        let content = fixture
            .painter
            .canvas
            .rect(layout::field_content_rect(AccountView::SignIn, element));
        let font = fixture
            .painter
            .font(
                &fixture.runtime,
                element.font_name,
                element.font_size,
                [0, 0, 0, 255],
            )
            .unwrap();
        for text in ["i", "u", "W", "invalid", "unused@example.invalid", "WXYZ"] {
            fixture.ui.focus(Some(Field::Email));
            fixture.ui.email.select_all();
            fixture.ui.text(text);
            fixture.ui.focus(None);
            let actual = fixture
                .painter
                .paint(&fixture.runtime, &fixture.ui, width, height, 0.0)
                .unwrap()
                .unwrap();
            let mut expected = Pixmap::from_vec(
                background.as_raw().clone(),
                tiny_skia::IntSize::from_wh(width, height).unwrap(),
            )
            .unwrap();
            let mut text_view =
                Pixmap::new(content.width.ceil() as u32, content.height.ceil() as u32).unwrap();
            text_lines(
                &mut text_view,
                &font,
                text,
                Rect::new(
                    0.0,
                    0.0,
                    font.native_string_width(text) as f32,
                    content.height,
                ),
                0,
                1,
            )
            .unwrap();
            // Independent oracle: overlay the complete glyph raster AFTER all
            // artwork. Native painter order is unchanged; the desktop content
            // box must prevent the later cap backgrounds from covering ink.
            expected.draw_pixmap(
                content.x.round() as i32,
                content.y.round() as i32,
                text_view.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
            let mismatched = actual
                .as_raw()
                .iter()
                .zip(expected.data())
                .filter(|(actual, expected)| actual != expected)
                .count();
            assert_eq!(
                mismatched, 0,
                "leading glyph covered: {width}x{height} {text:?}"
            );
            if let Some(path) = &destination {
                let field = pixels(&actual, fixture.painter.canvas.rect(element.rect));
                let zoom = image::imageops::resize(
                    &field,
                    field.width() * 6,
                    field.height() * 6,
                    image::imageops::FilterType::Nearest,
                );
                zoom.save(path.join(format!("fixed-{width}-{text}-field-6x.png")))
                    .unwrap();
            }
        }
    }
}

#[test]
fn cursor_hit_testing_uses_the_same_bordered_field_content_origin_as_drawing() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    for view in [
        AccountView::SignIn,
        AccountView::Register2,
        AccountView::ForgotPassword,
    ] {
        fixture.view(view);
        for field in [Field::Email, Field::Password] {
            if field == Field::Password && view == AccountView::ForgotPassword {
                continue;
            }
            fixture.ui.focus(Some(field));
            if field == Field::Email {
                fixture.ui.email_editor_mut().select_all();
            } else {
                fixture.ui.password.select_all();
            }
            fixture.ui.text("WXYZ");
            fixture
                .painter
                .paint(&fixture.runtime, &fixture.ui, 984, 738, 0.0)
                .unwrap()
                .unwrap();
            let name = if field == Field::Email {
                "emailTextField"
            } else {
                "passwordTextField"
            };
            let element = layout::layout_for(view)
                .unwrap()
                .elements
                .iter()
                .find(|element| element.name == name)
                .unwrap();
            let content = fixture
                .painter
                .canvas
                .rect(layout::field_content_rect(view, element));
            let font = fixture
                .painter
                .font(
                    &fixture.runtime,
                    element.font_name,
                    element.font_size,
                    [0, 0, 0, 255],
                )
                .unwrap();
            let first_advance =
                font.native_string_width(if field == Field::Password { "•" } else { "W" }) as f32;
            for (fraction, expected) in [(0.25, 0), (0.75, 1)] {
                fixture
                    .painter
                    .place_cursor(
                        &fixture.runtime,
                        &mut fixture.ui,
                        field,
                        content.x + first_advance * fraction,
                        false,
                    )
                    .unwrap();
                let editor = if field == Field::Email {
                    fixture.ui.email_editor()
                } else {
                    &fixture.ui.password
                };
                assert_eq!(
                    editor.cursor(),
                    expected,
                    "{view:?} {field:?} at {fraction}"
                );
            }
        }
    }
}
