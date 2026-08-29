//! UIKit/CoreText-compatible system-font payload and shaping behavior.

use std::sync::Arc;

use super::font_shaper;
use super::system_font_layout::*;

/// Cross-platform rendering payload for Purple's UIKit `SystemFont::Impl`.
///
/// The original object retains a UIFont, normalized colors and integer
/// metrics. The desktop host cannot retain that process-local UIKit pointer,
/// so it retains the resolved face bytes/index and the same immutable values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemFontRenderBinding {
    /// LabelPool lifetime generation. The native global pool is erased when
    /// its last live SystemFont::Impl is destroyed; deferred draws from an
    /// earlier generation must not alias a newly constructed label with the
    /// same recovered hash.
    pub label_pool_epoch: u64,
    pub family: String,
    pub font_data: Arc<[u8]>,
    pub face_index: u32,
    /// Shared host font directory used only when the retained UIFont face has
    /// no cmap entry for part of an NSString. Native UIKit owns an equivalent
    /// fallback cascade behind `sizeWithFont:`/`drawInRect:withFont:`.
    pub fallback_catalog: Option<SystemFontFallbackCatalog>,
    pub size: i32,
    pub fill_rgba: [u8; 4],
    pub stroke_width: i32,
    pub stroke_rgba: [u8; 4],
    pub style: i32,
    pub ascending: i32,
    pub descending: i32,
    pub leading: i32,
    /// Independent `NSString sizeWithFont:` line height used for cached label
    /// allocation. This is converted once from the untruncated face metrics;
    /// it must not be reconstructed by adding the three already-truncated
    /// IFont metric queries.
    pub label_line_height: i32,
}

/// One glyph selected by the same OpenType shaping pass used for both native
/// SystemFont measurement and deferred rasterization. Coordinates remain in
/// font units so the renderer applies the point-size scale only once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemFontShapedGlyph {
    pub face_slot: u16,
    pub glyph_id: u16,
    /// Baseline-relative logical pixels. A fallback face can use a different
    /// units-per-em value, so positions cannot remain in one font's units.
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemFontLayoutFace {
    pub family: String,
    pub font_data: Arc<[u8]>,
    pub face_index: u32,
    pub units_per_em: u16,
}

impl SystemFontLayoutFace {
    pub(super) fn same_face(&self, other: &Self) -> bool {
        self.face_index == other.face_index
            && (Arc::ptr_eq(&self.font_data, &other.font_data) || self.font_data == other.font_data)
    }

    /// CoreText applies Apple Color Emoji's private optical-size transform to
    /// its sbix image while retaining the UIFont point size for line metrics.
    /// The transform is 1.25x through 16pt, interpolates back to 1x at 24pt,
    /// and is identity thereafter. CTFont numeric probes reproduce this exact
    /// piecewise contract; it is not derivable from the public sbix header.
    pub fn native_raster_em_size(&self, point_size: i32) -> f64 {
        let point_size = f64::from(point_size);
        if !self.native_is_apple_color_emoji() {
            point_size
        } else if point_size <= 16.0 {
            point_size * 1.25
        } else if point_size < 24.0 {
            point_size * 0.5 + 12.0
        } else {
            point_size
        }
    }

    pub fn native_is_apple_color_emoji(&self) -> bool {
        self.family
            .trim_start_matches('.')
            .eq_ignore_ascii_case("AppleColorEmoji")
            || self.family.eq_ignore_ascii_case("Apple Color Emoji")
            || self.family.eq_ignore_ascii_case("Apple Color Emoji UI")
    }

