//! regex-no-obscure-range TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals like Tailwind classes, URLs, and scoped import paths
//! cannot false-positive as regex character classes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast;

/// Known obscure character-class ranges that cross ASCII groups and
/// silently include unexpected characters (e.g. `[A-z]` includes
/// `[\]^_\``). Matched as a literal substring of the regex pattern.
const OBSCURE_RANGES: &[&str] = &[
    "A-z", // includes [\]^_`
    "a-Z", // reversed / nonsensical
    "0-z", // digits + uppercase + symbols + lowercase
    "0-Z", // digits + symbols + uppercase
];

/// Returns true if the regex pattern contains an obscure character-class
/// range. We only look at patterns from real regex nodes, so plain
/// substring search is safe — there's no risk of matching string literals.
fn has_obscure_range(pattern: &str) -> bool {
    OBSCURE_RANGES.iter().any(|range| pattern.contains(range))
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = regex_ast::pattern_and_flags(&node, source) else {
        return;
    };
    if !has_obscure_range(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-obscure-range",
        "Character class range crosses ASCII groups (e.g. `[A-z]`) \u{2014} use `[A-Za-z]` instead.".into(),
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
    fn flags_a_to_z_upper_lower() {
        assert_eq!(run_on("const re = /[A-z]/;").len(), 1);
    }

    #[test]
    fn flags_zero_to_z() {
        assert_eq!(run_on("const re = /[0-z]/;").len(), 1);
    }

    #[test]
    fn allows_proper_range() {
        assert!(run_on("const re = /[A-Za-z]/;").is_empty());
    }

    #[test]
    fn allows_digit_range() {
        assert!(run_on("const re = /[0-9]/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        let src = r#"const x = "grid-cols-[A-z]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/A-z/path";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@scope/0-z-pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
