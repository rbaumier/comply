//! regex-no-standalone-backslash TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals containing backslashes (e.g. Windows paths, escape
//! sequences in Tailwind classes, scoped import paths) cannot
//! false-positive as regex identity escapes.

use crate::diagnostic::{Diagnostic, Severity};

/// Characters that are valid after a backslash in regex.
/// Standard escapes: d, D, w, W, s, S, b, B, n, r, t, f, v, 0,
/// plus anchors / grouping: k, p, P, u, x, c
/// plus regex metacharacters that need escaping: . * + ? ^ $ { } [ ] ( ) | / \
const VALID_AFTER_BACKSLASH: &[u8] = b"dDwWsSnrtfvbB0kpPuxc.*+?^${}[]()|\\/123456789";

/// Returns `true` when the regex pattern contains a backslash followed by
/// a plain ASCII letter that isn't a recognised escape sequence.
fn has_standalone_backslash(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len.saturating_sub(1) {
        if bytes[i] == b'\\' {
            let next = bytes[i + 1];
            if next == b'\\' {
                // Escaped backslash — skip both.
                i += 2;
                continue;
            }
            if !VALID_AFTER_BACKSLASH.contains(&next) && next.is_ascii_alphabetic() {
                return true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = crate::rules::regex_ast::pattern_and_flags(&node, source) else {
        return;
    };
    if !has_standalone_backslash(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-standalone-backslash",
        "Backslash followed by non-special character is an identity escape \u{2014} likely a mistake.".into(),
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
    fn flags_backslash_before_normal_letter() {
        // \a is not a valid regex escape
        assert_eq!(run_on(r#"const re = /\a/;"#).len(), 1);
    }

    #[test]
    fn flags_backslash_e() {
        assert_eq!(run_on(r#"const re = /\e/;"#).len(), 1);
    }

    #[test]
    fn allows_valid_escape_d() {
        assert!(run_on(r#"const re = /\d+/;"#).is_empty());
    }

    #[test]
    fn allows_valid_escape_w() {
        assert!(run_on(r#"const re = /\w+/;"#).is_empty());
    }

    #[test]
    fn allows_escaped_dot() {
        assert!(run_on(r#"const re = /\./;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "bg-[url(\a)]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_backslash_in_url_string() {
        let src = r#"const u = "http://a/\foo";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_empty_scoped_import() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
