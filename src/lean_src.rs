//! Lean source scanning: comment/string stripping, `import` extraction, and
//! forbidden-token detection.
//!
//! This is a Rust port of the `lean_code()` helper in TEAM42's
//! `site-percolation-square-lattice-lean/scripts/audit_axioms.py`, which exists
//! because a naive search would flag `sorry` inside comments and string
//! literals. See `docs/requirements.md` §4.1.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum LeanSrcError {
    UnterminatedBlockComment,
}

impl fmt::Display for LeanSrcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeanSrcError::UnterminatedBlockComment => {
                write!(f, "unterminated block comment")
            }
        }
    }
}

impl std::error::Error for LeanSrcError {}

/// Tokens whose presence in a repository's own Lean sources is a red flag under
/// the TEAM42 proof-delivery contract.
pub const FORBIDDEN_TOKENS: [&str; 6] = [
    "sorry",
    "admit",
    "axiom",
    "unsafe",
    "implemented_by",
    "extern",
];

/// Remove nested block comments, line comments and string literals, replacing
/// removed characters with spaces and preserving newlines so that line numbers
/// stay meaningful and adjacent tokens are never joined.
///
/// Character literals are deliberately *not* stripped: a Lean character literal
/// holds a single character, so it cannot contain any of the forbidden tokens.
pub fn strip_comments_and_strings(text: &str) -> Result<String, LeanSrcError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0usize;
    let mut depth: usize = 0;

    while i < chars.len() {
        let c = chars[i];
        if depth > 0 {
            if starts_with(&chars, i, "/-") {
                depth += 1;
                out.push(' ');
                out.push(' ');
                i += 2;
            } else if starts_with(&chars, i, "-/") {
                depth -= 1;
                out.push(' ');
                out.push(' ');
                i += 2;
            } else {
                out.push(if c == '\n' { '\n' } else { ' ' });
                i += 1;
            }
        } else if starts_with(&chars, i, "/-") {
            depth = 1;
            out.push(' ');
            out.push(' ');
            i += 2;
        } else if starts_with(&chars, i, "--") {
            // Line comment: blank every character up to (not including) the
            // newline, which is then handled normally. One output character per
            // input character keeps offsets aligned with the original text.
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '"' {
            out.push(' ');
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    // Keep the output exactly as long as the input so that a
                    // character index in the stripped text still refers to the
                    // same position in the original.
                    out.push(' ');
                    out.push(if chars[i + 1] == '\n' { '\n' } else { ' ' });
                    i += 2;
                } else if chars[i] == '"' {
                    out.push(' ');
                    i += 1;
                    break;
                } else {
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }

    if depth > 0 {
        return Err(LeanSrcError::UnterminatedBlockComment);
    }
    Ok(out.into_iter().collect())
}

fn starts_with(chars: &[char], i: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    i + p.len() <= chars.len() && chars[i..i + p.len()] == p[..]
}

/// Extract imported module names from already-stripped Lean source.
///
/// Handles `import A.B`, `public import A.B`, `import all A.B` and multiple
/// modules on one line. The `all` keyword is ignored.
pub fn extract_imports(stripped: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in stripped.lines() {
        let line = line.trim_start();
        let rest = if let Some(r) = line.strip_prefix("public import") {
            r
        } else if let Some(r) = line.strip_prefix("import") {
            r
        } else {
            continue;
        };
        // `import` must be followed by whitespace, not be part of an identifier.
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            continue;
        }
        for tok in rest.split_whitespace() {
            if tok == "all" {
                continue;
            }
            if is_module_ident(tok) {
                out.push(tok.to_string());
            }
        }
    }
    out
}

fn is_module_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '\'' || c == '.')
}

