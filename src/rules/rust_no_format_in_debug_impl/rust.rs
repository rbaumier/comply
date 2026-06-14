//! rust-no-format-in-debug-impl backend.
//!
//! For every `impl_item` whose trait is `Debug`/`fmt::Debug`/
//! `std::fmt::Debug`, find the `fn fmt(...)` method and scan its
//! body for `format!` macro invocations. Each one is a wasted
//! allocation that should be a `write!`.
//!
//! Two `format!` shapes are allowed:
//!
//! - One that supplies the *name* argument of a `debug_struct(...)` /
//!   `debug_tuple(...)` call — those methods require a runtime `&str`
//!   name, so a type name embedding a const generic (`format!("Grid<{N}>")`)
//!   can only be built that way.
//! - One whose arguments contain a truncation signal: a `.len()` /
//!   `.count()` / `.capacity()` method call or an index/slice expression
//!   (`v[0]`). Those mark an intentionally summarized rendering of large
//!   data (embedding vectors, result sets) where printing the full
//!   structure would dump unbounded output.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["impl_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(trait_node) = node.child_by_field_name("trait") else {
            return;
        };
        let Ok(trait_text) = trait_node.utf8_text(source_bytes) else {
            return;
        };
        if !is_debug_trait(trait_text) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        collect_format_macros_in(body, source_bytes, ctx, diagnostics);
    }
}

fn is_debug_trait(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "Debug"
        || trimmed == "fmt::Debug"
        || trimmed == "std::fmt::Debug"
        || trimmed == "core::fmt::Debug"
}

/// True when `format_node` (a `format!` invocation) supplies the *name*
/// argument of a `debug_struct(...)` / `debug_tuple(...)` call on the
/// formatter — e.g. `f.debug_struct(&format!("Grid<{N}>"))`. Those builder
/// methods take a runtime `&str` name, so when the type name embeds a const
/// generic the only way to spell it is `format!`. The macro may reach the
/// argument list directly, behind a `&`, or via `format!(...).as_str()`.
fn is_debug_builder_name_arg(format_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = format_node;
    // Walk up through wrappers that keep the resulting string flowing to the
    // same argument slot: `&format!(...)` (reference_expression) and adapters
    // like `format!(...).as_str()` (field_expression value → its call). Stop at
    // the enclosing `arguments` node and check whether it is the name argument
    // of a `debug_struct`/`debug_tuple` call.
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "reference_expression" => current = parent,
            "field_expression" => {
                // Adapter call receiver (`format!(...).as_str`): keep climbing.
                // A `.field` access where the macro is the field name can't happen.
                if parent.child_by_field_name("value") == Some(current) {
                    current = parent;
                } else {
                    return false;
                }
            }
            "call_expression" => {
                // `current` is the function of an adapter call (`.as_str()`):
                // climb past it. Otherwise it isn't in name-argument position.
                if parent.child_by_field_name("function") == Some(current) {
                    current = parent;
                } else {
                    return false;
                }
            }
            "arguments" => {
                return is_first_named_arg(parent, current)
                    && parent
                        .parent()
                        .is_some_and(|call| is_debug_builder_call(call, source));
            }
            _ => return false,
        }
    }
    false
}

/// The function of `call` is a `field_expression` whose field is
/// `debug_struct` or `debug_tuple`.
fn is_debug_builder_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let Some(field) = function.child_by_field_name("field") else {
        return false;
    };
    matches!(
        field.utf8_text(source),
        Ok("debug_struct") | Ok("debug_tuple")
    )
}

/// `child` is the first positional argument inside an `arguments` node.
fn is_first_named_arg(arguments: tree_sitter::Node, child: tree_sitter::Node) -> bool {
    let mut cursor = arguments.walk();
    arguments
        .named_children(&mut cursor)
        .next()
        .is_some_and(|first| first == child)
}

/// True when the `format!` invocation's arguments carry a truncation
/// signal — a `.len()` / `.count()` / `.capacity()` method call or an
/// index/slice expression (`v[0]`). Such a `format!` renders an
/// intentionally summarized view of large data, not a wasteful
/// re-allocation of an already-`Debug`-able value, so it is exempt.
///
/// Macro arguments are not parsed into a structured AST: they live in a
/// `token_tree` where `hits.len()` flattens to `(identifier "hits") .
/// (identifier "len") (token_tree "()")` and `v[0]` to `(identifier "v")
/// (token_tree "[0]")`. We walk that token stream looking for either
/// shape.
fn format_args_contain_truncation_signal(
    format_node: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let mut cursor = format_node.walk();
    format_node
        .children(&mut cursor)
        .find(|child| child.kind() == "token_tree")
        .is_some_and(|token_tree| token_tree_has_truncation_signal(token_tree, source))
}

const TRUNCATION_METHODS: &[&str] = &["len", "count", "capacity"];

