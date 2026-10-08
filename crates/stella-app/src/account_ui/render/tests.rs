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

pub(crate) fn native_account_window_color_fixture() -> Option<RgbaImage> {
    let mut fixture = Fixture::new()?;
    // Keep original English strings independent of the host's system locale.
    fixture.painter.strings = super::super::strings::Strings::default();
    fixture.ui.focus(Some(Field::Email));
    fixture.ui.text("synthetic-color-check@example.invalid");
    fixture.ui.focus(None);
    Some(fixture.paint())
}

#[test]
fn native_account_autoshrink_preserves_fit_and_minimum_size() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    let element = layout::layout_for(AccountView::Register1)
        .unwrap()
        .elements
        .iter()
        .find(|element| element.name == "dobLabel")
        .unwrap();
    let width = fixture.painter.canvas.rect(element.rect).width;
    let normal = fixture
        .painter
        .font(
            &fixture.runtime,
            element.font_name,
            element.font_size,
            element.color,
        )
        .unwrap();
    let short = fixture
        .painter
        .fitting_font(&fixture.runtime, element, "DOB", width, element.color)
        .unwrap();
    assert_eq!(short.size, normal.size);

    let mut overflow = String::new();
    while normal.native_string_width(&overflow) as f32 <= width {
        overflow.push('W');
    }
    let fitted = fixture
        .painter
        .fitting_font(&fixture.runtime, element, &overflow, width, element.color)
        .unwrap();
    assert!(fitted.size < normal.size);
    assert!(fitted.native_string_width(&overflow) as f32 <= width);

    let impossible = "W".repeat(100);
    let minimum = fixture
        .painter
        .fitting_font(&fixture.runtime, element, &impossible, width, element.color)
        .unwrap();
    assert_eq!(minimum.size, 14);
    assert!(minimum.native_string_width(&impossible) as f32 > width);

    let sign_in = layout::layout_for(AccountView::SignIn).unwrap();
    let field = sign_in
        .elements
        .iter()
        .find(|element| element.name == "emailTextField")
        .unwrap();
    let field_width = fixture
        .painter
        .canvas
        .rect(layout::field_content_rect(AccountView::SignIn, field))
        .width;
    let field_color = [0, 0, 0, 255];
    let base = fixture
        .painter
        .font(
            &fixture.runtime,
            field.font_name,
            field.font_size,
            field_color,
        )
        .unwrap();
    let mut email = String::new();
    while base.native_string_width(&email) as f32 <= field_width {
        email.push('W');
    }
    let fitted_field = fixture
        .painter
        .fitting_font(&fixture.runtime, field, &email, field_width, field_color)
        .unwrap();
    assert_eq!(fitted_field.size, 17);
    assert!(fitted_field.native_string_width(&email) as f32 <= field_width);
    fixture.ui.focus(Some(Field::Email));
    fixture.ui.text(&email);
    fixture.ui.focus(None);
    let rendered = fixture.paint();
    assert_eq!(rendered.dimensions(), (1024, 768));
}

#[test]
fn nib_label_shadow_draws_above_text_inside_original_label_bounds() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    let element = native_element("forgotPasswordLabel");
    assert_eq!(
        element.text_shadow,
        Some(([255, 255, 255, 255], [0.0, -1.0]))
    );
    assert_eq!(
        native_element("signInButton").text_shadow,
        Some(([128, 128, 128, 255], [0.0, -1.0]))
    );
    assert!(native_element("emailTextField").text_shadow.is_none());
    let font = fixture
        .painter
        .font(
            &fixture.runtime,
            element.font_name,
            element.font_size,
            element.color,
        )
        .unwrap();
    let mut shadow_font = font.clone();
    shadow_font.fill_rgba = [255; 4];
    let rect = Rect::new(25.0, 25.0, 180.0, 32.0);
    let mut plain = Pixmap::new(240, 100).unwrap();
    let mut shadowed = Pixmap::new(240, 100).unwrap();
    text_lines(&mut plain, &font, "Shadow", rect, 0, 1).unwrap();
    text_lines_with_shadow(
        &mut shadowed,
        &font,
        "Shadow",
        rect,
        0,
        1,
        Some((&shadow_font, [0.0, -1.0])),
    )
    .unwrap();
    let mut shadow_only = false;
    for y in 0..100usize {
        for x in 0..240usize {
            let alpha = (y * 240 + x) * 4 + 3;
            let new_alpha = shadowed.data()[alpha];
            if plain.data()[alpha] == 0 && new_alpha != 0 {
                shadow_only = true;
            }
            if !(25..205).contains(&x) || !(25..57).contains(&y) {
                assert_eq!(new_alpha, 0, "shadow escaped UILabel clipping bounds");
            }
        }
    }
    assert!(shadow_only, "native one-point offset must add visible ink");
}