    /// Private CoreText offset of the optically sized Apple sbix bitmap from
    /// its CTRun glyph origin. The values are CTFont bounding-rect outputs,
    /// not screenshot-derived coordinates.
    pub fn native_raster_origin_offset(&self, point_size: i32) -> [f64; 2] {
        if !self.native_is_apple_color_emoji() || point_size <= 0 {
            return [0.0, 0.0];
        }
        let x = match point_size {
            1 => 0.003_125,
            2 => 0.012_5,
            3 => 0.028_125,
            4 => 0.05,
            5 => 0.078_125,
            6 => 0.112_5,
            7 => 0.153_125,
            8 => 0.2,
            9 => 0.258_75,
            10 => 0.287_5,
            11 => 0.316_25,
            12 => 0.345,
            13 => 0.373_75,
            14 => 0.402_5,
            15 => 0.431_25,
            16 => 0.46,
            17 => 0.440_75,
            18 => 0.42,
            19 => 0.408_5,
            20 => 0.385,
            21 => 0.36,
            22 => 0.345,
            23 => 0.293_75,
            24 => 0.252_001,
            25 => 0.212_501,
            26 => 0.156_001,
            27 => 0.108,
            28 => 0.056,
            _ => 0.0,
        };
        let size = f64::from(point_size);
        let down = if point_size <= 16 {
            size * 0.25
        } else if point_size < 24 {
            6.0 - size * 0.125
        } else {
            size * 0.125
        };
        [x, down]
    }