/// Find forbidden tokens in already-stripped Lean source.
///
/// Identifier-aware: `sorryAx` does not match `sorry`, matching the behaviour of
/// the `\b(?:sorry|…)\b` regex used by TEAM42, but implemented without a regex
/// dependency and with unicode-aware boundaries.
pub fn scan_forbidden(stripped: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut line = 1usize;
    let mut cur = String::new();

    let flush = |cur: &mut String, line: usize, out: &mut Vec<(usize, String)>| {
        if !cur.is_empty() {
            if FORBIDDEN_TOKENS.contains(&cur.as_str()) {
                out.push((line, cur.clone()));
            }
            cur.clear();
        }
    };

    for c in stripped.chars() {
        if c == '\n' {
            flush(&mut cur, line, &mut out);
            line += 1;
        } else if is_ident_char(c) {
            cur.push(c);
        } else {
            flush(&mut cur, line, &mut out);
        }
    }
    flush(&mut cur, line, &mut out);
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_line_comments() {
        let src = "theorem a : True := trivial -- sorry\ntheorem b : True := trivial\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert!(!s.contains("sorry"));
        assert_eq!(s.lines().count(), src.lines().count());
    }

    #[test]
    fn strips_block_comments_and_keeps_line_count() {
        let src = "/- line1\nline2 sorry\n-/\ntheorem t : True := trivial\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert!(!s.contains("sorry"));
        assert_eq!(s.lines().count(), 4);
        assert!(s.contains("theorem t"));
    }

    #[test]
    fn strips_nested_block_comments() {
        let src = "/- outer /- inner sorry -/ still outer axiom -/\nok\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert!(!s.contains("sorry"));
        assert!(!s.contains("axiom"));
        assert!(s.contains("ok"));
    }

    #[test]
    fn strips_string_literals() {
        let src = "def msg := \"sorry axiom unsafe\"\ndef ok := 1\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert!(!s.contains("sorry"));
        assert!(s.contains("def ok"));
    }

    #[test]
    fn string_escape_does_not_end_string() {
        let src = "def s := \"a\\\"sorry\"\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert!(!s.contains("sorry"));
    }

    #[test]
    fn stripping_preserves_char_length() {
        // This invariant lets callers read a value out of the original text at a
        // position located in the stripped text (used for `elab "..."` names).
        let cases = [
            "def s := \"a\\\"b\"\n",
            "def s := \"\"\n",
            "/- c -/\ntheorem t : True := trivial\n",
            "x -- comment\n",
        ];
        for src in cases {
            let stripped = strip_comments_and_strings(src).unwrap();
            assert_eq!(stripped.chars().count(), src.chars().count(), "src={src:?}");
        }
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        assert_eq!(
            strip_comments_and_strings("/- oops"),
            Err(LeanSrcError::UnterminatedBlockComment)
        );
    }

    #[test]
    fn does_not_join_tokens_across_a_removed_comment() {
        let src = "foo/-x-/bar\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert!(s.contains("foo"));
        assert!(s.contains("bar"));
        assert!(!s.contains("foobar"));
    }

    #[test]
    fn imports_are_extracted() {
        let src =
            "import Mathlib\nimport SitePerc.Base\npublic import SitePerc.Road\nimport A.B C.D\n";
        let got = extract_imports(src);
        assert_eq!(
            got,
            vec!["Mathlib", "SitePerc.Base", "SitePerc.Road", "A.B", "C.D"]
        );
    }

    #[test]
    fn import_inside_comment_is_not_extracted() {
        let src =
            "/- import SitePerc.Hidden -/\n-- import SitePerc.AlsoHidden\nimport SitePerc.Real\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert_eq!(extract_imports(&s), vec!["SitePerc.Real"]);
    }

    #[test]
    fn imports_ignore_the_all_keyword() {
        let got = extract_imports("import all Mathlib\n");
        assert_eq!(got, vec!["Mathlib"]);
    }

    #[test]
    fn scan_finds_forbidden_tokens() {
        let flags = scan_forbidden("theorem t := by sorry\naxiom bad : False\n");
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0], (1, "sorry".to_string()));
        assert_eq!(flags[1], (2, "axiom".to_string()));
    }

    #[test]
    fn scan_does_not_match_inside_identifiers() {
        // `sorryAx` and `unsafeCast` are identifiers, not the forbidden tokens.
        let flags = scan_forbidden("def sorryAx := 1\ndef unsafeCast := 2\n");
        assert!(flags.is_empty(), "unexpected flags: {flags:?}");
    }

    #[test]
    fn scan_is_comment_and_string_aware() {
        let src = "/- sorry -/\ndef s := \"axiom\"\n-- unsafe\ntheorem ok : True := trivial\n";
        let s = strip_comments_and_strings(src).unwrap();
        assert!(scan_forbidden(&s).is_empty());
    }

    #[test]
    fn scan_matches_qualified_names_like_word_boundaries() {
        let flags = scan_forbidden("def x := Foo.axiom\n");
        assert_eq!(flags, vec![(1, "axiom".to_string())]);
    }
}