#[test]
fn nib_label_break_modes_preserve_clipping_and_unicode_tail_prefixes() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    let font = fixture
        .painter
        .font(&fixture.runtime, "OpenSans", 21.0, [0, 0, 0, 255])
        .unwrap();
    let clipped = layout::layout_for(AccountView::AccountNotVerified)
        .unwrap()
        .elements
        .iter()
        .find(|element| element.name == "verificationEmailSent")
        .unwrap();
    let text = "Long translated account message ".repeat(6);
    let style = TextLayout {
        alignment: clipped.alignment,
        max_lines: clipped.max_lines,
        line_break: clipped.line_break,
    };
    // The native two-line count permits explicit newlines; mode 2 never
    // introduces soft wraps or an ellipsis simply because the line is long.
    assert_eq!(
        drawing::label_lines(&font, &text, 120.0, style).as_slice(),
        std::slice::from_ref(&text)
    );
    assert_eq!(
        drawing::label_lines(&font, "first\nsecond\nthird", 120.0, style),
        ["first", "second"]
    );
    let word_style = TextLayout {
        line_break: LineBreak::WordWrap,
        ..style
    };
    let wrapped = drawing::label_lines(&font, &text, 120.0, word_style);
    assert_eq!(wrapped.len(), 2);
    assert!(wrapped.iter().all(|line| !line.contains('…')));
    let tailed = drawing::label_lines(
        &font,
        &text,
        120.0,
        TextLayout {
            line_break: LineBreak::TruncateTail,
            ..style
        },
    );
    assert_eq!(tailed.len(), 2);
    assert!(tailed[1].ends_with('…'));

    // An unbroken address is soft-wrapped without inserting a space between
    // clusters. Its final visible line must still be a prefix of the source.
    let address = format!("{}@example.invalid", "e\u{301}👩‍🚀🇨🇳👍🏽".repeat(12));
    for max_lines in [1, 2] {
        let lines = drawing::label_lines(
            &font,
            &address,
            160.0,
            TextLayout {
                max_lines,
                line_break: LineBreak::TruncateTail,
                ..style
            },
        );
        let retained = lines.join("");
        let prefix = retained.strip_suffix('…').unwrap();
        assert!(address.starts_with(prefix));
        assert!(
            address
                .grapheme_indices(true)
                .any(|(at, _)| at == prefix.len()),
            "truncation must end at a complete source grapheme"
        );
        assert!(
            lines
                .iter()
                .all(|line| font.native_string_width(line) <= 160)
        );
    }
}

