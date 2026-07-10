//! A minimal, single-pass Rust-source tokenizer used only by
//! `validate::refs` to (a) find every double-quoted string literal in a
//! file and (b) know which of those literals sit inside a
//! `#[cfg(test)]`-attributed item, so test-only asset-path fixtures (e.g.
//! `"fonts/does-not-exist.ttf"` in `src/core/mod.rs`) are never mistaken
//! for production references (#185).
//!
//! This is deliberately not a real Rust parser. It tracks just enough
//! structure -- string/char literals, `//` and `/* */` comments, and
//! `{`/`}`/`;` -- to find item boundaries. It handles:
//!
//! - Regular string literals, with backslash-escaped quotes.
//! - Raw strings: `r"..."`, `r#"..."#`, `r##"..."##`, and their `b`-prefixed
//!   byte-string equivalents.
//! - `'x'` char literals, disambiguated from lifetimes (`'a`, `'static`): a
//!   `'` only opens a char literal if a closing `'` follows within a short,
//!   bounded window.
//! - `//` line comments and `/* */` block comments (non-nested).
//!
//! It deliberately does not handle nested block comments or any construct
//! not observed in this repository's `src/` tree (verified by inspection
//! while building this check); an unusual construct would at worst shift
//! an item boundary by a few bytes, not produce a wrong file's worth of
//! false positives/negatives, since every check downstream only cares
//! about whether a given string literal's span falls inside a test range.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    OpenBrace,
    CloseBrace,
    Semicolon,
    /// A string (or byte-string/raw-string) literal; the content is the
    /// exact source bytes between the delimiters, not unescaped -- asset
    /// paths never contain backslash escapes, so this is enough.
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Byte offset of the token's first byte (opening quote for `Str`).
    pub pos: usize,
    /// Byte offset just past the token's last byte (closing quote for
    /// `Str`).
    pub end: usize,
}

/// Tokenizes `source` in one forward pass. See module docs for exactly
/// what is and isn't recognized.
pub fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];

        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }

        if let Some((opener_len, hashes)) = raw_string_opener(bytes, i) {
            let content_start = i + opener_len;
            let closer: String = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            if let Some(rel) = source.get(content_start..).and_then(|s| s.find(&closer)) {
                let content_end = content_start + rel;
                let end = content_end + closer.len();
                tokens.push(Token {
                    kind: TokenKind::Str(source[content_start..content_end].to_string()),
                    pos: i,
                    end,
                });
                i = end;
            } else {
                i = bytes.len();
            }
            continue;
        }

        if b == b'"' || (b == b'b' && bytes.get(i + 1) == Some(&b'"')) {
            let start = i;
            let content_start = if b == b'b' { i + 2 } else { i + 1 };
            let mut j = content_start;
            let mut escaped = false;
            while j < bytes.len() {
                if escaped {
                    escaped = false;
                    j += 1;
                    continue;
                }
                match bytes[j] {
                    b'\\' => {
                        escaped = true;
                        j += 1;
                    }
                    b'"' => {
                        j += 1;
                        break;
                    }
                    _ => j += 1,
                }
            }
            let content_end = j.saturating_sub(1).max(content_start).min(source.len());
            tokens.push(Token {
                kind: TokenKind::Str(source[content_start..content_end].to_string()),
                pos: start,
                end: j,
            });
            i = j;
            continue;
        }

        // Not a char literal (a lifetime, most likely) falls through and
        // consumes just the quote byte via the `match b` below.
        if b == b'\''
            && let Some(end) = char_literal_end(bytes, i)
        {
            i = end;
            continue;
        }

        match b {
            b'{' => {
                tokens.push(Token {
                    kind: TokenKind::OpenBrace,
                    pos: i,
                    end: i + 1,
                });
            }
            b'}' => {
                tokens.push(Token {
                    kind: TokenKind::CloseBrace,
                    pos: i,
                    end: i + 1,
                });
            }
            b';' => {
                tokens.push(Token {
                    kind: TokenKind::Semicolon,
                    pos: i,
                    end: i + 1,
                });
            }
            _ => {}
        }
        i += 1;
    }
    tokens
}

