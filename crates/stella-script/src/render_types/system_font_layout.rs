//! CoreText-equivalent paragraph itemization used before OpenType shaping.

use std::{
    collections::HashMap,
    fmt,
    ops::Range,
    sync::{Arc, Mutex},
};

pub(super) use unicode_script::Script as UnicodeScriptCode;
use unicode_script::UnicodeScript;
use unicode_segmentation::UnicodeSegmentation;

use super::SystemFontLayoutFace;

#[derive(Clone)]
pub struct SystemFontFallbackCatalog(Arc<SystemFontFallbackCatalogInner>);

struct SystemFontFallbackCatalogInner {
    database: Arc<fontdb::Database>,
    clusters: Mutex<HashMap<String, Option<SystemFontLayoutFace>>>,
    faces: Mutex<HashMap<fontdb::ID, SystemFontLayoutFace>>,
}

impl SystemFontFallbackCatalog {
    pub(crate) fn new(database: Arc<fontdb::Database>) -> Self {
        Self(Arc::new(SystemFontFallbackCatalogInner {
            database,
            clusters: Mutex::new(HashMap::new()),
            faces: Mutex::new(HashMap::new()),
        }))
    }

    fn resolve(&self, cluster: &str) -> Option<SystemFontLayoutFace> {
        if let Some(face) = self
            .0
            .clusters
            .lock()
            .ok()
            .and_then(|cache| cache.get(cluster).cloned())
        {
            return face;
        }

        let face = self.resolve_uncached(cluster);
        if let Ok(mut cache) = self.0.clusters.lock() {
            cache.insert(cluster.to_owned(), face.clone());
        }
        face
    }

    fn resolve_uncached(&self, cluster: &str) -> Option<SystemFontLayoutFace> {
        let presentation = native_system_font_cluster_presentation(cluster);
        let script = cluster
            .chars()
            .map(|character| character.script())
            .find_map(native_system_font_concrete_script)
            .unwrap_or(UnicodeScriptCode::Common);
        let preferred = if presentation == NativeSystemFontPresentation::Emoji {
            native_system_font_emoji_fallback_names()
        } else {
            native_system_font_fallback_names(script)
        };
        let preferred_id = preferred.iter().find_map(|preferred_name| {
            self.0
                .database
                .faces()
                .find(|face| {
                    (face.post_script_name.eq_ignore_ascii_case(preferred_name)
                        || face
                            .families
                            .iter()
                            .any(|(family, _)| family.eq_ignore_ascii_case(preferred_name)))
                        && native_system_font_face_supports(&self.0.database, face.id, cluster)
                        && (presentation != NativeSystemFontPresentation::Emoji
                            || native_system_font_face_has_color_tables(&self.0.database, face.id))
                })
                .map(|face| face.id)
        });
        let face_id = preferred_id
            .or_else(|| {
                self.0
                    .database
                    .faces()
                    .filter(|face| {
                        face.style == fontdb::Style::Normal
                            && !native_system_font_excluded_fallback(&face.post_script_name)
                            && match presentation {
                                NativeSystemFontPresentation::Emoji => {
                                    native_system_font_face_has_color_tables(
                                        &self.0.database,
                                        face.id,
                                    )
                                }
                                NativeSystemFontPresentation::Text
                                | NativeSystemFontPresentation::Automatic => {
                                    !native_system_font_face_has_color_tables(
                                        &self.0.database,
                                        face.id,
                                    )
                                }
                            }
                    })
                    .find(|face| {
                        native_system_font_face_supports(&self.0.database, face.id, cluster)
                    })
                    .map(|face| face.id)
            })
            .or_else(|| {
                // An automatic-presentation scalar may have no monochrome face on
                // the host. CoreText then accepts a color-only fallback rather
                // than returning the retained UIFont's notdef glyph. VS15 is the
                // exception: it is an explicit request for text presentation.
                (presentation == NativeSystemFontPresentation::Automatic)
                    .then(|| {
                        self.0.database.faces().find(|face| {
                            face.style == fontdb::Style::Normal
                                && !native_system_font_excluded_fallback(&face.post_script_name)
                                && native_system_font_face_supports(
                                    &self.0.database,
                                    face.id,
                                    cluster,
                                )
                        })
                    })
                    .flatten()
                    .map(|face| face.id)
            })?;
        self.load_face(face_id)
    }