#[test]
fn retained_email_labels_render_a_fitting_tail_inside_their_native_height() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    let destination = std::env::var_os("STELLA_ACCOUNT_LABEL_DIAGNOSTIC").map(PathBuf::from);
    let address = format!("{}@example.invalid", "We\u{301}👩‍🚀🇨🇳".repeat(40));
    for (view, name) in [
        (AccountView::AccountNotVerified, "verificationEmail"),
        (AccountView::ThanksForRegistering, "registrationEmail"),
    ] {
        fixture.ui.email.select_all();
        fixture.ui.email.replace("");
        fixture.view(view);
        fixture.ui.dirty();
        let background = fixture.paint();
        let element = layout::layout_for(view)
            .unwrap()
            .elements
            .iter()
            .find(|element| element.name == name)
            .unwrap();
        let font = fixture
            .painter
            .font(
                &fixture.runtime,
                element.font_name,
                element.font_size,
                element.color,
            )
            .unwrap();
        assert!(element.rect.height < (font.label_line_height * 2) as f32);
        // Independent raster oracle: retain the longest whole-grapheme
        // prefix whose complete glyph run plus ellipsis fits the label.
        let mut prefix = String::new();
        for cluster in address.graphemes(true) {
            if font.native_string_width(&format!("{prefix}{cluster}…")) as f32 > element.rect.width
            {
                break;
            }
            prefix.push_str(cluster);
        }
        prefix.push('…');
        let mut expected = Pixmap::from_vec(
            background.as_raw().clone(),
            tiny_skia::IntSize::from_wh(1024, 768).unwrap(),
        )
        .unwrap();
        text_lines(
            &mut expected,
            &font,
            &prefix,
            element.rect,
            element.alignment,
            1,
        )
        .unwrap();
        fixture.ui.email.replace(&address);
        fixture.ui.dirty();
        let actual = fixture.paint();
        assert!(
            actual.as_raw() == expected.data(),
            "{name} tail raster differs"
        );
        if let Some(path) = &destination {
            actual.save(path.join(format!("{name}-long.png"))).unwrap();
            background
                .save(path.join(format!("{name}-empty.png")))
                .unwrap();
        }
    }
}

#[test]
fn error_popup_measures_the_native_constraint_independently_of_its_two_visible_lines() {
    let Some(mut fixture) = Fixture::new() else {
        return;
    };
    let font = fixture
        .painter
        .font(
            &fixture.runtime,
            "OpenSans",
            layout::ERROR_POPUP_FONT_SIZE,
            [255; 4],
        )
        .unwrap();
    let line_height = font.label_line_height as f32;
    let text = "First\nSecond\nThird";
    let mut actual = Pixmap::new(1024, 768).unwrap();
    let rect = fixture
        .painter
        .error_popup(
            &mut actual,
            &fixture.runtime,
            Rect::new(670.0, 280.0, 30.0, 30.0),
            text,
        )
        .unwrap();
    assert_eq!(rect.width, 312.0);
    assert_eq!(rect.height, line_height * 3.0 + 30.0);
    let mut expected = Pixmap::new(1024, 768).unwrap();
    let artwork = fixture
        .painter
        .image(layout::ERROR_POPUP_IMAGE)
        .unwrap()
        .clone();
    drawing::draw_stretched(
        &mut expected,
        &artwork,
        rect,
        layout::ERROR_POPUP_CAPS,
        1.0,
        fixture.painter.image_densities[layout::ERROR_POPUP_IMAGE],
    );
    let inset = layout::ERROR_POPUP_TEXT_TOP_INSET;
    text_lines(
        &mut expected,
        &font,
        "First\nSecond",
        Rect::new(rect.x, rect.y + inset, rect.width, rect.height - inset),
        1,
        2,
    )
    .unwrap();
    assert_eq!(actual.data(), expected.data());
    let many = "One\nTwo\nThree\nFour\nFive\nSix\nSeven";
    let measured = drawing::word_wrap_size(&font, many, layout::ERROR_POPUP_TEXT_CONSTRAINT);
    let capacity = (layout::ERROR_POPUP_TEXT_CONSTRAINT[1] / line_height).floor();
    assert_eq!(measured[1], capacity * line_height);
    assert!(measured[1] > line_height * 2.0);
    assert_eq!(
        drawing::word_wrap_size(&font, "First", [330.0, 1.0])[1],
        line_height
    );
    if let Some(path) = std::env::var_os("STELLA_ACCOUNT_LABEL_DIAGNOSTIC") {
        actual
            .save_png(PathBuf::from(path).join("error-popup-three-measured-two-visible.png"))
            .unwrap();
    }
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