/// Recursively scans a `token_tree` for a truncation signal.
fn token_tree_has_truncation_signal(token_tree: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = token_tree.walk();
    let children: Vec<tree_sitter::Node> = token_tree.children(&mut cursor).collect();
    for (index, child) in children.iter().enumerate() {
        match child.kind() {
            // A nested `token_tree` is either an index/slice bracket
            // (`[0]` — a truncation signal on its own) or grouping
            // parens we must recurse into.
            "token_tree" => {
                if starts_with_byte(*child, source, b'[') {
                    return true;
                }
                if token_tree_has_truncation_signal(*child, source) {
                    return true;
                }
            }
            // `.len()` / `.count()` / `.capacity()`: a method-name
            // identifier preceded by `.` and followed by a `(` group.
            "identifier"
                if child
                    .utf8_text(source)
                    .is_ok_and(|text| TRUNCATION_METHODS.contains(&text))
                    && index
                        .checked_sub(1)
                        .is_some_and(|prev| is_dot(children.get(prev), source))
                    && children
                        .get(index + 1)
                        .is_some_and(|next| starts_with_byte(*next, source, b'(')) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Whether `node`'s source text begins with `byte`.
fn starts_with_byte(node: tree_sitter::Node, source: &[u8], byte: u8) -> bool {
    source.get(node.start_byte()) == Some(&byte)
}

/// Whether `node` is the anonymous `.` token.
fn is_dot(node: Option<&tree_sitter::Node>, source: &[u8]) -> bool {
    node.is_some_and(|n| n.utf8_text(source) == Ok("."))
}

fn collect_format_macros_in(
    body: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "macro_invocation"
            && let Some(macro_node) = node.child_by_field_name("macro")
            && let Ok(name) = macro_node.utf8_text(source)
            && name == "format"
            && !is_debug_builder_name_arg(node, source)
            && !format_args_contain_truncation_signal(node, source)
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-format-in-debug-impl".into(),
                message: "`format!` inside `Debug::fmt` allocates a \
                          throwaway `String`. Use `write!(f, \"...\", \
                          ...)` to stream directly into the formatter."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_format_in_debug_impl() {
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(&format!("Foo({})", self.x))
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_write_in_debug_impl() {
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "Foo({})", self.x)
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_format_in_other_impls() {
        let source = r#"impl Display for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(&format!("{}", self.x))
            }
        }"#;
        // Display is fair game — it's not on the same hot path as Debug.
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_as_debug_struct_name_arg() {
        // `debug_struct` takes a `&str` name; with a const generic the name
        // can only be built at runtime via `format!`. See #1326.
        let source = r#"impl<const N: usize> fmt::Debug for Grid<N> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_struct(&format!("Grid<{N}>")).finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_as_debug_tuple_name_arg() {
        let source = r#"impl<const N: usize> fmt::Debug for Grid<N> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_tuple(format!("Grid<{N}>").as_str()).finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn issue_1326_exempts_only_the_name_arg() {
        // sled src/db.rs: the name `format!` is exempt, the `.field(...)`
        // value `format!` is still the genuine waste the rule targets.
        let source = r#"impl<const LEAF_FANOUT: usize> fmt::Debug for Db<LEAF_FANOUT> {
            fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut debug_struct = w.debug_struct(&format!("Db<{}>", LEAF_FANOUT));
                debug_struct
                    .field("data", &format!("{:?}", self.iter().collect::<Vec<_>>()))
                    .finish()
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_format_for_field_value_even_with_debug_struct() {
        // The name arg is a literal here; the `format!` builds a field value,
        // which is the genuine waste the rule targets.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_struct("Foo")
                    .field("x", &format!("{}-{}", self.a, self.b))
                    .finish()
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_format_summarizing_len() {
        // meilisearch: `format!` produces a short summary of a large result
        // set; the `.len()` is the truncation signal. See #1333.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                debug.field("hits", &format!("[{} hits returned]", hits.len()));
                debug.finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_summarizing_index_and_len() {
        // meilisearch: a truncated embedding vector render mixing indexing
        // (`v[0]`) and `.len()`. See #1333.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                debug.field(
                    "vector",
                    &format!("[{}, {}, {}, ... {} dimensions]", v[0], v[1], v[2], v.len()),
                );
                debug.finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_summarizing_count() {
        // `.count()` is an equally valid truncation signal. See #1333.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                debug.field("items", &format!("[{} items]", self.iter().count()));
                debug.finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_field_value_format_without_truncation_signal() {
        // No `.len()`/`.count()`/`.capacity()` and no indexing: a plain
        // field-value `format!` is still the waste the rule targets. This is
        // #1326's negative-space guard — it must stay flagged.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                debug.field("name", &format!("{}", self.raw));
                debug.finish()
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }
}
