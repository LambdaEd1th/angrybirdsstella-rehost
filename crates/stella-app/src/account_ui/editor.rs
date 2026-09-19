//! Single-line desktop editor. Committed cursor/selection positions always
//! follow grapheme boundaries, including after pointer placement and insertion.
//! IME marked-text positions remain UTF-8 character offsets owned by the IME.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub(super) struct Editor {
    value: String,
    cursor: usize,
    anchor: usize,
    pub(super) preedit: String,
    pub(super) preedit_cursor: Option<(usize, usize)>,
}

impl Editor {
    pub(super) fn text(&self) -> &str {
        &self.value
    }

    pub(super) fn selection(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    pub(super) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(super) fn replace(&mut self, text: &str) {
        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
        let selected = self.selection();
        self.value.replace_range(selected.clone(), &text);
        // Inserting a ZWJ/regional indicator/prepend character can merge with
        // the text to the right. Keep the committed caret beyond that complete
        // cluster instead of leaving the next deletion inside it.
        self.cursor = grapheme_boundary_after(&self.value, selected.start + text.len());
        self.anchor = self.cursor;
        self.clear_preedit();
    }

    pub(super) fn set_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        self.preedit = text.to_owned();
        self.preedit_cursor =
            cursor.map(|(start, end)| (char_boundary(text, start), char_boundary(text, end)));
    }

    pub(super) fn clear_preedit(&mut self) {
        self.preedit.clear();
        self.preedit_cursor = None;
    }

    pub(super) fn move_to(&mut self, position: usize, extend: bool) {
        self.cursor = grapheme_boundary_before(&self.value, position);
        if !extend {
            self.anchor = self.cursor;
        }
        self.clear_preedit();
    }

    pub(super) fn move_horizontal(&mut self, right: bool, extend: bool) {
        let selected = self.selection();
        let target = if !extend && !selected.is_empty() {
            if right { selected.end } else { selected.start }
        } else if right {
            self.value
                .grapheme_indices(true)
                .find(|(i, _)| *i > self.cursor)
                .map_or(self.value.len(), |(i, _)| i)
        } else {
            self.value
                .grapheme_indices(true)
                .rfind(|(i, _)| *i < self.cursor)
                .map_or(0, |(i, _)| i)
        };
        self.move_to(target, extend);
    }

    pub(super) fn erase(&mut self, forward: bool) {
        if self.selection().is_empty() {
            self.move_horizontal(forward, true);
        }
        self.replace("");
    }

    pub(super) fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.value.len();
        self.clear_preedit();
    }

    /// Drawing receives only bullets for secure entry, including IME marked
    /// text. The actual password never enters any label/GPU resource cache.
    pub(super) fn display(&self, secure: bool) -> (String, usize, std::ops::Range<usize>) {
        let selected = self.selection();
        let mut value = self.value.clone();
        let mut cursor = self.cursor;
        let mut marked = selected.clone();
        if !self.preedit.is_empty() {
            value.replace_range(selected.clone(), &self.preedit);
            marked = selected.start..selected.start + self.preedit.len();
            cursor = selected.start + self.preedit_cursor.map_or(self.preedit.len(), |p| p.1);
        }
        if secure {
            let count = |end| value[..char_boundary(&value, end)].graphemes(true).count();
            let secured = "•".repeat(value.graphemes(true).count());
            (
                secured,
                count(cursor) * 3,
                count(marked.start) * 3..count(marked.end) * 3,
            )
        } else {
            (value, cursor, marked)
        }
    }
}

fn grapheme_boundary_before(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .map(|(offset, _)| offset)
        .take_while(|offset| *offset <= index)
        .last()
        .unwrap_or(0)
}

fn grapheme_boundary_after(text: &str, index: usize) -> usize {
    text.grapheme_indices(true)
        .find(|(offset, _)| *offset >= index)
        .map_or(text.len(), |(offset, _)| offset)
}

