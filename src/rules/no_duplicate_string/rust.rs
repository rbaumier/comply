//! no-duplicate-string — Rust backend.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::path_utils::is_cargo_example_path;
use crate::rules::sql_helpers::RUST_STRING_KINDS;

/// Macros whose first string argument is a message written inline at each
/// site: `format!`/`write!`/`print!` reject a non-literal template outright,
/// and `panic!`/`unreachable!`/`todo!`/`assert!` say what went wrong *here*.
/// A repeated message from one of these is not a duplicate worth extracting.
///
/// The list is load-bearing only for the placeholder-free case; a literal
/// carrying a `{…}` placeholder is a template whatever macro encloses it, and
/// `has_format_placeholder` recognizes it without consulting a name.
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

/// True when `node` is the message argument of a macro invocation that cannot
/// be hoisted: the first `string_literal` directly inside the macro's
/// `token_tree`, and either
///
/// - the literal carries a `{…}` format placeholder, or
/// - the macro is one of `FORMAT_MACROS`.
///
/// A placeholder-bearing literal is a template, not a value: every macro that
/// routes its first argument through `format_args!` — `format!`, `write!`,
/// `anyhow::bail!`, `tracing::warn!` and the rest — needs a literal in that
/// position, and hoisting the template to a `const` either fails to compile or
/// expands to the placeholder's own text with the named argument left unread.
/// That is a property of the literal, so it is decided from the literal's
/// `content` rather than from the macro's name.
///
/// Only the first literal is the template; later string arguments (e.g.
/// `format!("{}", "x")`) are ordinary extractable values and stay counted.
pub(super) fn is_format_template_arg(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    content: &str,
) -> bool {
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
    let mut cursor = token_tree.walk();
    let is_first_literal = token_tree
        .named_children(&mut cursor)
        .find(|child| child.kind() == "string_literal")
        .is_some_and(|first| first.id() == node.id());
    if !is_first_literal {
        return false;
    }
    if has_format_placeholder(content) {
        return true;
    }
    let Some(macro_name) = macro_invocation.child_by_field_name("macro") else {
        return false;
    };
    let Ok(name) = macro_name.utf8_text(source) else {
        return false;
    };
    let bare = name.rsplit("::").next().unwrap_or(name);
    FORMAT_MACROS.contains(&bare)
}

/// True when `content` carries at least one `format_args!` placeholder — an
/// unescaped `{…}` run whose body is a placeholder body (`{}`, `{0}`, `{unk}`,
/// `{x:?}`, `{n:>8}`). `{{` and `}}` are escaped braces and do not open one, so
/// `"{{literal braces}}"` is an ordinary hoistable value.
fn has_format_placeholder(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }
        if bytes.get(index + 1) == Some(&b'{') {
            index += 2;
            continue;
        }
        let body_start = index + 1;
        let body_end = bytes[body_start..]
            .iter()
            .position(|byte| matches!(byte, b'{' | b'}'))
            .map(|offset| body_start + offset);
        if let Some(end) = body_end
            && bytes[end] == b'}'
            && is_placeholder_body(&content[body_start..end])
        {
            return true;
        }
        index = body_start;
    }
    false
}

/// True when `body` — the text between a candidate placeholder's braces — is a
/// format-argument reference: an optional argument (empty for the next
/// positional one, an index like `0`, or an identifier like `unk`) followed by
/// an optional `:`-introduced format spec. Rules out brace runs that are plain
/// text, such as the `{ "type": "object" }` of an inline JSON snippet.
fn is_placeholder_body(body: &str) -> bool {
    let argument = body.split(':').next().unwrap_or(body);
    argument.is_empty()
        || argument.bytes().all(|byte| byte.is_ascii_digit())
        || (argument.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && argument.chars().all(|c| c.is_alphanumeric() || c == '_'))
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

/// True when `node` is a string literal lexically inside a `#[ ... ]`
/// attribute that itself sits inside a function-like macro
/// invocation's token tree — e.g. a struct field's
/// `#[serde(skip_serializing_if = "Option::is_none")]` passed to a
/// struct-generating macro like `config_struct!( ... )`. tree-sitter tokenizes
/// macro-invocation arguments as raw `token_tree`s, so the attribute is not
/// parsed as an `attribute_item` and the string has no `attribute_item`
/// ancestor; this is the macro-invocation analogue of the `attribute_item` arm
/// in `should_ignore_string_node`. The shape is recognized structurally by
/// ascending `token_tree` wrappers and matching an ancestor that is a
/// `[ ... ]` bracket group whose immediately-preceding sibling token is `#`.
/// Such a value is a compiler-mandated inline attribute literal that cannot be
/// hoisted to a `const`, so it must not count toward the duplicate tally. A
/// `[ ... ]` group *not* preceded by `#` (an array literal in macro-argument
/// position) is left subject to the count.
pub(super) fn is_macro_attribute_arg(node: tree_sitter::Node<'_>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() != "token_tree" {
            return false;
        }
        if is_hash_prefixed_bracket_group(parent) {
            return true;
        }
        current = parent;
    }
    false
}

