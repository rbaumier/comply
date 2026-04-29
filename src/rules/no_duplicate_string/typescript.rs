//! no-duplicate-string — TS / JS / TSX backend.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        super::collect_diagnostics(tree, ctx, TS_STRING_KINDS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_string_appearing_three_times() {
        let src = r#"
            const a = "hello world";
            const b = "hello world";
            const c = "hello world";
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_fourth_occurrence_too() {
        let src = r#"
            const a = "repeated str";
            const b = "repeated str";
            const c = "repeated str";
            const d = "repeated str";
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn ignores_short_strings() {
        let src = r#"
            const a = "short";
            const b = "short";
            const c = "short";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_two_occurrences() {
        let src = r#"
            const a = "long enough string";
            const b = "long enough string";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_string_in_comment() {
        // The literal `"structured_output"` appears once as a real
        // string but many times in comments — comments are not
        // visited, so the count stays at 1.
        let src = r#"
            // the "structured_output" field is different from "result"
            // we always check "structured_output" first
            // and fall back if "structured_output" is missing
            const FIELD = "structured_output";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_inner_quotes_of_template_string() {
        // The template contains `${var}` but its content is ONE AST
        // node, so it counts as one occurrence.
        let src = r#"
            const x = `outer "inner" outer`;
            const y = `another "inner" another`;
            const z = `final "inner" final`;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_tailwind_classes_in_jsx_classname() {
        // run_ts uses LANGUAGE_TYPESCRIPT which doesn't accept JSX,
        // so this case must use the TSX grammar via run_tsx.
        let src = r#"
            const A = () => <div className="text-muted-foreground">a</div>;
            const B = () => <div className="text-muted-foreground">b</div>;
            const C = () => <div className="text-muted-foreground">c</div>;
            const D = () => <div className="text-muted-foreground">d</div>;
            const E = () => <div className="text-muted-foreground">e</div>;
        "#;
        let diags = crate::rules::test_helpers::run_tsx(src, &Check);
        assert!(diags.is_empty(), "got: {diags:?}");
    }

    #[test]
    fn does_not_flag_class_attribute_in_jsx() {
        let src = r#"
            const A = () => <div class="text-muted-foreground">a</div>;
            const B = () => <div class="text-muted-foreground">b</div>;
            const C = () => <div class="text-muted-foreground">c</div>;
        "#;
        let diags = crate::rules::test_helpers::run_tsx(src, &Check);
        assert!(diags.is_empty(), "got: {diags:?}");
    }

    #[test]
    fn does_not_flag_repeated_import_specifiers() {
        let src = r#"
            import { test } from "@playwright/test";
            import { expect } from "@playwright/test";
            import { describe } from "@playwright/test";
            import { it } from "@playwright/test";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_strings_in_cn_helper() {
        let src = r#"
            const a = cn("text-muted-foreground", x);
            const b = cn("text-muted-foreground", y);
            const c = cn("text-muted-foreground", z);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_strings_in_clsx_helper() {
        let src = r#"
            const a = clsx("rounded-md border", x);
            const b = clsx("rounded-md border", y);
            const c = clsx("rounded-md border", z);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_normal_duplicates_when_jsx_present() {
        // Sanity: the JSX bypass is targeted — repeated strings
        // outside className/class still get flagged.
        let src = r#"
            const A = () => <div className="text-muted-foreground">a</div>;
            const x = "shared identifier value";
            const y = "shared identifier value";
            const z = "shared identifier value";
        "#;
        let diags = crate::rules::test_helpers::run_tsx(src, &Check);
        assert_eq!(diags.len(), 1);
    }
}
