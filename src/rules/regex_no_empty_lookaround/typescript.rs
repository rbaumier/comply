//! regex-no-empty-lookaround TypeScript / JavaScript / TSX backend.
//!
//! Flags empty lookarounds (`(?=)`, `(?!)`, `(?<=)`, `(?<!)`) inside
//! the tree-sitter `regex` node's pattern. An empty positive lookaround
//! always matches; an empty negative one always fails. AST gating
//! eliminates FPs from strings containing these tokens.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

const EMPTY_LOOKAROUNDS: &[&str] = &["(?=)", "(?!)", "(?<=)", "(?<!)"];

fn has_empty_lookaround(pattern: &str) -> bool {
    EMPTY_LOOKAROUNDS.iter().any(|n| pattern.contains(n))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_empty_lookaround(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-empty-lookaround",
        "Empty lookaround always matches or always fails \u{2014} add a pattern or remove it.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_lookahead() {
        assert_eq!(run_on("const re = /foo(?=)/;").len(), 1);
    }

    #[test]
    fn flags_empty_negative_lookahead() {
        assert_eq!(run_on("const re = /foo(?!)/;").len(), 1);
    }

    #[test]
    fn flags_empty_lookbehind() {
        assert_eq!(run_on("const re = /(?<=)bar/;").len(), 1);
    }

    #[test]
    fn flags_empty_negative_lookbehind() {
        assert_eq!(run_on("const re = /(?<!)bar/;").len(), 1);
    }

    #[test]
    fn allows_non_empty_lookahead() {
        assert!(run_on("const re = /foo(?=bar)/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
