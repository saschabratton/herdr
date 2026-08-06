use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What applying a key event to a [`LineEditor`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEditOutcome {
    TextChanged,
    CursorMoved,
    Ignored,
}

/// Single-line input buffer with a cursor, shared by the rename modals and
/// the worktree branch input.
///
/// The cursor is a byte offset into `text`, always on a char boundary and
/// never past `text.len()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineEditor {
    text: String,
    cursor: usize,
    /// When set, the next text-changing edit replaces the prefilled content
    /// instead of editing it; cursor movement clears the flag (deselect).
    pub replace_on_type: bool,
}

impl LineEditor {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replace the content, placing the cursor at the end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.replace_on_type = false;
    }

    /// Replace the content with a prefill, placing the cursor at the end.
    pub fn prefill(&mut self, text: impl Into<String>, replace_on_type: bool) {
        self.set_text(text);
        self.replace_on_type = replace_on_type;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.replace_on_type = false;
    }

    /// Enforce the replace-on-type rule: while the flag is set, the first
    /// text-changing edit discards the prefilled content wholesale. Returns
    /// true when the buffer was cleared.
    fn clear_prefill(&mut self) -> bool {
        if self.replace_on_type {
            self.clear();
            return true;
        }
        false
    }

    pub fn insert_str(&mut self, text: &str) {
        self.clear_prefill();
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    pub fn backspace(&mut self) {
        if self.clear_prefill() {
            return;
        }
        if let Some(ch) = self.text[..self.cursor].chars().next_back() {
            self.cursor -= ch.len_utf8();
            self.text.remove(self.cursor);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.clear_prefill() {
            return;
        }
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    pub fn kill_to_end(&mut self) {
        if self.clear_prefill() {
            return;
        }
        self.text.truncate(self.cursor);
    }

    pub fn kill_to_start(&mut self) {
        if self.clear_prefill() {
            return;
        }
        self.text.drain(..self.cursor);
        self.cursor = 0;
    }

    /// Delete the word before the cursor: trailing whitespace first, then a
    /// run of same-class chars (word chars vs separators), preserving the
    /// text after the cursor.
    pub fn delete_word_back(&mut self) {
        if self.clear_prefill() {
            return;
        }
        let tail_start = self.cursor;
        let mut head_end = skip_left(&self.text, self.cursor, char::is_whitespace);
        if let Some(word) = self.text[..head_end].chars().next_back().map(is_word_char) {
            head_end = skip_left(&self.text, head_end, |ch| {
                !ch.is_whitespace() && is_word_char(ch) == word
            });
        }
        self.text.drain(head_end..tail_start);
        self.cursor = head_end;
    }

    pub fn move_left(&mut self) {
        self.replace_on_type = false;
        if let Some(ch) = self.text[..self.cursor].chars().next_back() {
            self.cursor -= ch.len_utf8();
        }
    }

    pub fn move_right(&mut self) {
        self.replace_on_type = false;
        if let Some(ch) = self.text[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
    }

    pub fn move_start(&mut self) {
        self.replace_on_type = false;
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.replace_on_type = false;
        self.cursor = self.text.len();
    }

    pub fn move_word_left(&mut self) {
        self.replace_on_type = false;
        self.cursor = skip_left(&self.text, self.cursor, |ch| !is_word_char(ch));
        self.cursor = skip_left(&self.text, self.cursor, is_word_char);
    }

    pub fn move_word_right(&mut self) {
        self.replace_on_type = false;
        self.cursor = skip_right(&self.text, self.cursor, |ch| !is_word_char(ch));
        self.cursor = skip_right(&self.text, self.cursor, is_word_char);
    }

    pub fn apply_key(&mut self, key: &KeyEvent) -> LineEditOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('a') if ctrl => self.move_start(),
            KeyCode::Char('e') if ctrl => self.move_end(),
            KeyCode::Char('b') if ctrl => self.move_left(),
            KeyCode::Char('f') if ctrl => self.move_right(),
            KeyCode::Char('b') if alt => self.move_word_left(),
            KeyCode::Char('f') if alt => self.move_word_right(),
            KeyCode::Left if alt => self.move_word_left(),
            KeyCode::Right if alt => self.move_word_right(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.move_start(),
            KeyCode::End => self.move_end(),
            _ => return self.apply_edit_key(key),
        }
        LineEditOutcome::CursorMoved
    }

    fn apply_edit_key(&mut self, key: &KeyEvent) -> LineEditOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('k') if ctrl => self.kill_to_end(),
            KeyCode::Char('u') if ctrl => self.kill_to_start(),
            KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
                self.kill_to_start();
            }
            KeyCode::Backspace if ctrl || alt => self.delete_word_back(),
            KeyCode::Char('h' | 'w') if ctrl => self.delete_word_back(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete_forward(),
            KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
                self.insert_str(c.encode_utf8(&mut [0; 4]));
            }
            _ => return LineEditOutcome::Ignored,
        }
        LineEditOutcome::TextChanged
    }
}

