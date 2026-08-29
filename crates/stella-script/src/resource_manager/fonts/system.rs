//! Cross-platform SystemFont implementation of Purple's UIKit-backed object.

use std::sync::{Arc, OnceLock};

use mlua::Result as LuaResult;
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
};

use super::FontMetric;
use crate::{SystemFontFallbackCatalog, SystemFontRenderBinding, native_fcvtzs_f32, runtime_error};

static PLATFORM_SYSTEM_FONTS: OnceLock<PlatformSystemFonts> = OnceLock::new();

/// The shipped iOS profile selects this UIKit face before querying the host
/// operating system. Windows and Linux do not normally provide the Apple
/// PostScript name, so the desktop rehost must resolve that one known profile
/// face to the platform's closest bold sans-serif face.
const IOS_MENU_SYSTEM_FONT: &str = "ArialRoundedMTBold";

#[derive(Debug)]
struct PlatformSystemFonts {
    database: Arc<fontdb::Database>,
    fallback_catalog: SystemFontFallbackCatalog,
    names: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SystemFontState {
    render: SystemFontRenderBinding,
}

fn platform_system_fonts() -> &'static PlatformSystemFonts {
    PLATFORM_SYSTEM_FONTS.get_or_init(|| {
        // UIKit returns face/PostScript names after traversing font families.
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        #[cfg(target_os = "macos")]
        database.load_fonts_dir("/System/Library/AssetsV2/com_apple_MobileAsset_Font8");
        let names = native_available_font_names(database.faces().filter_map(|face| {
            face.families
                .first()
                .map(|(family, _)| (family.as_str(), face.post_script_name.as_str()))
        }));
        let database = Arc::new(database);
        let fallback_catalog = SystemFontFallbackCatalog::new(database.clone());
        PlatformSystemFonts {
            database,
            fallback_catalog,
            names,
        }
    })
}

fn native_available_font_names<'a>(
    faces: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    let faces = faces.into_iter().collect::<Vec<_>>();
    let mut families = Vec::new();
    for (family, _) in &faces {
        if !families.contains(family) {
            families.push(*family);
        }
    }
    let mut names = Vec::new();
    for family in families {
        names.extend(
            faces
                .iter()
                .filter(|(candidate, _)| *candidate == family)
                .map(|(_, post_script_name)| (*post_script_name).to_owned()),
        );
    }
    names
}

pub(crate) fn platform_system_font_names() -> &'static [String] {
    &platform_system_fonts().names
}

fn native_float_to_i32(value: f64) -> i32 {
    // LuaState::getFloat first narrows to S-register precision, then FCVTZS.
    native_fcvtzs_f32(value as f32)
}

fn native_double_to_i32(value: f64) -> i32 {
    // UIFont metrics and NSString sizes return CGFloat (double on this ARM64
    // build). The constructor converts D0 directly with FCVTZS; narrowing to
    // f32 first can cross an integer boundary before truncation.
    if !value.is_finite() || !(-2_147_483_648.0_f64..2_147_483_648.0_f64).contains(&value) {
        i32::MIN
    } else {
        value.trunc() as i32
    }
}

fn named_system_font_face(database: &fontdb::Database, family: &str) -> Option<fontdb::ID> {
    database
        .faces()
        .find(|face| face.post_script_name == family)
        .map(|face| face.id)
        .or_else(|| {
            let families = [fontdb::Family::Name(family)];
            database.query(&fontdb::Query {
                families: &families,
                ..fontdb::Query::default()
            })
        })
}

fn ios_menu_system_font_compatibility_face(database: &fontdb::Database) -> Option<fontdb::ID> {
    // Arial is the corresponding family selected by Purple's Windows font
    // profile. The remaining names cover common Linux installations before
    // delegating to fontdb's platform-configured generic sans-serif family.
    let families = [
        fontdb::Family::Name("Arial"),
        fontdb::Family::Name("Liberation Sans"),
        fontdb::Family::Name("DejaVu Sans"),
        fontdb::Family::Name("Noto Sans"),
        fontdb::Family::SansSerif,
    ];
    database
        .query(&fontdb::Query {
            families: &families,
            weight: fontdb::Weight::BOLD,
            ..fontdb::Query::default()
        })
        .or_else(|| {
            // A minimal host may only install a regular sans face. Starting
            // remains preferable to rejecting the iOS-only PostScript name.
            database.query(&fontdb::Query {
                families: &families,
                ..fontdb::Query::default()
            })
        })
}

