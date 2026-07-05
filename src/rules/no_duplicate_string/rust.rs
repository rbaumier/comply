//! no-duplicate-string — Rust backend.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::path_utils::is_cargo_example_path;
use crate::rules::sql_helpers::RUST_STRING_KINDS;

/// Macros whose first string argument is a compile-time format template or
/// panic/diagnostic message. `format!`/`panic!`/`unreachable!`/etc. require a
/// string *literal* — the template cannot be hoisted to a `const &str` and
/// still expand, and panic-family messages are idiomatically inlined at each
/// site, so a repeated template/message is not a duplicate worth extracting.
const FORMAT_MACROS: &[&str] = &[
    "format",
    "write",
    "writeln",
    "print",
    "println",
    "eprint",
    "eprintln",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
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

/// Macros whose string arguments are compile-time `cfg` predicate tokens
/// (`cfg!(feature = "x")`, `cfg_attr!(...)`). Rust requires these values to be
/// inline literal tokens — `cfg!(feature = FOO)` does not compile — so a
/// repeated cfg feature name cannot be hoisted to a `const`. This is the
/// macro-invocation form of the `#[cfg(...)]` attribute already skipped in
/// `should_ignore_string_node`.
const CFG_MACROS: &[&str] = &["cfg", "cfg_attr"];

/// True when `node` is a string argument of a `cfg!(...)` / `cfg_attr!(...)`
/// macro invocation. Ascends through the (possibly nested, e.g.
/// `cfg!(all(feature = "x"))`) `token_tree` wrappers to the enclosing
/// `macro_invocation` and matches its bare macro name. Such a literal is a
/// compile-time cfg predicate token that cannot be extracted to a `const`, so
/// it must not count toward the duplicate tally.
pub(super) fn is_cfg_macro_arg(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "token_tree" => current = parent,
            "macro_invocation" => {
                let Some(macro_name) = parent.child_by_field_name("macro") else {
                    return false;
                };
                let Ok(name) = macro_name.utf8_text(source) else {
                    return false;
                };
                let bare = name.rsplit("::").next().unwrap_or(name);
                return CFG_MACROS.contains(&bare);
            }
            _ => return false,
        }
    }
    false
}