    fn native_apple_color_emoji_advance(&self, point_size: i32) -> f64 {
        if !self.native_is_apple_color_emoji() {
            return f64::from(point_size);
        }
        f64::from(match point_size {
            i32::MIN..=0 => 0,
            1 => 1,
            2 => 3,
            3 => 4,
            4 => 5,
            5 => 6,
            6 => 8,
            7 => 9,
            8 => 11,
            9 => 12,
            10 => 13,
            11 => 15,
            12 => 16,
            13 => 17,
            14 => 19,
            15 => 20,
            16 => 21,
            17 => 22,
            18 => 22,
            19 => 23,
            20 => 23,
            21 => 23,
            22 => 24,
            23 => 24,
            24 => 25,
            25 => 26,
            _ => point_size,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemFontShapedLine {
    pub width: i32,
    pub glyphs: Vec<SystemFontShapedGlyph>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemFontLayout {
    pub width: i32,
    pub faces: Vec<SystemFontLayoutFace>,
    pub lines: Vec<SystemFontShapedLine>,
}

impl SystemFontRenderBinding {
    /// Shape the NSString payload once for both `sizeWithFont:`-equivalent
    /// measurement and `drawInRect:withFont:`-equivalent rasterization.
    ///
    /// UIKit delegates both calls to its text stack. Rustybuzz supplies the
    /// corresponding OpenType GSUB/GPOS pass on every desktop backend; a raw
    /// character/advance loop would disagree for ligatures, combining marks,
    /// contextual scripts and fonts whose kerning lives only in GPOS.
    pub fn native_system_font_layout(&self, text: &str) -> Option<SystemFontLayout> {
        let base_face = font_shaper::Face::from_slice(&self.font_data, self.face_index)?;
        let base_layout_face = SystemFontLayoutFace {
            family: self.family.clone(),
            font_data: self.font_data.clone(),
            face_index: self.face_index,
            units_per_em: u16::try_from(base_face.units_per_em()).ok()?,
        };
        let mut faces = vec![base_layout_face.clone()];
        let mut width: Option<i32> = None;
        let mut lines = Vec::new();

        for line in native_system_font_lines(text) {
            let mut pen_x = 0.0_f64;
            let mut pen_y = 0.0_f64;
            let mut glyphs = Vec::new();
            for run in native_system_font_visual_runs(line) {
                let mut script_runs = native_system_font_script_runs(line, run.range);
                if run.right_to_left {
                    script_runs.reverse();
                }
                for script_run in script_runs {
                    let mut face_runs = native_system_font_face_runs(
                        line,
                        script_run.range,
                        &base_face,
                        &base_layout_face,
                        self.fallback_catalog.as_ref(),
                    );
                    if run.right_to_left {
                        face_runs.reverse();
                    }
                    for face_run in face_runs {
                        let face_slot = if let Some(slot) = faces
                            .iter()
                            .position(|candidate| candidate.same_face(&face_run.face))
                        {
                            u16::try_from(slot).ok()?
                        } else {
                            let slot = u16::try_from(faces.len()).ok()?;
                            faces.push(face_run.face.clone());
                            slot
                        };
                        let mut face = font_shaper::Face::from_slice(
                            &face_run.face.font_data,
                            face_run.face.face_index,
                        )?;
                        if let Some(ppem) = u16::try_from(self.size).ok().filter(|size| *size != 0)
                        {
                            face.set_pixels_per_em(Some((ppem, ppem)));
                            face.set_points_per_em(Some(f32::from(ppem)));
                        }
                        let scale = face_run.face.native_raster_em_size(self.size)
                            / f64::from(face_run.face.units_per_em);
                        let mut buffer = font_shaper::UnicodeBuffer::new();
                        buffer.push_str(&line[face_run.range]);
                        buffer.set_direction(if run.right_to_left {
                            font_shaper::Direction::RightToLeft
                        } else {
                            font_shaper::Direction::LeftToRight
                        });
                        buffer.set_script(native_system_font_script(script_run.script));
                        let shaped = font_shaper::shape(&face, &[], buffer);
                        glyphs.reserve(shaped.len());
                        let mut previous_cluster = None;
                        for (info, position) in
                            shaped.glyph_infos().iter().zip(shaped.glyph_positions())
                        {
                            let cluster_start = previous_cluster != Some(info.cluster);
                            previous_cluster = Some(info.cluster);
                            let invisible_apple_selector =
                                face_run.face.native_is_apple_color_emoji()
                                    && info.glyph_id == 3
                                    && position.x_advance == 0
                                    && position.y_advance == 0
                                    && position.x_offset == 0
                                    && position.y_offset == 0;
                            if !invisible_apple_selector {
                                glyphs.push(SystemFontShapedGlyph {
                                    face_slot,
                                    glyph_id: info.glyph_id as u16,
                                    x: pen_x + f64::from(position.x_offset) * scale,
                                    y: pen_y + f64::from(position.y_offset) * scale,
                                });
                            }
                            let advance = if face_run.face.native_is_apple_color_emoji()
                                && cluster_start
                                && position.x_advance != 0
                            {
                                face_run.face.native_apple_color_emoji_advance(self.size)
                                    * f64::from(position.x_advance)
                                    / f64::from(face_run.face.units_per_em)
                            } else {
                                f64::from(position.x_advance) * scale
                            };
                            pen_x += advance;
                            pen_y += f64::from(position.y_advance) * scale;
                        }
                    }
                }
            }
            let line_width = native_font_fcvtzs_f64(pen_x);
            width = Some(width.map_or(line_width, |current| current.max(line_width)));
            lines.push(SystemFontShapedLine {
                width: line_width,
                glyphs,
            });
        }

        Some(SystemFontLayout {
            width: width.unwrap_or(0),
            faces,
            lines,
        })
    }

    /// UIKit's `sizeWithFont:` width followed by AArch64 `FCVTZS`. Purple
    /// first converts UTF-8 to UTF-32, so callers that need a range must use
    /// [`Self::native_string_bounds`] rather than slicing UTF-8 bytes.
    pub fn native_string_width(&self, text: &str) -> i32 {
        self.native_system_font_layout(text)
            .map_or(0, |layout| layout.width)
    }

    /// Measured label height used by the byte-string IFont virtual. Purple's
    /// outer overload returns zero before entering UIKit for an empty source.
    pub fn native_string_height(&self, text: &str) -> i32 {
        if text.is_empty() {
            return 0;
        }
        self.native_measured_string_height(text)
    }

    /// Height returned after the nonempty byte-string wrapper has selected a
    /// UTF-32 substring and called `NSString sizeWithFont:`. An empty selected
    /// range is still one empty UIKit line; this differs from the outer
    /// overload's early return for an empty source string.
    fn native_measured_string_height(&self, text: &str) -> i32 {
        let lines = native_system_font_lines(text).len() as f64;
        native_font_fcvtzs_f64(f64::from(self.label_line_height) * lines)
    }

    /// Implements `SystemFont::Impl::getBounds` at `0x10047664C`.
    ///
    /// `start` and `count` are UTF-32 codepoint indices. The byte-string
    /// wrapper clamps a positive start beyond the string to its end, while a
    /// negative start reaches `basic_string::substr` as an invalid size_t and
    /// is represented by `None`. A negative count becomes a size_t-sized
    /// request and therefore selects the complete remaining suffix. An empty
    /// source bypasses range validation and measures as zero.
    pub fn native_string_bounds(
        &self,
        text: &str,
        horizontal_anchor: &str,
        vertical_anchor: &str,
        start: i32,
        count: i32,
    ) -> Option<[i32; 4]> {
        if text.is_empty() {
            return Some(self.native_bounds_from_measurement(
                0,
                0,
                horizontal_anchor,
                vertical_anchor,
            ));
        }
        let substring = native_font_codepoint_substring(text, start, count)?;
        Some(self.native_bounds_from_measurement(
            self.native_string_width(&substring),
            self.native_measured_string_height(&substring),
            horizontal_anchor,
            vertical_anchor,
        ))
    }

    fn native_bounds_from_measurement(
        &self,
        string_width: i32,
        string_height: i32,
        horizontal_anchor: &str,
        vertical_anchor: &str,
    ) -> [i32; 4] {
        // getBounds converts the integer width/height back to float32. In
        // particular HCENTER multiplies by -0.5 before each edge is converted;
        // it must not be collapsed into one integer width or rectangle size.
        let width = string_width as f32;
        let height = string_height as f32;
        let horizontal = match horizontal_anchor {
            "HCENTER" => width * -0.5,
            "RIGHT" => -width,
            _ => 0.0,
        };
        let vertical_metrics = self.ascending.wrapping_add(self.descending);
        let vertical = match vertical_anchor {
            "VCENTER" => (vertical_metrics / 2).wrapping_neg(),
            "BOTTOM" => vertical_metrics.wrapping_neg(),
            "BASELINE" => self.ascending.wrapping_neg(),
            // Native Anchor values TOP=0 and VPIVOT=4 both take the default
            // branch. BASELINE=3 is the branch that subtracts ascender.
            _ => 0,
        } as f32;
        let stroke = self.stroke_width as f32;
        [
            native_font_fcvtzs_f32(horizontal - stroke),
            native_font_fcvtzs_f32(vertical - stroke),
            native_font_fcvtzs_f32((width + horizontal) + stroke),
            native_font_fcvtzs_f32((height + vertical) + stroke),
        ]
    }
}

/// Split the line fragments recognized by Cocoa's legacy NSString drawing
/// stack. CRLF is one separator; LF, FF, CR, NEL, Unicode line separator and
/// Unicode paragraph separator each terminate a line. Vertical tab remains a
/// shaped control character rather than a line boundary.
fn native_system_font_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        if !matches!(
            character,
            '\n' | '\u{000C}' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}'
        ) {
            continue;
        }
        lines.push(&text[line_start..index]);
        line_start = index + character.len_utf8();
        if character == '\r' && matches!(characters.peek(), Some((_, '\n'))) {
            let (lf_index, lf) = characters.next().expect("peeked CRLF tail");
            line_start = lf_index + lf.len_utf8();
        }
    }
    lines.push(&text[line_start..]);
    lines
}

fn native_font_codepoint_substring(text: &str, start: i32, count: i32) -> Option<String> {
    let codepoints = text.chars().collect::<Vec<_>>();
    if codepoints.is_empty() {
        return Some(String::new());
    }
    // Native loads only W8 from the UTF-32 string's size field.
    let length = codepoints.len() as i32;
    // 0x100476468..480 and 0x10047657C..594 normalize in signed W-register
    // arithmetic before calling the UTF-32 overload. Keep the wrapping add:
    // it affects whether a very large signed count is pre-clamped.
    let start = if length < start { length } else { start };
    let count = if start.wrapping_add(count) > length {
        length.wrapping_sub(start)
    } else {
        count
    };
    let start = usize::try_from(start).ok()?;
    // The normalized signed count is sign-extended and consumed as size_t by
    // basic_string::substr. A remaining negative value selects the suffix.
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    let end = start.saturating_add(count).min(codepoints.len());
    Some(codepoints[start..end].iter().collect())
}

fn native_font_fcvtzs_f32(value: f32) -> i32 {
    if !value.is_finite() || !(-2_147_483_648.0_f32..2_147_483_648.0_f32).contains(&value) {
        i32::MIN
    } else {
        value.trunc() as i32
    }
}

fn native_font_fcvtzs_f64(value: f64) -> i32 {
    if !value.is_finite() || !(-2_147_483_648.0_f64..2_147_483_648.0_f64).contains(&value) {
        i32::MIN
    } else {
        value.trunc() as i32
    }
}

#[cfg(test)]
mod system_font_bounds_tests {
    use super::*;

    fn open_sans() -> Option<Vec<u8>> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../angry birds stella v1.1.6/Payload/Purple.app/OpenSans-Regular.ttf"),
        )
        .ok()
    }