fn resolve_system_font_face(database: &fontdb::Database, family: &str) -> Option<fontdb::ID> {
    named_system_font_face(database, family).or_else(|| {
        (family == IOS_MENU_SYSTEM_FONT)
            .then(|| ios_menu_system_font_compatibility_face(database))
            .flatten()
    })
}

/// Reproduce `sub_1004471B4` / `sub_100447480` and `sub_100477DD0`.
/// Lua exposes the four channels as A,R,G,B. Each value first narrows to f32,
/// then FCVTZS's without an explicit clamp; the packed bytes are finally
/// normalized inside the native Color constructor.
pub(crate) fn system_font_color_from_lua(argb: [f32; 4]) -> [u8; 4] {
    let [alpha, red, green, blue] = argb.map(native_fcvtzs_f32);
    let packed = (red as u32).wrapping_shl(16)
        | (alpha as u32).wrapping_shl(24)
        | (green as u32).wrapping_shl(8)
        | blue as u32;
    [
        (packed >> 16) as u8,
        (packed >> 8) as u8,
        packed as u8,
        (packed >> 24) as u8,
    ]
}

pub(crate) fn create_system_font_state(
    label_pool_epoch: u64,
    family: &str,
    size: f64,
    fill_rgba: [u8; 4],
    stroke_width: f64,
    stroke_rgba: [u8; 4],
    style: f64,
) -> LuaResult<SystemFontState> {
    let fonts = platform_system_fonts();
    let face_id = resolve_system_font_face(&fonts.database, family)
        .ok_or_else(|| runtime_error(format!("Font {family} is not available.")))?;

    let size = native_float_to_i32(size);
    let resolved = fonts
        .database
        .with_face_data(face_id, |data, face_index| {
            let face = FontRef::from_index(data, face_index).ok()?;
            let metrics = face.metrics(Size::unscaled(), LocationRef::default());
            let scale = f64::from(size) / f64::from(metrics.units_per_em);
            let ascending = f64::from(metrics.ascent) * scale;
            let descending = -f64::from(metrics.descent) * scale;
            let leading = f64::from(metrics.leading) * scale;
            Some((
                std::sync::Arc::<[u8]>::from(data),
                face_index,
                native_double_to_i32(ascending),
                native_double_to_i32(descending),
                native_double_to_i32(leading),
                native_double_to_i32(ascending + descending + leading),
            ))
        })
        .flatten()
        .ok_or_else(|| runtime_error(format!("Font {family} is not available.")))?;

    let style = native_float_to_i32(style);
    if style != 0 {
        let style_name = match style {
            0 => "Normal",
            1 => "Bold",
            2 => "Italic",
            _ => "Unknown",
        };
        return Err(runtime_error(format!(
            "Style {style_name} is not available for font {family}."
        )));
    }

    Ok(SystemFontState {
        render: SystemFontRenderBinding {
            label_pool_epoch,
            family: family.to_owned(),
            font_data: resolved.0,
            face_index: resolved.1,
            fallback_catalog: Some(fonts.fallback_catalog.clone()),
            size,
            fill_rgba,
            stroke_width: native_float_to_i32(stroke_width),
            stroke_rgba,
            style,
            ascending: resolved.2,
            descending: resolved.3,
            leading: resolved.4,
            label_line_height: resolved.5,
        },
    })
}

impl SystemFontState {
    pub(crate) fn render_binding(&self) -> SystemFontRenderBinding {
        self.render.clone()
    }
}

pub(crate) fn system_font_string_width(font: &SystemFontState, text: &str) -> i32 {
    font.render.native_string_width(text)
}