/// Detects a raw-string opener (`r"`, `r#"`, `r##"`, ... and the
/// `b`-prefixed byte-string equivalents) at `i`. Returns
/// `(opener_byte_len, hash_count)` on a match.
fn raw_string_opener(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if bytes.get(j) == Some(&b'b') {
        j += 1;
    }
    if bytes.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let hashes_start = j;
    while bytes.get(j) == Some(&b'#') {
        j += 1;
    }
    let hash_count = j - hashes_start;
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    j += 1;
    Some((j - i, hash_count))
}

/// Returns the byte offset just past a `'x'`-style char literal starting
/// at `i` (which points at the opening `'`), or `None` if `i` is more
/// likely a lifetime (`'a`) or other non-char-literal use of `'`.
fn char_literal_end(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if bytes.get(j) == Some(&b'\\') {
        j += 1;
        // Bounded scan for the closing quote of an escape sequence
        // (`\n`, `\'`, `\u{2019}`, ...); real Rust escapes never need
        // more than a handful of bytes.
        let limit = (j + 10).min(bytes.len());
        while j < limit {
            if bytes[j] == b'\'' {
                return Some(j + 1);
            }
            j += 1;
        }
        return None;
    }
    if j < bytes.len() {
        let char_len = utf8_len(bytes[j]);
        let after = j + char_len;
        if bytes.get(after) == Some(&b'\'') {
            return Some(after + 1);
        }
    }
    None
}

fn utf8_len(first_byte: u8) -> usize {
    if first_byte & 0x80 == 0 {
        1
    } else if first_byte & 0xE0 == 0xC0 {
        2
    } else if first_byte & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}

/// Finds the byte ranges of every `#[cfg(test)]`-attributed item in
/// `source` (attribute start through the item's closing `}`/`;`).
pub fn test_code_ranges(source: &str, tokens: &[Token]) -> Vec<Range<usize>> {
    const MARKER: &str = "#[cfg(test)]";
    let mut ranges = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = source.get(search_from..).and_then(|s| s.find(MARKER)) {
        let attr_start = search_from + rel;
        let after_attr = attr_start + MARKER.len();
        let item_start = skip_trivia_and_attributes(source.as_bytes(), after_attr);
        let end = item_end(tokens, item_start).max(after_attr);
        ranges.push(attr_start..end);
        search_from = end;
    }
    ranges
}

/// Skips whitespace, `//`/`/* */` comments, and any further `#[...]`
/// attributes, returning the offset of the actual item that follows.
fn skip_trivia_and_attributes(bytes: &[u8], mut i: usize) -> usize {
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if bytes.get(i) == Some(&b'#') && bytes.get(i + 1) == Some(&b'[') {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        break;
    }
    i
}

/// Finds where the item starting at `item_start` ends: the first top-level
/// `;` if no top-level `{` is seen first (a statement-like item: `const`,
/// `use`, a unit/tuple `struct`, ...), otherwise the matching `}` for the
/// item's outermost `{` (a `fn`/`mod`/`impl`/braced-`struct`/... body).
fn item_end(tokens: &[Token], item_start: usize) -> usize {
    let start_idx = tokens.partition_point(|t| t.pos < item_start);
    let mut depth: i32 = 0;
    let mut in_body = false;
    for token in &tokens[start_idx..] {
        match &token.kind {
            TokenKind::OpenBrace => {
                in_body = true;
                depth += 1;
            }
            TokenKind::CloseBrace => {
                depth -= 1;
                if in_body && depth <= 0 {
                    return token.end;
                }
            }
            TokenKind::Semicolon => {
                if !in_body {
                    return token.end;
                }
            }
            TokenKind::Str(_) => {}
        }
    }
    tokens.last().map(|t| t.end).unwrap_or(item_start)
}

/// Every string literal in `tokens` whose span does not fall inside any of
/// `test_ranges`, paired with its 1-based line number in `source` (for
/// diagnostics).
pub fn production_string_literals<'a>(
    source: &str,
    tokens: &'a [Token],
    test_ranges: &[Range<usize>],
) -> Vec<(usize, &'a str)> {
    tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokenKind::Str(content) => Some((t.pos, t.end, content.as_str())),
            _ => None,
        })
        .filter(|(pos, end, _)| !test_ranges.iter().any(|r| r.start <= *pos && *end <= r.end))
        .map(|(pos, _, content)| (line_of(source, pos), content))
        .collect()
}

