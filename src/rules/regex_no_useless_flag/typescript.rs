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

fn has_useless_flag(pattern: &str, flags: &str) -> bool {
    let pbytes = pattern.as_bytes();

    // `i` is useless only when the pattern has no ASCII letter to case-fold.
    // Letters inside a character class (`[a-z]`, `[jfmasond]`) ARE
    // case-sensitive without `i`, so they count too.
    if flags.contains('i') {
        let mut has_letter = false;
        let mut k = 0;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2; // escape sequence (`\d`, `\w`, …) — never a literal letter
                continue;
            }
            if pbytes[k] == b'[' {
                // Scan the class body: any letter inside is case-sensitive.
                k += 1;
                while k < pbytes.len() && pbytes[k] != b']' {
                    if pbytes[k] == b'\\' {
                        k += 1;
                    } else if pbytes[k].is_ascii_alphabetic() {
                        has_letter = true;
                    }
                    k += 1;
                }
                if has_letter {
                    break;
                }
                continue;
            }
            if pbytes[k].is_ascii_alphabetic() {
                has_letter = true;
                break;
            }
            k += 1;
        }
        if !has_letter {
            return true;
        }
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
}
