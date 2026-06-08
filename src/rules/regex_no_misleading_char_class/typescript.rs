//! regex-no-misleading-char-class TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and scoped import paths
//! inside string literals cannot false-positive as regex char classes.

use crate::diagnostic::{Diagnostic, Severity};

const ZWJ: char = '\u{200D}';

/// Scans a regex pattern for `[...]` character classes containing
/// multi-codepoint graphemes: chars above U+FFFF (which JS regex splits
/// into surrogate pairs) or ZWJ sequences (which are split grapheme-wise).
fn has_misleading_char_class(pattern: &str) -> bool {
    let mut in_class = false;
    let mut escaped = false;
    for ch in pattern.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '[' && !in_class {
            in_class = true;
            continue;
        }
        if ch == ']' && in_class {
            in_class = false;
            continue;
        }
        if in_class && (ch as u32 > 0xFFFF || ch == ZWJ) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = crate::rules::regex_ast::pattern_and_flags(&node, source) else {
        return;
    };
    if !has_misleading_char_class(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-misleading-char-class",
        "Character class contains multi-codepoint graphemes \u{2014} they will be split into individual code points.".into(),
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
    fn flags_emoji_in_char_class() {
        // U+1F468 is above U+FFFF
        let code = "const re = /[\u{1F468}]/;";
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn flags_zwj_in_char_class() {
        // Family emoji with ZWJ
        let code = "const re = /[\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}]/;";
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_ascii_char_class() {
        assert!(run_on("const re = /[abc]/;").is_empty());
    }

    #[test]
    fn allows_emoji_outside_char_class() {
        assert!(run_on("const re = /\u{1F468}/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr] [\u{1F468}]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "https://example.com/[\u{1F468}]/path";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@scope/[\u{1F468}]-pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
