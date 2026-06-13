//! no-duplicate-string — Rust backend.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

/// Macros whose first string argument is a compile-time format template.
/// `format!`/`panic!`/etc. require a string *literal* — the template
/// cannot be hoisted to a `const &str` and still expand, so a repeated
/// template is not a duplicate worth extracting.
const FORMAT_MACROS: &[&str] = &[
    "format",
    "write",
    "writeln",
    "print",
    "println",
    "eprint",
    "eprintln",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "format_args",
];

/// True when `node` is the format-string argument of a format-like macro
/// (`format!`, `write!`, `panic!`, …): the first `string_literal` directly
/// inside the macro's `token_tree`. Such a literal is a compile-time
/// template and cannot be extracted to a `const`, so it must not count
/// toward the duplicate tally.
pub(super) fn is_format_template_arg(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(token_tree) = node.parent() else {
        return false;
    };
    if token_tree.kind() != "token_tree" {
        return false;
    }
    let Some(macro_invocation) = token_tree.parent() else {
        return false;
    };
    if macro_invocation.kind() != "macro_invocation" {
        return false;
    }
    let Some(macro_name) = macro_invocation.child_by_field_name("macro") else {
        return false;
    };
    let Ok(name) = macro_name.utf8_text(source) else {
        return false;
    };
    let bare = name.rsplit("::").next().unwrap_or(name);
    if !FORMAT_MACROS.contains(&bare) {
        return false;
    }
    // Only the *first* string literal in the token tree is the format
    // template; later string arguments (e.g. `format!("{}", "x")`) are
    // ordinary extractable values.
    let mut cursor = token_tree.walk();
    token_tree
        .named_children(&mut cursor)
        .find(|child| child.kind() == "string_literal")
        .is_some_and(|first| first.id() == node.id())
}

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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
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
    fn skips_strings_in_cfg_test_module() {
        let src = r#"
            #[cfg(test)]
            mod tests {
                fn setup() {
                    let a = "test fixture data";
                    let b = "test fixture data";
                    let c = "test fixture data";
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_strings_in_test_fn() {
        let src = r#"
            #[test]
            fn it_works() {
                let a = "expected value here";
                let b = "expected value here";
                let c = "expected value here";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_strings_in_attributes() {
        let src = r#"
            #[cfg_attr(feature = "postgres_backend", derive(AsExpression))]
            #[cfg_attr(feature = "postgres_backend", diesel(sql_type = Ts))]
            #[cfg_attr(feature = "postgres_backend", derive(FromSqlRow))]
            struct Proxy;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_same_value_across_categorized_arrays() {
        // The issue's FP: keyword lookup tables where the same value
        // belongs to several categorized `const` arrays. Each array is a
        // standalone enumeration, so the repetition is intentional data,
        // not a business constant worth extracting.
        let src = r#"
            const SHORTHAND_PROPERTIES: &[&str] = &["align-content", "flex"];
            const ANIMATABLE_PROPERTIES: &[&str] = &["align-content", "color"];
            const TRANSITION_PROPERTIES: &[&str] = &["align-content", "margin"];
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_value_duplicated_in_expressions() {
        // A genuine duplicate: the same literal hard-coded across plain
        // expressions (not array data) is still extractable.
        let src = r#"
            fn f() {
                let a = greet("welcome-banner");
                let b = greet("welcome-banner");
                let c = greet("welcome-banner");
                let _ = (a, b, c);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_repeated_format_template() {
        // The issue's FP: the same format template repeated across
        // `format!`/`write!`/`panic!`. It cannot be hoisted to a `const`
        // because format macros require a literal, so it must not be
        // flagged.
        let src = r#"
            fn f(id: u8, w: &mut String) {
                let a = format!("unimplemented {id:?}");
                let b = format!("unimplemented {id:?}");
                let _ = write!(w, "unimplemented {id:?}");
                let c = format!("unimplemented {id:?}");
                panic!("unimplemented {id:?}");
                let _ = (a, b, c);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_template_string_arg_in_format_macro() {
        // Only the format-string (first literal) is exempt. A plain
        // value argument repeated across format macros is still an
        // extractable duplicate.
        let src = r#"
            fn f() {
                let a = format!("{}", "welcome-banner-label");
                let b = format!("{}", "welcome-banner-label");
                let c = format!("{}", "welcome-banner-label");
                let _ = (a, b, c);
            }
        "#;
        assert_eq!(run(src).len(), 1);
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
