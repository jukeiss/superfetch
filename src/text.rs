//! Measuring, cutting and comparing strings.

use std::borrow::Cow;

use crate::style::RESET;

/// Splits off the escape sequence at the start of `s`, if there is one.
///
/// Only CSI sequences (`ESC [ ... final`) are recognised, which covers
/// everything superfetch emits and what logo files normally contain.
fn split_escape(s: &str) -> Option<(&str, &str)> {
    let body = s.strip_prefix("\x1b[")?;
    let end = body.find(|c: char| ('\x40'..='\x7e').contains(&c))?;
    Some(s.split_at(2 + end + 1))
}

/// Columns a character occupies in a terminal.
///
/// This follows the Unicode East Asian Width classes for the blocks that
/// matter in practice: CJK and emoji are two columns wide, combining marks
/// and other invisible characters none. It is not a full implementation of
/// the standard, only enough to keep columns aligned.
fn char_width(c: char) -> usize {
    match c as u32 {
        // Control characters.
        0x00..=0x1f | 0x7f..=0x9f => 0,
        // Combining marks, zero-width spaces and joiners, variation selectors.
        0x0300..=0x036f
        | 0x0483..=0x0489
        | 0x0591..=0x05bd
        | 0x0610..=0x061a
        | 0x064b..=0x065f
        | 0x1ab0..=0x1aff
        | 0x1dc0..=0x1dff
        | 0x200b..=0x200f
        | 0x202a..=0x202e
        | 0x2060..=0x2064
        | 0x20d0..=0x20f0
        | 0xfe00..=0xfe0f
        | 0xfe20..=0xfe2f
        | 0xfeff
        | 0xe0100..=0xe01ef => 0,
        // Hangul Jamo, CJK, Hangul syllables, fullwidth forms, emoji.
        0x1100..=0x115f
        | 0x2e80..=0x303e
        | 0x3041..=0x33ff
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xa000..=0xa4cf
        | 0xa960..=0xa97f
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe10..=0xfe19
        | 0xfe30..=0xfe6f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x1f300..=0x1f64f
        | 0x1f680..=0x1f6ff
        | 0x1f900..=0x1f9ff
        | 0x1fa70..=0x1faff
        | 0x20000..=0x3fffd => 2,
        _ => 1,
    }
}

/// Number of terminal columns `s` occupies.
pub fn width(s: &str) -> usize {
    let mut rest = s;
    let mut columns = 0;
    while let Some(c) = rest.chars().next() {
        if let Some((_, tail)) = split_escape(rest) {
            rest = tail;
            continue;
        }
        columns += char_width(c);
        rest = &rest[c.len_utf8()..];
    }
    columns
}

/// Shortens `s` to at most `max` columns, marking the cut with an ellipsis.
pub fn truncate(s: &str, max: usize) -> Cow<'_, str> {
    if width(s) <= max {
        return Cow::Borrowed(s);
    }
    let budget = max.saturating_sub(1);
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut columns = 0;
    let mut styled = false;
    while let Some(c) = rest.chars().next() {
        if let Some((escape, tail)) = split_escape(rest) {
            out.push_str(escape);
            styled = true;
            rest = tail;
            continue;
        }
        let w = char_width(c);
        if columns + w > budget {
            break;
        }
        columns += w;
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if max > 0 {
        out.push('…');
    }
    if styled {
        out.push_str(RESET);
    }
    Cow::Owned(out)
}

/// Appends spaces to `out`.
pub fn pad(out: &mut String, columns: usize) {
    out.extend(std::iter::repeat_n(' ', columns));
}

/// The candidate within two edits of `name`, if there is one. Used to
/// turn "unknown module 'gnu'" into "did you mean 'gpu'?".
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    candidates
        .into_iter()
        .map(|candidate| (edit_distance(name, candidate), candidate))
        .filter(|(distance, _)| *distance <= 2)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// Levenshtein distance, one row at a time.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = diagonal + usize::from(ca != *cb);
            diagonal = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_ignores_escapes() {
        assert_eq!(width("plain"), 5);
        assert_eq!(width("\x1b[1;32mgreen\x1b[0m"), 5);
        assert_eq!(width("\x1b[38;2;1;2;3mx"), 1);
    }

    #[test]
    fn width_follows_unicode_classes() {
        assert_eq!(width("━━━"), 3);
        assert_eq!(width("…›‹·─"), 5);
        assert_eq!(width("日本"), 4);
        assert_eq!(width("한글"), 4);
        assert_eq!(width("ｆｕｌｌ"), 8);
        assert_eq!(width("🦀"), 2);
        // "e" followed by a combining acute accent is one column.
        assert_eq!(width("e\u{301}"), 1);
        assert_eq!(width("a\u{200b}b"), 2);
    }

    #[test]
    fn truncate_keeps_short_lines() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exact", 5), "exact");
    }

    #[test]
    fn truncate_counts_columns_not_bytes() {
        assert_eq!(truncate("abcdefgh", 5), "abcd…");
        assert_eq!(
            truncate("\x1b[31mabcdefgh\x1b[0m", 5),
            "\x1b[31mabcd…\x1b[0m"
        );
        assert_eq!(width(&truncate("日本語のテキスト", 6)), 5);
    }

    #[test]
    fn edit_distances() {
        assert_eq!(edit_distance("gnu", "gpu"), 1);
        assert_eq!(edit_distance("memory", "memory"), 0);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn closest_picks_the_nearest_candidate() {
        let names = ["cpu", "gpu", "memory"];
        assert_eq!(closest("gnu", names), Some("gpu"));
        assert_eq!(closest("memroy", names), Some("memory"));
        assert_eq!(closest("xyzzy", names), None);
    }
}