/// True when `node` sits inside a `macro_rules!` definition body. The arm
/// bodies of a `macro_definition` are raw token trees: a string literal there
/// is template code spliced into every expansion (typically a `concat!`
/// fragment or an attribute value), not an expression that can be hoisted to a
/// `const`. Such literals must not count toward the duplicate tally.
pub(super) fn is_in_macro_rules_body(node: tree_sitter::Node<'_>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "macro_definition" {
            return true;
        }
        current = parent;
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        // Cargo `examples/` files are illustrative example targets, not
        // production code. Intentional string repetition there (e.g. a source
        // identifier threaded through a builder chain) is part of the demo and
        // need not be hoisted to a `const`, so skip them like test code.
        if is_cargo_example_path(ctx.path) {
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

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
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
    fn skips_duplicate_string_in_cargo_examples_dir() {
        // The issue's FP: a source-file identifier repeated across a
        // builder-pattern chain in `examples/stresstest.rs`. Example targets
        // are illustrative code, so the repetition need not be hoisted.
        let src = r#"
            fn main() {
                let a = Label::new(("stresstest.tao", 1..2));
                let b = Label::new(("stresstest.tao", 2..3));
                let c = Label::new(("stresstest.tao", 3..4));
                let _ = (a, b, c);
            }
        "#;
        assert!(run_at(src, "examples/stresstest.rs").is_empty());
    }

    #[test]
    fn still_flags_duplicate_string_in_production_src() {
        // The same duplicated source identifier in production `src/` is still
        // an extractable constant and stays flagged.
        let src = r#"
            fn main() {
                let a = Label::new(("stresstest.tao", 1..2));
                let b = Label::new(("stresstest.tao", 2..3));
                let c = Label::new(("stresstest.tao", 3..4));
                let _ = (a, b, c);
            }
        "#;
        assert_eq!(run_at(src, "src/render.rs").len(), 1);
    }

    #[test]
    fn does_not_flag_unreachable_sentinel_across_stub_methods() {
        // The issue's FP (async-std): a doc-only struct compiled solely for
        // rustdoc, every method stubbed with the same `unreachable!` message.
        // The message is a panic-family sentinel that is idiomatically
        // inlined, not extracted to a `const`.
        let src = r#"
            impl Metadata {
                pub fn file_type(&self) -> FileType {
                    unreachable!("this impl only appears in the rendered docs")
                }
                pub fn is_dir(&self) -> bool {
                    unreachable!("this impl only appears in the rendered docs")
                }
                pub fn is_file(&self) -> bool {
                    unreachable!("this impl only appears in the rendered docs")
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_todo_or_unimplemented_message() {
        // `todo!` / `unimplemented!` messages are panic-family literals,
        // inlined at each call site rather than hoisted to a constant.
        let src = r#"
            fn a() { todo!("wire up the storage backend later") }
            fn b() { todo!("wire up the storage backend later") }
            fn c() { unimplemented!("wire up the storage backend later") }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_repeated_must_use_attribute_message() {
        // The issue's FP (futures-lite): the same `#[must_use = "..."]`
        // message on many combinator structs. The attribute argument must be
        // an inline string literal on each type and cannot be hoisted to a
        // `const`, so it must not be flagged.
        let src = r#"
            #[must_use = "streams do nothing unless polled"]
            pub struct TryUnfold;
            #[must_use = "streams do nothing unless polled"]
            pub struct Chain;
            #[must_use = "streams do nothing unless polled"]
            pub struct Zip;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_repeated_deprecated_note_attribute() {
        // `#[deprecated(note = "...")]` repeated across items — the note is an
        // attribute argument and cannot be extracted to a constant.
        let src = r#"
            #[deprecated(note = "use the new builder API instead")]
            pub fn old_one() {}
            #[deprecated(note = "use the new builder API instead")]
            pub fn old_two() {}
            #[deprecated(note = "use the new builder API instead")]
            pub fn old_three() {}
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_concat_fragment_inside_macro_rules_body() {
        // The issue's FP (ratatui): a string fragment is an argument to
        // `concat!()` inside `macro_rules!` arm bodies, spliced together with
        // a `stringify!($metavar)`. The arm body is a raw token tree —
        // `concat!` accepts only literals and the `#[must_use = ...]` value
        // must be an inline literal — so the fragment cannot be hoisted to a
        // `const`. The same suffix appears across both arms of two macros.
        let src = r##"
            macro_rules! color {
                (pub const $variant:expr, $color:ident(), $on_color:ident() -> $ty:ty) => {
                    #[must_use = concat!("`", stringify!($color), "` returns the modified style without modifying the original")]
                    pub const fn $color(self) -> $ty {
                        self.fg($variant)
                    }
                    #[must_use = concat!("`", stringify!($on_color), "` returns the modified style without modifying the original")]
                    pub const fn $on_color(self) -> $ty {
                        self.bg($variant)
                    }
                };
            }
            macro_rules! modifier {
                (pub const $variant:expr, $modifier:ident(), $not_modifier:ident() -> $ty:ty) => {
                    #[must_use = concat!("`", stringify!($modifier), "` returns the modified style without modifying the original")]
                    pub const fn $modifier(self) -> $ty {
                        self.add_modifier($variant)
                    }
                };
            }
        "##;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_feature_strings_in_macro_rules_template() {
        // The issue's FP (dtolnay/syn): `#[cfg(feature = "...")]` /
        // `#[cfg_attr(docsrs, doc(cfg(feature = "...")))]` repeated inside a
        // `macro_rules!` arm. Tree-sitter parses the attribute syntax as raw
        // tokens in a `token_tree`, so the literal has no `attribute_item`
        // ancestor — but the arm is a single token template and the cfg
        // strings are compiler-mandated inline literals that cannot be
        // hoisted to a `const`.
        let src = r#"
            macro_rules! define_punctuation_structs {
                ($name:ident) => {
                    #[cfg(feature = "clone-impls")]
                    #[cfg_attr(docsrs, doc(cfg(feature = "clone-impls")))]
                    impl Copy for $name {}

                    #[cfg(feature = "clone-impls")]
                    #[cfg_attr(docsrs, doc(cfg(feature = "clone-impls")))]
                    impl Clone for $name {
                        fn clone(&self) -> Self {
                            *self
                        }
                    }
                };
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_cfg_feature_flag_strings() {
        // The issue's FP (swc): `cfg!(feature = "typescript")` used as a
        // compile-time boolean across many parser methods. `cfg!` requires a
        // literal token — `cfg!(feature = TS)` does not compile — so the
        // feature name cannot be hoisted to a `const`.
        let src = r#"
            fn a() -> bool { if !cfg!(feature = "typescript") { return false; } true }
            fn b() -> bool { if !cfg!(feature = "typescript") { return false; } true }
            fn c() -> bool { if !cfg!(feature = "typescript") { return false; } true }
            fn d() -> bool { if !cfg!(feature = "typescript") { return false; } true }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_cfg_attr_macro_arg() {
        // The `cfg_attr!(...)` macro-invocation form: its cfg predicate string
        // is equally a compiler-mandated literal token, not extractable.
        let src = r#"
            fn a() { let _ = cfg_attr!(feature = "postgres_backend", derive(X)); }
            fn b() { let _ = cfg_attr!(feature = "postgres_backend", derive(X)); }
            fn c() { let _ = cfg_attr!(feature = "postgres_backend", derive(X)); }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_nested_cfg_predicate() {
        // `cfg!(all(...))` / `cfg!(not(...))` nest the feature string in an
        // inner `token_tree`; it is still a cfg predicate token.
        let src = r#"
            fn a() -> bool { cfg!(all(feature = "async-runtime", unix)) }
            fn b() -> bool { cfg!(all(feature = "async-runtime", unix)) }
            fn c() -> bool { cfg!(not(feature = "async-runtime")) }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_duplicate_string_in_non_cfg_macro() {
        // Precision: only `cfg!`/`cfg_attr!` args are exempt. A genuine
        // duplicate string argument to an ordinary macro is still extractable.
        let src = r#"
            fn f() {
                log_event!("welcome-banner-label");
                log_event!("welcome-banner-label");
                log_event!("welcome-banner-label");
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