fn char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_never_splits_unicode_clusters_or_selected_ranges() {
        let mut editor = Editor::default();
        editor.replace("A中e\u{301}👩‍🚀");
        editor.erase(false);
        assert_eq!(editor.text(), "A中e\u{301}");
        editor.erase(false);
        assert_eq!(editor.text(), "A中");
        editor.move_horizontal(false, true);
        editor.replace("文");
        assert_eq!(editor.text(), "A文");
        editor.move_to(0, false);
        editor.erase(true);
        assert_eq!(editor.text(), "文");
    }

    #[test]
    fn ime_commit_replaces_selection_once_and_clears_marked_text() {
        let mut editor = Editor::default();
        editor.replace("replace");
        editor.select_all();
        editor.set_preedit("拼音", Some((3, 6)));
        assert_eq!(editor.text(), "replace");
        assert_eq!(editor.display(false).0, "拼音");
        editor.replace("中文");
        assert_eq!(editor.text(), "中文");
        assert!(editor.preedit.is_empty());
        assert_eq!(editor.cursor(), 6);
    }

    #[test]
    fn secure_paint_contains_no_plaintext_even_during_ime() {
        let mut editor = Editor::default();
        editor.replace("秘密");
        editor.set_preedit("password", Some((0, 8)));
        let (paint, cursor, marked) = editor.display(true);
        assert!(paint.chars().all(|ch| ch == '•'));
        assert_eq!(paint, "•".repeat(10));
        assert_eq!(cursor, paint.len());
        assert_eq!(marked, 6..30);
        editor.select_all();
        editor.replace("a\n\r\0b\t");
        assert_eq!(editor.text(), "ab");
    }

    #[test]
    fn pointer_positions_inside_clusters_snap_before_whole_graphemes() {
        for cluster in ["e\u{301}", "👩‍🚀", "🇨🇳", "👍🏽", "\u{600}A"] {
            for inside in 1..cluster.len() {
                let mut editor = Editor::default();
                editor.replace(&format!("L{cluster}R"));
                editor.move_to(1 + inside, false);
                assert_eq!(editor.cursor(), 1, "{cluster:?} byte {inside}");
                assert_eq!(editor.selection(), 1..1);
                editor.erase(true);
                assert_eq!(editor.text(), "LR", "{cluster:?} byte {inside}");
            }
        }
    }

    #[test]
    fn pointer_extended_selection_never_splits_cluster_in_either_direction() {
        let mut editor = Editor::default();
        editor.replace("Le\u{301}R");
        editor.move_to(0, false);
        editor.move_to(3, true);
        assert_eq!(editor.selection(), 0..1);
        editor.replace("Q");
        assert_eq!(editor.text(), "Qe\u{301}R");
        editor.move_to(usize::MAX, false);
        editor.move_to(3, true);
        assert_eq!(editor.selection(), 1..5);
        editor.replace("X");
        assert_eq!(editor.text(), "QX");
    }

    #[test]
    fn insertions_that_merge_graphemes_keep_caret_after_complete_cluster() {
        for (initial, position, insertion, result) in [
            ("👩🚀", "👩".len(), "\u{200d}", "👩‍🚀"),
            ("🇳", 0, "🇨", "🇨🇳"),
            ("A", 0, "\u{600}", "\u{600}A"),
        ] {
            let mut editor = Editor::default();
            editor.replace(initial);
            editor.move_to(position, false);
            editor.replace(insertion);
            assert_eq!(editor.text(), result);
            assert_eq!(editor.cursor(), result.len());
            assert_eq!(editor.selection(), result.len()..result.len());
            editor.erase(false);
            assert_eq!(editor.text(), "");
        }
    }

    #[test]
    fn deleting_between_clusters_recomputes_joined_cluster_boundary() {
        let mut editor = Editor::default();
        editor.replace("🇨X🇳");
        editor.move_to("🇨".len(), false);
        editor.erase(true);
        assert_eq!(editor.text(), "🇨🇳");
        assert_eq!(editor.cursor(), "🇨🇳".len());
        editor.erase(false);
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn ime_cursor_offsets_are_safe_but_do_not_rewrite_ime_character_selection() {
        let mut editor = Editor::default();
        editor.replace("A");
        editor.set_preedit("中e\u{301}", Some((1, usize::MAX)));
        assert_eq!(editor.preedit_cursor, Some((0, 6)));
        assert_eq!(editor.display(false), ("A中e\u{301}".to_owned(), 7, 1..7));
        // Unlike committed editing, an IME may intentionally select part of
        // its current marked grapheme while composing that grapheme.
        editor.set_preedit("e\u{301}", Some((1, 1)));
        assert_eq!(editor.preedit_cursor, Some((1, 1)));
        assert_eq!(editor.text(), "A");
        editor.replace("e\u{301}");
        assert_eq!(editor.text(), "Ae\u{301}");
        assert_eq!(editor.cursor(), 4);
        editor.erase(false);
        assert_eq!(editor.text(), "A");
    }
}