    fn binding() -> SystemFontRenderBinding {
        SystemFontRenderBinding {
            label_pool_epoch: 0,
            family: "Test".to_owned(),
            font_data: Arc::from([]),
            face_index: 0,
            fallback_catalog: None,
            size: 40,
            fill_rgba: [0, 0, 0, 255],
            stroke_width: 2,
            stroke_rgba: [0, 0, 0, 255],
            style: 0,
            ascending: 37,
            descending: 8,
            leading: 0,
            label_line_height: 46,
        }
    }

    fn open_sans_binding() -> Option<SystemFontRenderBinding> {
        Some(SystemFontRenderBinding {
            font_data: Arc::from(open_sans()?),
            family: "OpenSans".to_owned(),
            ..binding()
        })
    }

    #[test]
    fn system_font_measurement_retains_the_glyphs_from_one_shaping_pass() {
        let Some(binding) = open_sans_binding() else {
            eprintln!("skipping Purple.app font regression: OpenSans-Regular.ttf is unavailable");
            return;
        };
        let ligature = binding.native_system_font_layout("ffi").unwrap();
        assert_eq!(ligature.lines.len(), 1);
        assert!(ligature.lines[0].glyphs.len() < "ffi".chars().count());
        assert_eq!(binding.native_string_width("ffi"), ligature.width);

        let combined = binding.native_system_font_layout("e\u{301}").unwrap();
        assert_eq!(combined.lines[0].glyphs.len(), 1);

        let multiline = binding.native_system_font_layout("ffi\nW").unwrap();
        assert_eq!(multiline.lines.len(), 2);
        assert_eq!(
            multiline.width,
            multiline.lines.iter().map(|line| line.width).max().unwrap()
        );
    }