/// True when `token_tree` is a `[ ... ]` bracket group immediately preceded by
/// a `#` marker token — the `#[ ... ]` attribute shape as it
/// appears among raw macro tokens. The opening `[` is the group's first child;
/// the `#` is an anonymous sibling token directly before the group.
fn is_hash_prefixed_bracket_group(token_tree: tree_sitter::Node<'_>) -> bool {
    let opens_with_bracket = token_tree
        .child(0)
        .is_some_and(|delimiter| delimiter.kind() == "[");
    opens_with_bracket
        && token_tree
            .prev_sibling()
            .is_some_and(|marker| marker.kind() == "#")
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

/// True when `node` is a cell of a lookup table — a literal whose meaning is
/// the row it sits in rather than a value some call site could name. Such a
/// literal recurs because the data recurs, has no call site to be extracted
/// away from, and hoisting each cell replaces a scannable table with
/// identifier soup. Two spellings of the same table:
///
/// - the keys of a keyword-dispatch `match` (`is_match_arm_pattern`);
/// - the cells of a `const`/`static` array of rows (`is_const_table_element`).
pub(super) fn is_lookup_table_cell(node: tree_sitter::Node<'_>) -> bool {
    is_match_arm_pattern(node) || is_const_table_element(node)
}

/// True when `node` is a string literal in `match`-arm **pattern** position
/// (`match name { "is_datetime" => …, "a" | "b" => … }`), reached through a
/// `match_pattern`/`or_pattern` wrapper. The same canonical key validly recurs
/// across the parallel `match` blocks of a builtin dispatcher (function-call vs
/// method-call syntax, sync vs async). Only the pattern side is exempt: a
/// literal in the arm's value expression or guard is not wrapped in a
/// `match_pattern`/`or_pattern`, so it stays in ordinary expression position
/// and still counts.
fn is_match_arm_pattern(node: tree_sitter::Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| matches!(parent.kind(), "match_pattern" | "or_pattern"))
        && crate::rules::rust_helpers::match_arm_of_pattern(node).is_some()
}