fn line_of(source: &str, byte_pos: usize) -> usize {
    source
        .get(..byte_pos)
        .map(|prefix| prefix.matches('\n').count() + 1)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_plain_string_literal() {
        let source = r#"const FOO: &str = "sprites/player.png";"#;
        let tokens = tokenize(source);
        let ranges = test_code_ranges(source, &tokens);
        let refs = production_string_literals(source, &tokens, &ranges);
        assert_eq!(refs, vec![(1, "sprites/player.png")]);
    }

    #[test]
    fn excludes_a_string_literal_inside_a_cfg_test_fn() {
        let source = concat!(
            "const REAL: &str = \"sprites/player.png\";\n",
            "#[cfg(test)]\n",
            "fn broken() -> &'static str {\n",
            "    \"fonts/does-not-exist.ttf\"\n",
            "}\n",
            "const AFTER: &str = \"audio/music_menu.ogg\";\n",
        );
        let tokens = tokenize(source);
        let ranges = test_code_ranges(source, &tokens);
        let refs = production_string_literals(source, &tokens, &ranges);
        assert_eq!(
            refs,
            vec![(1, "sprites/player.png"), (6, "audio/music_menu.ogg")]
        );
    }

    #[test]
    fn excludes_a_string_literal_inside_a_cfg_test_mod() {
        let source = concat!(
            "const REAL: &str = \"ui/icon_coin.png\";\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    #[test]\n",
            "    fn it_matches() {\n",
            "        assert_eq!(super::REAL, \"ui/icon_coin.png\");\n",
            "    }\n",
            "}\n",
        );
        let tokens = tokenize(source);
        let ranges = test_code_ranges(source, &tokens);
        let refs = production_string_literals(source, &tokens, &ranges);
        assert_eq!(refs, vec![(1, "ui/icon_coin.png")]);
    }

    #[test]
    fn excludes_a_cfg_test_const_ending_in_a_semicolon() {
        let source = concat!(
            "#[cfg(test)]\n",
            "const FIXTURE: &str = \"gear/does-not-exist.png\";\n",
            "const REAL: &str = \"gear/buzdugan_cu_trei_peceti.png\";\n",
        );
        let tokens = tokenize(source);
        let ranges = test_code_ranges(source, &tokens);
        let refs = production_string_literals(source, &tokens, &ranges);
        assert_eq!(refs, vec![(3, "gear/buzdugan_cu_trei_peceti.png")]);
    }

    #[test]
    fn a_lifetime_is_not_mistaken_for_a_char_literal() {
        let source = "fn foo<'a>(x: &'a str) -> &'a str { x }";
        // Must not panic or miscount braces; there is exactly one brace
        // pair around the trivial function body.
        let tokens = tokenize(source);
        let opens = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::OpenBrace)
            .count();
        let closes = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::CloseBrace)
            .count();
        assert_eq!(opens, 1);
        assert_eq!(closes, 1);
    }

    #[test]
    fn a_raw_string_containing_braces_and_quotes_does_not_confuse_brace_tracking() {
        let source = concat!(
            "#[cfg(test)]\n",
            "fn json_fixture() -> &'static str {\n",
            "    r#\"{\"version\":2,\"path\":\"sprites/player.png\"}\"#\n",
            "}\n",
            "const REAL: &str = \"ui/icon_coin.png\";\n",
        );
        let tokens = tokenize(source);
        let ranges = test_code_ranges(source, &tokens);
        let refs = production_string_literals(source, &tokens, &ranges);
        assert_eq!(refs, vec![(5, "ui/icon_coin.png")]);
    }

    #[test]
    fn a_char_literal_brace_does_not_confuse_brace_tracking() {
        let source = concat!(
            "#[cfg(test)]\n",
            "fn opens_brace() -> char {\n",
            "    '{'\n",
            "}\n",
            "const REAL: &str = \"audio/sfx_hit.ogg\";\n",
        );
        let tokens = tokenize(source);
        let ranges = test_code_ranges(source, &tokens);
        let refs = production_string_literals(source, &tokens, &ranges);
        assert_eq!(refs, vec![(5, "audio/sfx_hit.ogg")]);
    }
}