    #[test]
    fn system_font_layout_reorders_uax9_runs_before_shaping() {
        assert_eq!(
            native_system_font_visual_runs("abc אבג 123"),
            [
                NativeSystemFontDirectionalRun {
                    range: 0..4,
                    right_to_left: false,
                },
                NativeSystemFontDirectionalRun {
                    range: 11..14,
                    right_to_left: false,
                },
                NativeSystemFontDirectionalRun {
                    range: 4..11,
                    right_to_left: true,
                },
            ]
        );
        assert_eq!(
            native_system_font_visual_runs("אבג abc 123"),
            [
                NativeSystemFontDirectionalRun {
                    range: 7..14,
                    right_to_left: false,
                },
                NativeSystemFontDirectionalRun {
                    range: 0..7,
                    right_to_left: true,
                },
            ]
        );

        let Some(binding) = open_sans_binding() else {
            eprintln!("skipping Purple.app font regression: OpenSans-Regular.ttf is unavailable");
            return;
        };
        let layout = binding.native_system_font_layout("abc אבג 123").unwrap();
        let face = font_shaper::Face::from_slice(&binding.font_data, 0).unwrap();
        let expected = [
            face.glyph_index('a').unwrap().to_u32() as u16,
            face.glyph_index('b').unwrap().to_u32() as u16,
            face.glyph_index('c').unwrap().to_u32() as u16,
            face.glyph_index(' ').unwrap().to_u32() as u16,
            face.glyph_index('1').unwrap().to_u32() as u16,
            face.glyph_index('2').unwrap().to_u32() as u16,
            face.glyph_index('3').unwrap().to_u32() as u16,
            face.glyph_index(' ').unwrap().to_u32() as u16,
            0,
            0,
            0,
        ];
        assert_eq!(
            layout.lines[0]
                .glyphs
                .iter()
                .map(|glyph| glyph.glyph_id)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn system_font_script_itemization_keeps_neutrals_and_marks_attached() {
        assert_eq!(
            native_system_font_script_runs("abc 漢字 123", 0..14),
            [
                NativeSystemFontScriptRun {
                    range: 0..4,
                    script: UnicodeScriptCode::Latin,
                },
                NativeSystemFontScriptRun {
                    range: 4..14,
                    script: UnicodeScriptCode::Han,
                },
            ]
        );
        assert_eq!(
            native_system_font_script_runs("e\u{301} α", 0..6),
            [
                NativeSystemFontScriptRun {
                    range: 0..4,
                    script: UnicodeScriptCode::Latin,
                },
                NativeSystemFontScriptRun {
                    range: 4..6,
                    script: UnicodeScriptCode::Greek,
                },
            ]
        );
    }

    #[test]
    fn system_font_layout_uses_cocoa_line_separators_and_coalesces_crlf() {
        let Some(binding) = open_sans_binding() else {
            eprintln!("skipping Purple.app font regression: OpenSans-Regular.ttf is unavailable");
            return;
        };
        let expected_width = binding.native_string_width("WW");
        for separator in [
            "\n", "\u{000C}", "\r", "\r\n", "\u{0085}", "\u{2028}", "\u{2029}",
        ] {
            let text = format!("WW{separator}i");
            let layout = binding.native_system_font_layout(&text).unwrap();
            assert_eq!(layout.lines.len(), 2, "separator {separator:?}");
            assert_eq!(layout.width, expected_width, "separator {separator:?}");
            assert_eq!(binding.native_string_height(&text), 92);
        }

        let vertical_tab = binding.native_system_font_layout("WW\u{000B}i").unwrap();
        assert_eq!(vertical_tab.lines.len(), 1);
        assert_eq!(binding.native_string_height("WW\u{000B}i"), 46);
    }

    #[test]
    fn system_font_bounds_match_native_anchor_and_independent_fcvtzs_edges() {
        let binding = binding();
        assert_eq!(
            binding.native_bounds_from_measurement(5, 46, "LEFT", "TOP"),
            [-2, -2, 7, 48]
        );
        assert_eq!(
            binding.native_bounds_from_measurement(5, 46, "HCENTER", "TOP"),
            [-4, -2, 4, 48]
        );
        assert_eq!(
            binding.native_bounds_from_measurement(5, 46, "RIGHT", "BOTTOM"),
            [-7, -47, 2, 3]
        );
        assert_eq!(
            binding.native_bounds_from_measurement(5, 46, "LEFT", "BASELINE"),
            [-2, -39, 7, 11]
        );
        assert_eq!(
            binding.native_bounds_from_measurement(5, 46, "LEFT", "VPIVOT"),
            [-2, -2, 7, 48]
        );
    }

    #[test]
    fn system_font_substrings_use_utf32_indices_and_native_signed_ranges() {
        let text = "Aé鸟B";
        assert_eq!(
            native_font_codepoint_substring(text, 1, 2).as_deref(),
            Some("é鸟")
        );
        assert_eq!(
            native_font_codepoint_substring(text, 2, -1).as_deref(),
            Some("鸟B")
        );
        assert_eq!(
            native_font_codepoint_substring(text, 4, 1),
            Some(String::new())
        );
        assert_eq!(native_font_codepoint_substring(text, -1, 1), None);
        assert_eq!(
            native_font_codepoint_substring(text, 5, 1),
            Some(String::new())
        );
        assert_eq!(
            native_font_codepoint_substring("", -9, i32::MIN),
            Some(String::new())
        );
        assert_eq!(
            native_font_codepoint_substring(text, 1, i32::MAX).as_deref(),
            Some("é鸟B")
        );
    }

    #[test]
    fn empty_system_font_bounds_ignore_invalid_substring_range() {
        assert_eq!(
            binding().native_string_bounds("", "LEFT", "TOP", -99, i32::MIN),
            Some([-2, -2, 2, 2])
        );
        assert_eq!(
            binding().native_string_bounds("", "LEFT", "BASELINE", i32::MAX, -1),
            Some([-2, -39, 2, -35])
        );
    }

    #[test]
    fn nonempty_source_with_empty_range_keeps_one_uikit_line_height() {
        assert_eq!(
            binding().native_string_bounds("A", "LEFT", "TOP", 1, 0),
            Some([-2, -2, 2, 48])
        );
        assert_eq!(
            binding().native_string_bounds("A", "LEFT", "TOP", i32::MAX, 1),
            Some([-2, -2, 2, 48])
        );
        assert_eq!(binding().native_string_height(""), 0);
    }
}