/// True when `node` is an element of an array literal that initialises a
/// `const`/`static` item, reached through any nesting of tuple, array and
/// reference wrappers (`static TABLE: &[(u16, &str, &str)] = &[(0x0436,
/// "Afrikaans", "South Africa"), …]`). The container must be a `const`/`static`
/// item: an array built at runtime inside a function body may well be
/// copy-paste, so its elements stay counted.
fn is_const_table_element(node: tree_sitter::Node<'_>) -> bool {
    let mut current = node;
    let mut inside_array = false;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "array_expression" => inside_array = true,
            "tuple_expression" | "reference_expression" => {}
            "const_item" | "static_item" => return inside_array,
            _ => return false,
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
    fn does_not_flag_serde_skip_serializing_if_in_config_macro() {
        // The issue's FP (moonrepo/moon): structs generated via a
        // `config_struct!( ... )` macro carry `#[serde(...)]` field attributes.
        // tree-sitter tokenizes the macro arguments as a raw `token_tree`, so
        // `"Option::is_none"` has no `attribute_item` ancestor — but it is a
        // serde attribute argument that must be an inline literal and cannot be
        // hoisted to a `const`, exactly like a real `#[serde(...)]` attribute.
        let src = r#"
            config_struct!(
                pub struct S {
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    a: Option<i32>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    b: Option<i32>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    c: Option<i32>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    d: Option<i32>
                }
            );
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_array_literal_in_macro_arg() {
        // Precision for the `#` marker: only a `[ ... ]` group preceded by `#`
        // (the attribute shape) is exempt. A plain array literal in
        // macro-argument position is a `[ ... ]` group with no `#` marker, so
        // its repeated elements stay extractable duplicates.
        let src = r#"
            my_macro!([
                "duplicated-value",
                "duplicated-value",
                "duplicated-value",
                "duplicated-value"
            ]);
        "#;
        assert_eq!(run(src).len(), 2);
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

    #[test]
    fn does_not_flag_string_literals_in_match_arm_patterns() {
        // The issue's FP (surrealdb): a builtin-function dispatcher with
        // parallel `match name { … }` blocks (function-call vs method-call
        // syntax), each repeating the same canonical function-name keys in
        // arm-pattern position. These are categorized keyword-table keys, not
        // extractable business constants.
        let src = r#"
            fn dispatch(name: &str) -> u8 {
                match name {
                    "is_datetime" => 1,
                    "is_decimal" => 2,
                    _ => 0,
                }
            }
            fn dispatch_method(name: &str) -> u8 {
                match name {
                    "is_datetime" => 3,
                    "is_decimal" => 4,
                    _ => 0,
                }
            }
            fn dispatch_async(name: &str) -> u8 {
                match name {
                    "is_datetime" => 5,
                    _ => 0,
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_string_literals_in_or_pattern_match_arms() {
        // Keys grouped with `"a" | "b" =>` are still match-arm patterns; the
        // `or_pattern` wrapper must not defeat the skip.
        let src = r#"
            fn categorize(name: &str) -> u8 {
                match name {
                    "is_datetime" | "is_decimal" => 1,
                    _ => 0,
                }
            }
            fn categorize_again(name: &str) -> u8 {
                match name {
                    "is_datetime" | "is_decimal" => 2,
                    _ => 0,
                }
            }
            fn categorize_third(name: &str) -> u8 {
                match name {
                    "is_datetime" => 3,
                    _ => 0,
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_repeated_match_arm_value_string() {
        // A literal in the arm's *value* expression is ordinary expression
        // position (direct child of `match_arm`, not a pattern wrapper) and
        // stays an extractable duplicate.
        let src = r#"
            fn label(x: u8) -> &'static str {
                match x {
                    0 => "shared label text",
                    1 => "shared label text",
                    _ => "shared label text",
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_placeholder_template_in_error_macro() {
        // The issue's FP (ripgrep): the same `{unk}`-bearing template passed to
        // `anyhow::bail!` from four independent flag parsers. `bail!` routes its
        // first argument through `format_args!` exactly as `panic!` does, so
        // hoisting the template to a `const` expands to the literal text
        // `choice '{unk}' is unrecognized` with `unk` left unread. The qualified
        // and unqualified spellings get the same verdict.
        let src = r#"
            fn parse_a(v: &str) -> anyhow::Result<u8> {
                match v {
                    "path" => Ok(1),
                    unk => anyhow::bail!("choice '{unk}' is unrecognized here"),
                }
            }
            fn parse_b(v: &str) -> anyhow::Result<u8> {
                match v {
                    "none" => Ok(2),
                    unk => bail!("choice '{unk}' is unrecognized here"),
                }
            }
            fn parse_c(v: &str) -> anyhow::Result<u8> {
                match v {
                    "line" => Ok(3),
                    unk => anyhow::bail!("choice '{unk}' is unrecognized here"),
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_placeholder_template_in_logging_macro() {
        // Same property, a different macro family: a `tracing`/`log` template
        // needs a literal in the same position, so it is not extractable
        // either.
        let src = r#"
            fn a(attempt: u32, max: u32) { tracing::warn!("retrying {attempt} of {max} now"); }
            fn b(attempt: u32, max: u32) { tracing::warn!("retrying {attempt} of {max} now"); }
            fn c(attempt: u32, max: u32) { log::info!("retrying {attempt} of {max} now"); }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_escaped_braces_literal_in_ordinary_macro() {
        // Precision: `{{` / `}}` are escaped braces, not a placeholder, so the
        // literal is an ordinary hoistable value in a macro with no
        // format-template contract.
        let src = r#"
            fn f() {
                log_event!("{{literal braces}} in the label");
                log_event!("{{literal braces}} in the label");
                log_event!("{{literal braces}} in the label");
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_brace_run_that_is_not_a_placeholder_body() {
        // Precision: the exemption is a format placeholder, not "contains
        // braces". An inline JSON payload has a `{ … }` run whose body is not
        // an argument reference, so it stays an extractable duplicate.
        let src = r#"
            fn f() {
                log_event!("{ \"type\": \"object\" } payload");
                log_event!("{ \"type\": \"object\" } payload");
                log_event!("{ \"type\": \"object\" } payload");
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_placeholder_literal_outside_macro_position() {
        // Precision: a template is unhoistable only where a macro demands a
        // literal. The same text bound to a variable is an ordinary value.
        let src = r#"
            fn f() {
                let a = "choice '{unk}' is unrecognized here";
                let b = "choice '{unk}' is unrecognized here";
                let c = "choice '{unk}' is unrecognized here";
                let _ = (a, b, c);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_second_string_arg_carrying_braces() {
        // Precision: only the *first* literal is the template. A later argument
        // is an ordinary value even when it happens to carry a placeholder.
        let src = r#"
            fn f() {
                let a = format!("{}", "value {x} in the label");
                let b = format!("{}", "value {x} in the label");
                let c = format!("{}", "value {x} in the label");
                let _ = (a, b, c);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_cells_of_a_static_lookup_table() {
        // The issue's FP (ttf-parser): a static table mapping Windows LCIDs to
        // language/region names. `"South Africa"` recurs because five languages
        // in the table are spoken there — the repetition *is* the mapping, and
        // each row owns its own datum.
        let src = r#"
            #[rustfmt::skip]
            static TABLE: &[(u16, &str, &str)] = &[
                (0x0436, "Afrikaans",  "South Africa"),
                (0x0409, "English",    "South Africa"),
                (0x0432, "Setswana",   "South Africa"),
                (0x0434, "Xhosa",      "South Africa"),
                (0x0435, "Zulu",       "South Africa"),
                (0x0C07, "German",     "Switzerland"),
                (0x100C, "French",     "Switzerland"),
                (0x0810, "Italian",    "Switzerland"),
                (0x141A, "Bosnian",    "Bosnia and Herzegovina"),
                (0x101A, "Croatian",   "Bosnia and Herzegovina"),
                (0x201A, "Serbian",    "Bosnia and Herzegovina"),
            ];
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_cells_of_a_const_array_table() {
        // The `const` spelling of the same table, without the `&` reference
        // wrapper, and with a nested array row.
        let src = r#"
            const TABLE: [(&str, &str); 3] = [
                ("af", "South Africa"),
                ("en", "South Africa"),
                ("zu", "South Africa"),
            ];
            const GROUPS: &[&[&str]] = &[
                &["Afrikaans", "South Africa"],
                &["English", "South Africa"],
            ];
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_repeated_cells_of_a_runtime_array() {
        // Precision for the `const`/`static` requirement: the same rows built
        // inside a function body may well be copy-paste, so the cells stay
        // counted.
        let src = r#"
            fn build() -> [(&'static str, &'static str); 3] {
                [
                    ("af", "South Africa"),
                    ("en", "South Africa"),
                    ("zu", "South Africa"),
                ]
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn counts_only_the_non_table_occurrences_of_a_table_value() {
        // Table cells are not counted at all, so a value that also appears in
        // ordinary expression position is tallied on those occurrences alone.
        let src = r#"
            static TABLE: &[(u16, &str)] = &[
                (0x0436, "South Africa"),
                (0x0409, "South Africa"),
            ];
            fn a() -> &'static str { label("South Africa") }
            fn b() -> &'static str { label("South Africa") }
            fn c() -> &'static str { label("South Africa") }
        "#;
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("appears 3 times"));
    }

    #[test]
    fn still_flags_repeated_expression_position_string() {
        // Repeated `.expect("valid datetime")` messages are expression
        // position, not match-arm patterns, so they remain flagged.
        let src = r#"
            fn f(a: Option<u8>, b: Option<u8>, c: Option<u8>) {
                let x = a.expect("valid datetime");
                let y = b.expect("valid datetime");
                let z = c.expect("valid datetime");
                let _ = (x, y, z);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