pub(crate) fn system_font_metric(font: &SystemFontState, metric: FontMetric) -> f64 {
    match metric {
        FontMetric::MaxAscending => f64::from(font.render.ascending),
        FontMetric::MaxDescending => f64::from(font.render.descending),
        FontMetric::Leading => f64::from(font.render.leading),
        FontMetric::Tracking => 0.0,
        FontMetric::Height => f64::from(font.render.ascending.wrapping_add(font.render.descending)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn metric_state(ascending: i32, descending: i32) -> SystemFontState {
        SystemFontState {
            render: SystemFontRenderBinding {
                label_pool_epoch: 0,
                family: "Test".to_owned(),
                font_data: Arc::from([]),
                face_index: 0,
                fallback_catalog: None,
                size: 1,
                fill_rgba: [0, 0, 0, 255],
                stroke_width: 0,
                stroke_rgba: [0, 0, 0, 255],
                style: 0,
                ascending,
                descending,
                leading: 0,
                label_line_height: 1,
            },
        }
    }

    #[test]
    fn lua_system_font_colors_follow_native_argb_pack_without_preclamping() {
        assert_eq!(
            system_font_color_from_lua([64.9, 128.9, 32.9, 255.9]),
            [128, 32, 255, 64]
        );
        // The native OR operations do not mask each converted input before
        // shifting. A negative final blue slot therefore sign-extends across
        // every packed channel.
        assert_eq!(
            system_font_color_from_lua([0.0, 0.0, 0.0, -1.0]),
            [255, 255, 255, 255]
        );
    }

    #[test]
    fn available_font_names_keep_family_then_face_order_and_duplicates() {
        let names = native_available_font_names([
            ("Family B", "B-Regular"),
            ("Family A", "A-Regular"),
            ("Family B", "B-Bold"),
            ("Family A", "A-Regular"),
        ]);
        assert_eq!(names, ["B-Regular", "B-Bold", "A-Regular", "A-Regular"]);
    }

    #[test]
    fn ui_font_metrics_fcvtzs_directly_from_double_precision() {
        // Both values round to an adjacent integer if first narrowed to f32.
        // The constructor instructions at 0x100477750/770/78C instead consume
        // the Objective-C double result directly.
        assert_eq!(native_double_to_i32(36.999_999_9), 36);
        assert_eq!(native_float_to_i32(36.999_999_9), 37);
        assert_eq!(native_double_to_i32(-36.999_999_9), -36);
        assert_eq!(native_float_to_i32(-36.999_999_9), -37);
        assert_eq!(native_double_to_i32(f64::NAN), i32::MIN);
        assert_eq!(native_double_to_i32(2_147_483_648.0), i32::MIN);
    }

    #[test]
    fn unavailable_font_is_reported_before_unsupported_style() {
        let error = create_system_font_state(
            0,
            "STELLA_FONT_THAT_CANNOT_EXIST_7C4D9E",
            12.0,
            [255, 255, 255, 255],
            0.0,
            [0, 0, 0, 255],
            1.0,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("Font STELLA_FONT_THAT_CANNOT_EXIST_7C4D9E is not available."),
            "{error}"
        );
        assert!(!error.contains("Style Bold"), "{error}");
    }

    #[test]
    fn ios_menu_face_has_a_portable_sans_compatibility_target() {
        let fonts = platform_system_fonts();
        let fallback = ios_menu_system_font_compatibility_face(&fonts.database)
            .expect("platform font database should provide a sans-serif face");
        assert!(fonts.database.face(fallback).is_some());

        let resolved = resolve_system_font_face(&fonts.database, IOS_MENU_SYSTEM_FONT)
            .expect("iOS menu face should resolve exactly or through compatibility");
        if let Some(exact) = named_system_font_face(&fonts.database, IOS_MENU_SYSTEM_FONT) {
            assert_eq!(resolved, exact, "an installed native face must win");
        }
    }

    #[test]
    fn installed_ios_boot_face_keeps_the_intercepted_stella_width_contract() {
        const FAMILY: &str = "ArialRoundedMTBold";
        if !platform_system_font_names()
            .iter()
            .any(|name| name == FAMILY)
        {
            // The binary has the same platform dependency: UIFont creation
            // fails when this named face is absent. Cross-platform CI may not
            // install Apple's supplemental font.
            return;
        }
        let font =
            create_system_font_state(0, FAMILY, 40.0, [0, 0, 0, 255], 0.0, [0, 0, 0, 255], 0.0)
                .unwrap();
        assert_eq!(font.render.ascending, 37);
        assert_eq!(font.render.descending, 8);
        assert_eq!(font.render.leading, 0);
        assert_eq!(font.render.label_line_height, 46);
        assert_eq!(font.render.native_string_width("Stella"), 110);

        for (text, width, fallback, expected_glyphs, expected_x) in [
            (
                "abc אבג 123",
                228,
                "LucidaGrande",
                vec![68, 69, 70, 3, 20, 21, 22, 3, 610, 609, 608],
                vec![
                    0.0,
                    23.2421875,
                    48.2421875,
                    72.01171875,
                    82.01171875,
                    105.78125,
                    129.55078125,
                    153.3203125,
                    163.3203125,
                    180.41015625,
                    202.9296875,
                ],
            ),
            (
                "abc مرحبا 123",
                241,
                "GeezaPro",
                vec![68, 69, 70, 3, 20, 21, 22, 3, 241, 244, 261, 273, 345],
                vec![
                    0.0,
                    23.2421875,
                    48.2421875,
                    72.01171875,
                    82.01171875,
                    105.78125,
                    129.55078125,
                    153.3203125,
                    163.3203125,
                    175.0201316681736,
                    186.8826994801085,
                    207.80494179475588,
                    224.11597253616637,
                ],
            ),
            (
                "abc 漢字 123",
                243,
                "PingFangSC-Regular",
                vec![68, 69, 70, 3, 20344, 2561, 3, 20, 21, 22],
                vec![
                    0.0,
                    23.2421875,
                    48.2421875,
                    72.01171875,
                    82.01171875,
                    122.01171875,
                    162.01171875,
                    172.01171875,
                    195.78125,
                    219.55078125,
                ],
            ),
        ] {
            let layout = font.render.native_system_font_layout(text).unwrap();
            assert_eq!(layout.width, width, "{text:?}");
            assert_eq!(layout.faces[0].family, FAMILY, "{text:?}");
            assert_eq!(layout.faces[1].family, fallback, "{text:?}");
            assert_eq!(
                layout.lines[0]
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.glyph_id)
                    .collect::<Vec<_>>(),
                expected_glyphs,
                "{text:?}"
            );
            for (glyph, expected) in layout.lines[0].glyphs.iter().zip(expected_x) {
                assert!((glyph.x - expected).abs() < 1.0e-9, "{text:?}");
                assert_eq!(glyph.y, 0.0, "{text:?}");
            }
            assert!(
                layout.lines[0]
                    .glyphs
                    .iter()
                    .any(|glyph| glyph.face_slot == 1),
                "{text:?}"
            );
        }
    }

    #[test]
    fn installed_apple_emoji_fallback_matches_coretext_aat_contracts() {
        const BASE: &str = "ArialRoundedMTBold";
        if !platform_system_font_names().iter().any(|name| name == BASE)
            || !platform_system_font_names()
                .iter()
                .any(|name| name == "AppleColorEmoji")
        {
            return;
        }
        let font = create_system_font_state(
            0,
            BASE,
            40.0,
            [255, 255, 255, 255],
            0.0,
            [0, 0, 0, 255],
            0.0,
        )
        .unwrap();

        for (text, width, glyph_id) in [
            ("\u{1F600}", 40, 2096),
            ("\u{2600}\u{FE0F}", 40, 189),
            (
                "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
                40,
                3237,
            ),
            ("\u{1F44D}\u{1F3FD}", 40, 1139),
            ("\u{1F1E8}\u{1F1F3}", 40, 423),
            ("1\u{FE0F}\u{20E3}", 40, 139),
        ] {
            let layout = font.render.native_system_font_layout(text).unwrap();
            assert_eq!(layout.width, width, "{text:?}");
            assert!(layout.faces[1].native_is_apple_color_emoji(), "{text:?}");
            assert_eq!(layout.lines[0].glyphs.len(), 1, "{text:?}");
            assert_eq!(layout.lines[0].glyphs[0].glyph_id, glyph_id, "{text:?}");
            assert_eq!(layout.lines[0].glyphs[0].x, 0.0, "{text:?}");
        }

        let mixed = font
            .render
            .native_system_font_layout("A\u{1F600}B")
            .unwrap();
        assert_eq!(mixed.width, 97);
        assert_eq!(mixed.lines[0].glyphs.len(), 3);
        assert!(
            mixed.faces[usize::from(mixed.lines[0].glyphs[1].face_slot)]
                .native_is_apple_color_emoji()
        );
        assert!((mixed.lines[0].glyphs[1].x - 28.769_531_25).abs() < 1.0e-9);
        assert!((mixed.lines[0].glyphs[2].x - 68.769_531_25).abs() < 1.0e-9);

        for (size, width) in [(12.0, 16), (17.0, 22)] {
            let small = create_system_font_state(
                0,
                BASE,
                size,
                [255, 255, 255, 255],
                0.0,
                [0, 0, 0, 255],
                0.0,
            )
            .unwrap();
            let layout = small.render.native_system_font_layout("\u{1F600}").unwrap();
            assert_eq!(layout.width, width, "{size}");
            assert_eq!(layout.lines[0].glyphs[0].x, 0.0, "{size}");
        }
    }

    #[test]
    fn installed_arial_unicode_matches_coretext_mixed_direction_runs() {
        let Ok(font) = create_system_font_state(
            0,
            "ArialUnicodeMS",
            40.0,
            [255, 255, 255, 255],
            0.0,
            [0, 0, 0, 255],
            0.0,
        ) else {
            // The regression remains deterministic on Apple hosts carrying
            // this iOS-era face and is intentionally skipped elsewhere.
            return;
        };

        let hebrew = font
            .render
            .native_system_font_layout("abc אבג 123")
            .unwrap();
        assert_eq!(hebrew.width, 215);
        assert_eq!(
            hebrew.lines[0]
                .glyphs
                .iter()
                .map(|glyph| glyph.glyph_id)
                .collect::<Vec<_>>(),
            [68, 69, 70, 3, 20, 21, 22, 3, 1156, 1155, 1154]
        );
        for (glyph, expected_units) in hebrew.lines[0].glyphs.iter().zip([
            0, 1139, 2278, 3302, 3871, 5010, 6149, 7288, 7857, 8699, 9852,
        ]) {
            let expected = f64::from(expected_units) * 40.0 / 2048.0;
            assert!((glyph.x - expected).abs() < f64::EPSILON);
            assert_eq!(glyph.face_slot, 0);
        }

        let arabic = font
            .render
            .native_system_font_layout("abc مرحبا 123")
            .unwrap();
        assert_eq!(arabic.width, 236);
        assert_eq!(
            arabic.lines[0]
                .glyphs
                .iter()
                .map(|glyph| glyph.glyph_id)
                .collect::<Vec<_>>(),
            [68, 69, 70, 3, 20, 21, 22, 3, 6510, 6514, 6531, 6542, 6595]
        );

        for (text, width, expected) in [
            (
                "abc (אבג) 123",
                242,
                vec![68, 69, 70, 3, 11, 1156, 1155, 1154, 12, 3, 20, 21, 22],
            ),
            (
                "(אבג abc) 123",
                242,
                vec![20, 21, 22, 3, 11, 68, 69, 70, 3, 1156, 1155, 1154, 12],
            ),
            ("123 אבג", 139, vec![1156, 1155, 1154, 3, 20, 21, 22]),
        ] {
            let layout = font.render.native_system_font_layout(text).unwrap();
            assert_eq!(layout.width, width, "{text:?}");
            assert_eq!(
                layout.lines[0]
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.glyph_id)
                    .collect::<Vec<_>>(),
                expected,
                "{text:?}"
            );
        }
    }

    #[test]
    fn system_font_height_getter_wraps_the_native_w_register_addition() {
        let font = metric_state(i32::MAX, 2);
        assert_eq!(
            system_font_metric(&font, FontMetric::Height),
            -2_147_483_647.0
        );
    }
}
