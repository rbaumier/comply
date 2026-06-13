//! regex-no-useless-flag TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. Detects flags that have no
//! effect on the surrounding pattern:
//!
//! - `i` when the pattern has no literal letters (case-folding is a
//!   no-op).
//! - `m` when the pattern has no `^` or `$` anchors.
//! - `s` when the pattern has no `.` to re-scope.
//!
//! AST-only detection eliminates the TextCheck false-positive class
//! where URLs, import paths, and Tailwind arbitrary-value strings
//! were parsed as `/pattern/flags`.

use crate::diagnostic::{Diagnostic, Severity};

/// A letter `/i` can fold: alphabetic with a distinct upper/lowercase form.
/// Covers non-ASCII case pairs (`č`/`Č`, `é`/`É`, `ñ`/`Ñ`) and excludes
/// caseless alphabetics (CJK, etc.) where `/i` truly has no effect.
fn is_case_variable_letter(c: char) -> bool {
    c.is_alphabetic() && (c.to_lowercase().next() != Some(c) || c.to_uppercase().next() != Some(c))
}

/// `/i` matters iff the pattern holds a letter with a case to fold. Letters
/// inside a character class (`[a-z]`, `[cč]`) are case-sensitive too, so they
/// count. Iterates by `char` so multi-byte UTF-8 letters are seen as letters,
/// not as ASCII-failing continuation bytes.
fn pattern_has_case_variable_letter(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        match chars[k] {
            '\\' => {
                k += 2; // escape sequence (`\d`, `\w`, …) — never a literal letter
            }
            '[' => {
                // Scan the class body: any letter inside is case-sensitive.
                k += 1;
                while k < chars.len() && chars[k] != ']' {
                    if chars[k] == '\\' {
                        k += 1;
                    } else if is_case_variable_letter(chars[k]) {
                        return true;
                    }
                    k += 1;
                }
                k += 1; // past `]`
            }
            c if is_case_variable_letter(c) => return true,
            _ => k += 1,
        }
    }
    false
}

fn has_useless_flag(pattern: &str, flags: &str) -> bool {
    let pbytes = pattern.as_bytes();

    // `i` is useless only when the pattern has no foldable letter.
    if flags.contains('i') && !pattern_has_case_variable_letter(pattern) {
        return true;
    }

    // `m` flag with no ^ or $
    if flags.contains('m') {
        let has_anchor = pbytes.contains(&b'^') || pbytes.contains(&b'$');
        if !has_anchor {
            return true;
        }
    }

    // `s` flag with no unescaped `.`
    if flags.contains('s') {
        let mut k = 0;
        let mut has_dot = false;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2;
                continue;
            }
            if pbytes[k] == b'.' {
                has_dot = true;
                break;
            }
            k += 1;
        }
        if !has_dot {
            return true;
        }
    }

    false
}

/// Extract pattern + flags from a tree-sitter `regex` node. Prefers
/// named fields; falls back to `/pattern/flags` text parsing when the
/// fields aren't exposed.
fn regex_parts<'a>(node: &tree_sitter::Node<'_>, source: &'a [u8]) -> Option<(&'a str, &'a str)> {
    let pattern = node
        .child_by_field_name("pattern")
        .and_then(|n| n.utf8_text(source).ok());
    let flags = node
        .child_by_field_name("flags")
        .and_then(|n| n.utf8_text(source).ok());

    if let (Some(p), Some(f)) = (pattern, flags) {
        return Some((p, f));
    }
    if let Some(p) = pattern {
        // Pattern present but no flags node — treat flags as empty.
        return Some((p, ""));
    }

    // Grammar fallback: split the raw text on the last `/`.
    let full = node.utf8_text(source).ok()?;
    let inner = full.strip_prefix('/')?;
    let last_slash = inner.rfind('/')?;
    Some((&inner[..last_slash], &inner[last_slash + 1..]))
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, flags)) = regex_parts(&node, source) else { return };
    if flags.is_empty() {
        return;
    }
    if !has_useless_flag(pattern, flags) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-flag",
        "Regex flag has no effect on this pattern \u{2014} remove it.".into(),
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
    fn flags_useless_i_flag() {
        assert_eq!(run_on(r#"const re = /\d+/i;"#).len(), 1);
    }

    #[test]
    fn allows_useful_i_flag() {
        assert!(run_on(r#"const re = /foo/i;"#).is_empty());
    }

    #[test]
    fn flags_useless_m_flag() {
        assert_eq!(run_on(r#"const re = /foo/m;"#).len(), 1);
    }

    #[test]
    fn flags_useless_s_flag() {
        assert_eq!(run_on(r#"const re = /foo/s;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://localhost:6762/api/v1/diffs/query";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    // --- Regression: letters inside character classes are case-sensitive,
    //     so `/i` is meaningful there (issue #1907). These run through the
    //     production oxc backend via the rule registry. ---

    fn run_oxc(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id("regex-no-useless-flag", source, "t.ts")
    }

    #[test]
    fn allows_i_flag_with_letters_only_in_char_class() {
        // /^[jfmasond]/i — without /i this misses "J", "F", … so /i matters.
        assert!(run_oxc(r#"const re = /^[jfmasond]/i;"#).is_empty());
    }

    #[test]
    fn allows_i_flag_with_letter_range_in_char_class() {
        assert!(run_oxc(r#"const re = /[a-z]/i;"#).is_empty());
        assert!(run_oxc(r#"const re = /[A-Za-z]/i;"#).is_empty());
    }

    #[test]
    fn flags_useless_i_flag_no_letters_anywhere_oxc() {
        // No letters at all (even inside classes) → /i is genuinely useless.
        assert_eq!(run_oxc(r#"const re = /\d+/i;"#).len(), 1);
        assert_eq!(run_oxc(r#"const re = /[0-9]/i;"#).len(), 1);
    }

    // --- Regression: non-ASCII letters have case pairs (`č`/`Č`), so `/i`
    //     is meaningful for them, top-level and inside character classes
    //     (issue #1908). ---

    #[test]
    fn allows_i_flag_with_non_ascii_letter_in_char_class() {
        // /^[cč]/i from date-fns sl locale — `č`/`Č` is a case pair.
        assert!(run_oxc(r#"const re = /^[cč]/i;"#).is_empty());
        // Non-ASCII letter alone in the class, no ASCII letter to mask it.
        assert!(run_oxc(r#"const re = /^[č]/i;"#).is_empty());
    }

    #[test]
    fn allows_i_flag_with_non_ascii_letter_top_level() {
        assert!(run_oxc(r#"const re = /čšž/i;"#).is_empty());
        assert!(run_oxc(r#"const re = /é/i;"#).is_empty());
        assert!(run_oxc(r#"const re = /ñ/i;"#).is_empty());
    }
}
