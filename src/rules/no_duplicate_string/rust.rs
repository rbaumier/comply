//! no-duplicate-string — Rust backend.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        super::collect_diagnostics(tree, ctx, RUST_STRING_KINDS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_string_appearing_three_times() {
        let src = r#"
            fn f() {
                let a = "hello world";
                let b = "hello world";
                let c = "hello world";
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_short_strings() {
        let src = r#"
            fn f() {
                let a = "short";
                let b = "short";
                let c = "short";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_contents_of_a_single_raw_string() {
        // The user's exact FP: a JSON schema in ONE raw string contains
        // dozens of `"type"` / `"object"` quote-wrapped words, but the
        // AST sees the whole body as a single string_literal and
        // counts it once.
        let src = r###"
            fn f() {
                let schema = r#"{
                    "type": "object",
                    "properties": {
                        "a": { "type": "string" },
                        "b": { "type": "string" },
                        "c": { "type": "string" }
                    }
                }"#;
                let _ = schema;
            }
        "###;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_string_appearing_only_in_comments() {
        let src = r#"
            fn f() {
                // the "structured_output" field
                // fall back if "structured_output" is missing
                // always read "structured_output" first
                let field = "structured_output";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_raw_string_duplicated_three_times() {
        // Same raw-string body three times → correctly flagged.
        let src = r###"
            fn f() {
                let a = r#"SHARED_BODY"#;
                let b = r#"SHARED_BODY"#;
                let c = r#"SHARED_BODY"#;
                let _ = (a, b, c);
            }
        "###;
        assert_eq!(run(src).len(), 1);
    }
}