    fn load_face(&self, face_id: fontdb::ID) -> Option<SystemFontLayoutFace> {
        if let Some(face) = self
            .0
            .faces
            .lock()
            .ok()
            .and_then(|faces| faces.get(&face_id).cloned())
        {
            return Some(face);
        }
        let family = self.0.database.face(face_id)?.post_script_name.clone();
        let face = self
            .0
            .database
            .with_face_data(face_id, |data, face_index| {
                let face = ttf_parser::Face::parse(data, face_index).ok()?;
                Some(SystemFontLayoutFace {
                    family,
                    font_data: Arc::from(data),
                    face_index,
                    units_per_em: face.units_per_em(),
                })
            })
            .flatten()?;
        if let Ok(mut faces) = self.0.faces.lock() {
            faces.insert(face_id, face.clone());
        }
        Some(face)
    }
}

impl fmt::Debug for SystemFontFallbackCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SystemFontFallbackCatalog")
            .field("faces", &self.0.database.faces().count())
            .field(
                "cached_clusters",
                &self.0.clusters.lock().map_or(0, |cache| cache.len()),
            )
            .field(
                "loaded_faces",
                &self.0.faces.lock().map_or(0, |faces| faces.len()),
            )
            .finish()
    }
}

impl PartialEq for SystemFontFallbackCatalog {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SystemFontFallbackCatalog {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeSystemFontDirectionalRun {
    pub(super) range: Range<usize>,
    pub(super) right_to_left: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeSystemFontScriptRun {
    pub(super) range: Range<usize>,
    pub(super) script: UnicodeScriptCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeSystemFontFaceRun {
    pub(super) range: Range<usize>,
    pub(super) face: SystemFontLayoutFace,
}

/// Resolve one Cocoa line into UAX #9 level runs in left-to-right display
/// order. CoreText performs this paragraph pass before its CTRuns reach the
/// font shaper; Rustybuzz intentionally shapes one directional run and does
/// not implement paragraph-level bidi reordering itself.
pub(super) fn native_system_font_visual_runs(line: &str) -> Vec<NativeSystemFontDirectionalRun> {
    if line.is_empty() {
        return Vec::new();
    }
    let bidi = unicode_bidi::ParagraphBidiInfo::new(line, None);
    let (levels, ranges) = bidi.visual_runs(0..line.len());
    ranges
        .into_iter()
        .map(|range| NativeSystemFontDirectionalRun {
            right_to_left: levels[range.start].is_rtl(),
            range,
        })
        .collect()
}

/// Itemize a single directional level run by Unicode Script. Common,
/// Inherited and Unknown characters stay with a neighbouring concrete script
/// so punctuation and combining marks are not detached from the text they
/// modify. When two different scripts surround a neutral, UIKit's observable
/// run boundary is reproduced by retaining it with the preceding script.
pub(super) fn native_system_font_script_runs(
    line: &str,
    range: Range<usize>,
) -> Vec<NativeSystemFontScriptRun> {
    if range.is_empty() {
        return Vec::new();
    }

    let chars = line[range.clone()]
        .char_indices()
        .map(|(offset, character)| (range.start + offset, character.script()))
        .collect::<Vec<_>>();
    let concrete = chars
        .iter()
        .map(|(_, script)| native_system_font_concrete_script(*script))
        .collect::<Vec<_>>();
    let mut resolved = Vec::with_capacity(chars.len());
    let mut previous = None;
    for (index, script) in concrete.iter().copied().enumerate() {
        let script = match script {
            Some(script) => script,
            None => previous
                .or_else(|| concrete[index + 1..].iter().copied().flatten().next())
                .unwrap_or(UnicodeScriptCode::Common),
        };
        previous = native_system_font_concrete_script(script).or(previous);
        resolved.push(script);
    }

    let mut runs = Vec::new();
    let mut start = chars[0].0;
    let mut script = resolved[0];
    for (index, &next_script) in resolved.iter().enumerate().skip(1) {
        if next_script != script {
            runs.push(NativeSystemFontScriptRun {
                range: start..chars[index].0,
                script,
            });
            start = chars[index].0;
            script = next_script;
        }
    }
    runs.push(NativeSystemFontScriptRun {
        range: start..range.end,
        script,
    });
    runs
}

pub(super) fn native_system_font_face_runs(
    line: &str,
    range: Range<usize>,
    base_face: &rustybuzz::Face<'_>,
    base_layout_face: &SystemFontLayoutFace,
    fallback_catalog: Option<&SystemFontFallbackCatalog>,
) -> Vec<NativeSystemFontFaceRun> {
    if range.is_empty() {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let mut run_start = range.start;
    let mut run_face = base_layout_face.clone();
    for (relative, cluster) in line[range.clone()].grapheme_indices(true) {
        let index = range.start + relative;
        let presentation = native_system_font_cluster_presentation(cluster);
        let face = if presentation == NativeSystemFontPresentation::Emoji {
            fallback_catalog
                .and_then(|catalog| catalog.resolve(cluster))
                .unwrap_or_else(|| base_layout_face.clone())
        } else if native_system_font_face_covers_cluster(base_face, cluster) {
            base_layout_face.clone()
        } else {
            fallback_catalog
                .and_then(|catalog| catalog.resolve(cluster))
                .unwrap_or_else(|| base_layout_face.clone())
        };
        if index != run_start && !run_face.same_face(&face) {
            runs.push(NativeSystemFontFaceRun {
                range: run_start..index,
                face: run_face,
            });
            run_start = index;
        }
        run_face = face;
    }
    runs.push(NativeSystemFontFaceRun {
        range: run_start..range.end,
        face: run_face,
    });
    runs
}

fn native_system_font_face_supports(
    database: &fontdb::Database,
    face_id: fontdb::ID,
    cluster: &str,
) -> bool {
    database
        .with_face_data(face_id, |data, face_index| {
            ttf_parser::Face::parse(data, face_index)
                .ok()
                .is_some_and(|face| {
                    cluster.chars().all(|character| {
                        native_system_font_fallback_ignorable(character)
                            || face.glyph_index(character).is_some()
                    })
                })
        })
        .unwrap_or(false)
}

fn native_system_font_face_has_color_tables(
    database: &fontdb::Database,
    face_id: fontdb::ID,
) -> bool {
    database
        .with_face_data(face_id, |data, face_index| {
            ttf_parser::Face::parse(data, face_index)
                .ok()
                .is_some_and(|face| {
                    let tables = face.tables();
                    tables.sbix.is_some()
                        || tables.bdat.is_some()
                        || tables.cbdt.is_some()
                        || tables.ebdt.is_some()
                        || tables.colr.is_some()
                })
        })
        .unwrap_or(false)
}

fn native_system_font_face_covers_cluster(face: &rustybuzz::Face<'_>, cluster: &str) -> bool {
    cluster.chars().all(|character| {
        native_system_font_fallback_ignorable(character) || face.glyph_index(character).is_some()
    })
}

fn native_system_font_fallback_ignorable(character: char) -> bool {
    character.is_whitespace()
        || character.is_control()
        || matches!(
            character,
            '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}'
        )
}

fn native_system_font_excluded_fallback(post_script_name: &str) -> bool {
    ["LastResort"]
        .iter()
        .any(|excluded| post_script_name.contains(excluded))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeSystemFontPresentation {
    Automatic,
    Text,
    Emoji,
}

fn native_system_font_cluster_presentation(cluster: &str) -> NativeSystemFontPresentation {
    if cluster.contains('\u{FE0E}') {
        return NativeSystemFontPresentation::Text;
    }
    if cluster.contains('\u{FE0F}')
        || cluster.contains('\u{200D}')
        || cluster.contains('\u{20E3}')
        || cluster.chars().any(native_system_font_default_emoji)
    {
        NativeSystemFontPresentation::Emoji
    } else {
        NativeSystemFontPresentation::Automatic
    }
}

fn native_system_font_default_emoji(character: char) -> bool {
    matches!(
        character,
        '\u{231A}'..='\u{231B}'
            | '\u{23E9}'..='\u{23EC}'
            | '\u{23F0}'
            | '\u{23F3}'
            | '\u{25FD}'..='\u{25FE}'
            | '\u{2614}'..='\u{2615}'
            | '\u{2648}'..='\u{2653}'
            | '\u{267F}'
            | '\u{2693}'
            | '\u{26A1}'
            | '\u{26AA}'..='\u{26AB}'
            | '\u{26BD}'..='\u{26BE}'
            | '\u{26C4}'..='\u{26C5}'
            | '\u{26CE}'
            | '\u{26D4}'
            | '\u{26EA}'
            | '\u{26F2}'..='\u{26F3}'
            | '\u{26F5}'
            | '\u{26FA}'
            | '\u{26FD}'
            | '\u{2705}'
            | '\u{270A}'..='\u{270B}'
            | '\u{2728}'
            | '\u{274C}'
            | '\u{274E}'
            | '\u{2753}'..='\u{2755}'
            | '\u{2757}'
            | '\u{2795}'..='\u{2797}'
            | '\u{27B0}'
            | '\u{27BF}'
            | '\u{2B1B}'..='\u{2B1C}'
            | '\u{2B50}'
            | '\u{2B55}'
            | '\u{1F000}'..='\u{1FAFF}'
    )
}

fn native_system_font_emoji_fallback_names() -> &'static [&'static str] {
    &[
        "AppleColorEmoji",
        ".AppleColorEmojiUI",
        "SegoeUIEmoji",
        "NotoColorEmoji",
        "NotoColorEmoji-Regular",
    ]
}

fn native_system_font_fallback_names(script: UnicodeScriptCode) -> &'static [&'static str] {
    use UnicodeScriptCode::*;
    match script {
        Hebrew => &[
            "LucidaGrande",
            "ArialHebrew",
            "NotoSansHebrew-Regular",
            "DejaVuSans",
        ],
        Arabic => &["GeezaPro", "Arial", "NotoSansArabic-Regular", "DejaVuSans"],
        Han => &[
            "PingFangSC-Regular",
            "HiraginoSans-W3",
            "HiraKakuProN-W3",
            "NotoSansCJKsc-Regular",
            "ArialUnicodeMS",
        ],
        Hiragana | Katakana => &[
            "HiraginoSans-W3",
            "HiraKakuProN-W3",
            "YuGothic-Regular",
            "NotoSansCJKjp-Regular",
            "ArialUnicodeMS",
        ],
        Hangul => &[
            "AppleSDGothicNeo-Regular",
            "NotoSansCJKkr-Regular",
            "MalgunGothic",
            "ArialUnicodeMS",
        ],
        Thai => &[
            "Thonburi",
            "NotoSansThai-Regular",
            "Tahoma",
            "ArialUnicodeMS",
        ],
        Devanagari => &[
            "KohinoorDevanagari-Regular",
            "NotoSansDevanagari-Regular",
            "Mangal",
            "ArialUnicodeMS",
        ],
        Bengali => &[
            "KohinoorBangla-Regular",
            "NotoSansBengali-Regular",
            "ArialUnicodeMS",
        ],
        Gurmukhi => &["GurmukhiMN", "NotoSansGurmukhi-Regular", "ArialUnicodeMS"],
        Gujarati => &[
            "GujaratiSangamMN",
            "NotoSansGujarati-Regular",
            "ArialUnicodeMS",
        ],
        Tamil => &["TamilSangamMN", "NotoSansTamil-Regular", "ArialUnicodeMS"],
        Telugu => &["TeluguSangamMN", "NotoSansTelugu-Regular", "ArialUnicodeMS"],
        Kannada => &[
            "KannadaSangamMN",
            "NotoSansKannada-Regular",
            "ArialUnicodeMS",
        ],
        Malayalam => &[
            "MalayalamSangamMN",
            "NotoSansMalayalam-Regular",
            "ArialUnicodeMS",
        ],
        Oriya => &["OriyaSangamMN", "NotoSansOriya-Regular", "ArialUnicodeMS"],
        Armenian => &["Mshtakan", "NotoSansArmenian-Regular", "ArialUnicodeMS"],
        Georgian => &["NotoSansGeorgian-Regular", "ArialUnicodeMS"],
        Ethiopic => &["Kefa", "NotoSansEthiopic-Regular", "ArialUnicodeMS"],
        _ => &[],
    }
}

fn native_system_font_concrete_script(script: UnicodeScriptCode) -> Option<UnicodeScriptCode> {
    (!matches!(
        script,
        UnicodeScriptCode::Common | UnicodeScriptCode::Inherited | UnicodeScriptCode::Unknown
    ))
    .then_some(script)
}

pub(super) fn native_system_font_rustybuzz_script(script: UnicodeScriptCode) -> rustybuzz::Script {
    let tag = rustybuzz::ttf_parser::Tag::from_bytes(&script.as_iso15924_tag().to_be_bytes());
    rustybuzz::Script::from_iso15924_tag(tag).unwrap_or(rustybuzz::script::UNKNOWN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn purple_font(name: &str) -> Option<Vec<u8>> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../angry birds stella v1.1.6/Payload/Purple.app")
                .join(name),
        )
        .ok()
    }

    #[test]
    fn fallback_catalog_loads_one_shared_face_for_multiple_character_queries() {
        let (Some(open_sans), Some(angry_birds_text)) = (
            purple_font("OpenSans-Regular.ttf"),
            purple_font("AngryBirdsText-Regular.ttf"),
        ) else {
            eprintln!("skipping Purple.app font regression: bundle fonts are unavailable");
            return;
        };
        let mut database = fontdb::Database::new();
        database.load_font_data(open_sans);
        database.load_font_data(angry_birds_text);
        let catalog = SystemFontFallbackCatalog::new(Arc::new(database));

        let first = catalog.resolve("\u{F8FF}").unwrap();
        let second = catalog.resolve("\u{F8FF}").unwrap();
        assert_eq!(first.family, "AngryBirdsText-Regular");
        assert!(Arc::ptr_eq(&first.font_data, &second.font_data));
        assert_eq!(catalog.0.faces.lock().unwrap().len(), 1);
        assert_eq!(catalog.0.clusters.lock().unwrap().len(), 1);
    }

    #[test]
    fn missing_cmap_character_becomes_a_separate_fallback_face_run() {
        let (Some(open_sans), Some(angry_birds_text)) = (
            purple_font("OpenSans-Regular.ttf"),
            purple_font("AngryBirdsText-Regular.ttf"),
        ) else {
            eprintln!("skipping Purple.app font regression: bundle fonts are unavailable");
            return;
        };
        let base_data = Arc::<[u8]>::from(open_sans.clone());
        let base = rustybuzz::Face::from_slice(&base_data, 0).unwrap();
        let base_layout = SystemFontLayoutFace {
            family: "OpenSans".to_owned(),
            font_data: base_data.clone(),
            face_index: 0,
            units_per_em: u16::try_from(base.units_per_em()).unwrap(),
        };
        let mut database = fontdb::Database::new();
        database.load_font_data(open_sans);
        database.load_font_data(angry_birds_text);
        let catalog = SystemFontFallbackCatalog::new(Arc::new(database));

        let runs =
            native_system_font_face_runs("A\u{F8FF}B", 0..5, &base, &base_layout, Some(&catalog));
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].range, 0..1);
        assert!(runs[0].face.same_face(&base_layout));
        assert_eq!(runs[1].range, 1..4);
        assert_eq!(runs[1].face.family, "AngryBirdsText-Regular");
        assert_eq!(runs[2].range, 4..5);
        assert!(runs[2].face.same_face(&base_layout));
    }

    #[test]
    fn emoji_presentation_honors_variation_selectors_and_default_ranges() {
        assert_eq!(
            native_system_font_cluster_presentation("\u{2600}"),
            NativeSystemFontPresentation::Automatic
        );
        assert_eq!(
            native_system_font_cluster_presentation("\u{2600}\u{FE0F}"),
            NativeSystemFontPresentation::Emoji
        );
        assert_eq!(
            native_system_font_cluster_presentation("\u{1F600}"),
            NativeSystemFontPresentation::Emoji
        );
        assert_eq!(
            native_system_font_cluster_presentation("\u{231A}"),
            NativeSystemFontPresentation::Emoji
        );
        assert_eq!(
            native_system_font_cluster_presentation("\u{231A}\u{FE0E}"),
            NativeSystemFontPresentation::Text
        );
    }
}
