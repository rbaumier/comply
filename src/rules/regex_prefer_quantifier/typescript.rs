//! regex-prefer-quantifier TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URL paths, Tailwind arbitrary-value classes, scoped import paths,
//! and multi-byte characters inside comments/strings cannot
//! false-positive or panic on non-char-boundary slicing.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Tokenize a regex pattern into elements (single chars or escape
/// sequences like `\d`). Character classes `[...]`, groups `(` `)`,
/// alternation `|`, and `{m,n}` quantifiers are emitted as opaque
/// tokens so they never participate in repetition runs.
fn tokenize(pattern: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next_len = pattern[i + 1..].chars().next().map_or(1, |c| c.len_utf8());
            tokens.push(&pattern[i..i + 1 + next_len]);
            i += 1 + next_len;
        } else if bytes[i] == b'[' {
            // Skip character class entirely.
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b']' {
                if bytes[i] == b'\\' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1; // skip `]`
            }
            tokens.push(&pattern[start..i]);
        } else if bytes[i] == b'(' || bytes[i] == b')' || bytes[i] == b'|' {
            tokens.push(&pattern[i..i + 1]);
            i += 1;
        } else if bytes[i] == b'{' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            tokens.push(&pattern[start..i]);
        } else {
            let ch_len = pattern[i..].chars().next().map_or(1, |c| c.len_utf8());
            tokens.push(&pattern[i..i + ch_len]);
            i += ch_len;
        }
    }
    tokens
}

/// Detect 3+ consecutive identical tokens in a regex pattern
/// (e.g. `aaa` or `\d\d\d\d`). Structural tokens (groups,
/// alternation, anchors, quantifiers, char classes) are ignored.
fn has_repeated_tokens(pattern: &str) -> bool {
    let tokens = tokenize(pattern);
    let mut run = 1;
    for i in 1..tokens.len() {
        let prev = tokens[i - 1];
        let cur = tokens[i];
        if cur == prev
            && !matches!(cur, "(" | ")" | "|" | "?" | "+" | "*" | "^" | "$" | ".")
            && !cur.starts_with('{')
            && !cur.starts_with('[')
        {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 1;
        }
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_repeated_tokens(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-prefer-quantifier",
        "Repeated identical pattern in regex \u{2014} use a quantifier like `a{3}` or `\\d{4}`.".into(),
        Severity::Warning,
    ));
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_repeated_chars() {
        assert_eq!(run_on("const re = /aaa/;").len(), 1);
    }

    #[test]
    fn flags_repeated_escape() {
        assert_eq!(run_on(r#"const re = /\d\d\d\d/;"#).len(), 1);
    }

    #[test]
    fn allows_two_chars() {
        assert!(run_on("const re = /aa/;").is_empty());
    }

    #[test]
    fn allows_quantifier_already() {
        assert!(run_on("const re = /a{3}/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/aaa/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_panic_on_multibyte_chars() {
        let src = r#"const re = /cabinets vétérinaires/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
