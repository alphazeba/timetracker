use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Split `text` into spans, highlighting every case-insensitive match of `term`.
/// Non-matching segments get `base_style`; matches get black-on-pink bold.
///
/// Works in char space to avoid byte-boundary panics with multi-byte chars
/// whose lowercase forms have different byte lengths (e.g. 'İ' → "i\u{307}").
pub fn highlight_spans<'a>(text: &'a str, term: &str, base_style: Style) -> Vec<Span<'a>> {
    if term.is_empty() {
        return vec![Span::styled(text.to_string(), base_style)];
    }
    let match_style = base_style
        .fg(Color::Black)
        .bg(Color::Rgb(255, 182, 193))
        .add_modifier(Modifier::BOLD);

    let text_chars: Vec<char> = text.chars().collect();
    let lower_text_chars: Vec<char> = text.to_lowercase().chars().collect();
    let lower_term_chars: Vec<char> = term.to_lowercase().chars().collect();
    let term_len = lower_term_chars.len();

    let mut matches: Vec<usize> = Vec::new();
    let mut i = 0;
    while i + term_len <= lower_text_chars.len() {
        if lower_text_chars[i..i + term_len] == lower_term_chars[..] {
            matches.push(i);
            i += term_len;
        } else {
            i += 1;
        }
    }

    if matches.is_empty() {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    let byte_offsets: Vec<usize> = text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(text.len()))
        .collect();
    let char_slice = |from: usize, to: usize| -> &str {
        &text[byte_offsets[from]..byte_offsets[to]]
    };

    let mut spans = Vec::new();
    let mut pos = 0;
    for start in matches {
        if start > pos {
            spans.push(Span::styled(char_slice(pos, start).to_string(), base_style));
        }
        let end = start + term_len;
        spans.push(Span::styled(char_slice(start, end).to_string(), match_style));
        pos = end;
    }
    if pos < text_chars.len() {
        spans.push(Span::styled(char_slice(pos, text_chars.len()).to_string(), base_style));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: 'İ' (U+0130, 2 bytes) lowercases to "i\u{307}" (3 bytes).
    /// The old byte-index approach would panic slicing text[0..1] inside 'İ'.
    #[test]
    fn highlight_spans_multibyte_no_panic() {
        let style = Style::default();
        let spans = highlight_spans("İstanbul", "i", style);
        let combined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(combined, "İstanbul");
    }
}