impl From<&str> for LineEditor {
    fn from(text: &str) -> Self {
        let mut editor = Self::default();
        editor.set_text(text);
        editor
    }
}

impl PartialEq<&str> for LineEditor {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl PartialEq<String> for LineEditor {
    fn eq(&self, other: &String) -> bool {
        &self.text == other
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Walk `at` left past chars matching `pred`, staying on char boundaries.
fn skip_left(text: &str, mut at: usize, mut pred: impl FnMut(char) -> bool) -> usize {
    while let Some(ch) = text[..at].chars().next_back() {
        if !pred(ch) {
            return at;
        }
        at -= ch.len_utf8();
    }
    at
}

/// Walk `at` right past chars matching `pred`, staying on char boundaries.
fn skip_right(text: &str, mut at: usize, mut pred: impl FnMut(char) -> bool) -> usize {
    while let Some(ch) = text[at..].chars().next() {
        if !pred(ch) {
            return at;
        }
        at += ch.len_utf8();
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn movement_keys_position_the_cursor() {
        let mut editor = LineEditor::from("ab cd");
        assert_eq!(editor.cursor(), 5);

        assert_eq!(
            editor.apply_key(&key(KeyCode::Home, KeyModifiers::empty())),
            LineEditOutcome::CursorMoved
        );
        assert_eq!(editor.cursor(), 0);

        editor.apply_key(&key(KeyCode::Right, KeyModifiers::empty()));
        assert_eq!(editor.cursor(), 1);
        editor.apply_key(&key(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(editor.cursor(), 2);
        editor.apply_key(&key(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(editor.cursor(), 1);
        editor.apply_key(&key(KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert_eq!(editor.cursor(), 5);
        editor.apply_key(&key(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(editor.cursor(), 0);
        editor.apply_key(&key(KeyCode::End, KeyModifiers::empty()));
        assert_eq!(editor.cursor(), 5);
        editor.apply_key(&key(KeyCode::Left, KeyModifiers::empty()));
        assert_eq!(editor.cursor(), 4);
    }

    #[test]
    fn word_movement_skips_separators_then_word_chars() {
        let mut editor = LineEditor::from("foo bar-baz");
        editor.apply_key(&key(KeyCode::Char('b'), KeyModifiers::ALT));
        assert_eq!(editor.cursor(), 8);
        editor.apply_key(&key(KeyCode::Left, KeyModifiers::ALT));
        assert_eq!(editor.cursor(), 4);
        editor.apply_key(&key(KeyCode::Char('b'), KeyModifiers::ALT));
        assert_eq!(editor.cursor(), 0);

        editor.apply_key(&key(KeyCode::Char('f'), KeyModifiers::ALT));
        assert_eq!(editor.cursor(), 3);
        editor.apply_key(&key(KeyCode::Right, KeyModifiers::ALT));
        assert_eq!(editor.cursor(), 7);
        editor.apply_key(&key(KeyCode::Char('f'), KeyModifiers::ALT));
        assert_eq!(editor.cursor(), 11);
    }

    #[test]
    fn insert_after_move_start_prepends() {
        let mut editor = LineEditor::from("website");
        editor.apply_key(&key(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(
            editor.apply_key(&key(KeyCode::Char('Z'), KeyModifiers::SHIFT)),
            LineEditOutcome::TextChanged
        );
        assert_eq!(editor, "Zwebsite");
        assert_eq!(editor.cursor(), 1);
    }

    #[test]
    fn kill_to_end_truncates_at_cursor() {
        let mut editor = LineEditor::from("branch-name");
        editor.move_start();
        for _ in 0..6 {
            editor.move_right();
        }
        editor.apply_key(&key(KeyCode::Char('k'), KeyModifiers::CONTROL));
        assert_eq!(editor, "branch");
        assert_eq!(editor.cursor(), 6);
    }

    #[test]
    fn kill_to_start_at_end_clears_everything() {
        let mut editor = LineEditor::from("website zero");
        editor.apply_key(&key(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert!(editor.is_empty());
        assert_eq!(editor.cursor(), 0);
    }

    #[test]
    fn kill_to_start_mid_string_keeps_tail() {
        let mut editor = LineEditor::from("prefix-tail");
        editor.move_start();
        for _ in 0..7 {
            editor.move_right();
        }
        editor.kill_to_start();
        assert_eq!(editor, "tail");
        assert_eq!(editor.cursor(), 0);
    }

    #[test]
    fn delete_word_back_mid_string_preserves_tail() {
        let mut editor = LineEditor::from("one two three");
        editor.move_word_left();
        assert_eq!(editor.cursor(), 8);
        editor.delete_word_back();
        assert_eq!(editor, "one three");
        assert_eq!(editor.cursor(), 4);

        let mut editor = LineEditor::from("website-zero");
        editor.delete_word_back();
        assert_eq!(editor, "website-");
    }

    #[test]
    fn delete_forward_removes_char_under_cursor() {
        let mut editor = LineEditor::from("abc");
        editor.move_start();
        editor.apply_key(&key(KeyCode::Delete, KeyModifiers::empty()));
        assert_eq!(editor, "bc");
        editor.move_end();
        editor.apply_key(&key(KeyCode::Delete, KeyModifiers::empty()));
        assert_eq!(editor, "bc");
    }

    #[test]
    fn multibyte_text_round_trips_without_boundary_panics() {
        let mut editor = LineEditor::from("héllo日本👍");
        editor.move_start();
        editor.move_right();
        editor.move_right();
        assert_eq!(editor.cursor(), "hé".len());
        editor.insert_str("ß");
        assert_eq!(editor, "héßllo日本👍");

        editor.move_end();
        editor.backspace();
        assert_eq!(editor, "héßllo日本");
        editor.move_left();
        editor.delete_forward();
        assert_eq!(editor, "héßllo日");
        editor.move_word_left();
        assert_eq!(editor.cursor(), 0);
        editor.kill_to_end();
        assert!(editor.is_empty());
    }

    #[test]
    fn replace_on_type_insert_replaces_prefill() {
        let mut editor = LineEditor::default();
        editor.prefill("generated", true);
        editor.insert_str("x");
        assert_eq!(editor, "x");
        assert!(!editor.replace_on_type);
    }

    #[test]
    fn replace_on_type_delete_and_kill_clear_prefill() {
        for code in [
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Char('k'),
            KeyCode::Char('u'),
            KeyCode::Char('w'),
        ] {
            let mut editor = LineEditor::default();
            editor.prefill("generated", true);
            let modifiers = match code {
                KeyCode::Char(_) => KeyModifiers::CONTROL,
                _ => KeyModifiers::empty(),
            };
            assert_eq!(
                editor.apply_key(&key(code, modifiers)),
                LineEditOutcome::TextChanged
            );
            assert!(editor.is_empty(), "{code:?} should clear the prefill");
            assert!(!editor.replace_on_type);
        }
    }

    #[test]
    fn replace_on_type_movement_only_clears_the_flag() {
        let mut editor = LineEditor::default();
        editor.prefill("generated", true);
        editor.apply_key(&key(KeyCode::Left, KeyModifiers::empty()));
        assert_eq!(editor, "generated");
        assert!(!editor.replace_on_type);

        editor.insert_str("x");
        assert_eq!(editor, "generatexd");
    }

    #[test]
    fn unbound_keys_are_ignored() {
        let mut editor = LineEditor::from("text");
        for (code, modifiers) in [
            (KeyCode::Enter, KeyModifiers::empty()),
            (KeyCode::Esc, KeyModifiers::empty()),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
            (KeyCode::Char('x'), KeyModifiers::ALT),
        ] {
            assert_eq!(
                editor.apply_key(&key(code, modifiers)),
                LineEditOutcome::Ignored
            );
        }
        assert_eq!(editor, "text");
    }
}
