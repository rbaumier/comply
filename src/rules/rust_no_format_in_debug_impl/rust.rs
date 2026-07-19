//! rust-no-format-in-debug-impl backend.
//!
//! For every `impl_item` whose trait is `Debug`/`fmt::Debug`/
//! `std::fmt::Debug`, find the `fn fmt(...)` method and scan its
//! body for `format!` macro invocations. Each one is a wasted
//! allocation that should be a `write!`.
//!
//! Four `format!` shapes are allowed:
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
//! - One that produces an owned `String` *value* rather than a write sink:
//!   the initializer of a `let` binding (`let s = format!(...)`) or the
//!   return value of a closure (`unwrap_or_else(|e| format!(...))`,
//!   `.map(|i| { ...; format!(...) })`). There is no formatter in scope to
//!   `write!` to — the closure or binding must yield an owned `String`.
//! - One supplying the *value* argument of a debug-builder field method —
//!   `.field(name, &format!(...))` / `.entry(...)` / `.key(...)` / `.value(...)`
//!   whose receiver roots at a `debug_struct`/`debug_tuple`/`debug_list`/
//!   `debug_set`/`debug_map` call — either inline in the method-call chain, or,
//!   when the receiver is a bare local identifier, through its `let` binding's
//!   initializer (`let mut debug = f.debug_struct(..); debug.field(..)`). Those
//!   methods take a `&dyn Debug`, so a combined render of several values can
//!   only be passed via `format!`; `write!`-ing into the formatter would bypass
//!   the builder and produce malformed output.
//!
//! Nested helper functions (`fn helper() { ... }` declared inside the `fmt`
//! method body) are skipped entirely: they open a new scope where the outer
//! `fmt`'s formatter is not visible, so a `format!` there returns an owned
//! `String` that the helper cannot `write!` anywhere. Only `format!` in the
//! impl's own method bodies is scanned.

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

/// Debug-builder field methods that take a `&dyn Debug` value argument.
const DEBUG_BUILDER_FIELD_METHODS: &[&str] = &["field", "entry", "key", "value"];

/// The `debug_*` builder constructors whose returned builder a field method
/// chains off of.
const DEBUG_BUILDER_CTORS: &[&str] = &[
    "debug_struct",
    "debug_tuple",
    "debug_list",
    "debug_set",
    "debug_map",
];

/// True when the `format!` invocation supplies a *value* argument of a
/// debug-builder field method — `.field(name, &format!(...))` / `.entry(...)` /
/// `.key(...)` / `.value(...)` — whose receiver chain roots at a `debug_struct`
/// / `debug_tuple` / `debug_list` / `debug_set` / `debug_map` call. Those
/// methods take a `&dyn Debug`; a render combining several values can only be
/// produced via `format!`, and `write!`-ing into the formatter would bypass the
/// builder and emit malformed output, so this `format!` cannot be rewritten.
///
/// The receiver may root at the constructor inline (`f.debug_struct("Foo")
/// .field(...)`) or through a `let` binding when the builder is bound to a
/// local and fields are added across statements (`let mut debug =
/// f.debug_struct(..); if let Some(v) = .. { debug.field(..) }`). Requiring the
/// resolved receiver to root at a `debug_*` constructor keeps the exemption
/// precise: a `.field(&format!(...))` on an arbitrary (non-builder) receiver is
/// not blanket-exempted.
fn is_debug_builder_field_value(format_node: tree_sitter::Node, source: &[u8]) -> bool {
    let value = climb_value_wrappers(format_node);
    let Some(arguments) = value.parent() else {
        return false;
    };
    if arguments.kind() != "arguments" {
        return false;
    }
    let Some(call) = arguments.parent() else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let is_field_method = function
        .child_by_field_name("field")
        .and_then(|field| field.utf8_text(source).ok())
        .is_some_and(|name| DEBUG_BUILDER_FIELD_METHODS.contains(&name));
    if !is_field_method {
        return false;
    }
    function
        .child_by_field_name("value")
        .is_some_and(|receiver| receiver_roots_at_debug_builder(receiver, source))
}

/// Whether a debug-builder field method's `receiver` roots at a `debug_*`
/// constructor — either inline in the method-call chain, or, when the receiver
/// is a bare local identifier, through the initializer of its `let` binding.
///
/// The local-binding form is idiomatic when fields are added conditionally
/// (`let mut debug = f.debug_struct(..); if let Some(v) = .. { debug.field(..) }`),
/// which a fluent chain cannot express.
fn receiver_roots_at_debug_builder(receiver: tree_sitter::Node, source: &[u8]) -> bool {
    if receiver_chain_roots_at_debug_builder(receiver, source) {
        return true;
    }
    receiver.kind() == "identifier"
        && receiver
            .utf8_text(source)
            .ok()
            .and_then(|name| local_let_binding_init(receiver, name, source))
            .is_some_and(|init| receiver_chain_roots_at_debug_builder(init, source))
}

/// Resolves the bare identifier `name` (a `.field(..)` receiver) to the
/// initializer of its nearest preceding `let name = <init>` binding in an
/// enclosing scope. Walks outward from `node`; within each scope only bindings
/// declared before the use are considered and the last such binding wins
/// (honoring shadowing). Mirrors the ancestor walk in
/// `rust_helpers::local_let_binds_*`.
fn local_let_binding_init<'a>(
    node: tree_sitter::Node<'a>,
    name: &str,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut child = node;
    while let Some(parent) = child.parent() {
        let mut init = None;
        let mut cursor = parent.walk();
        for sib in parent.children(&mut cursor) {
            if sib.id() == child.id() {
                break;
            }
            if sib.kind() == "let_declaration"
                && let Some(pattern) = sib.child_by_field_name("pattern")
                && crate::rules::rust_helpers::let_pattern_binds(pattern, name, source)
            {
                init = sib.child_by_field_name("value");
            }
        }
        if init.is_some() {
            return init;
        }
        child = parent;
    }
    None
}

/// Walks a method-call receiver chain (`a.b(..).c(..)` → `c`'s receiver is
/// `a.b(..)`, whose receiver is `a`) looking for a `debug_*` builder
/// constructor call (`f.debug_struct(...)`). A chain whose only links are
/// further field/method accesses on a `debug_*` call returns true.
fn receiver_chain_roots_at_debug_builder(receiver: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = receiver;
    loop {
        match current.kind() {
            "call_expression" => {
                if call_is_debug_builder_ctor(current, source) {
                    return true;
                }
                // Step to the receiver of this call's method (`recv.method(..)`).
                let Some(function) = current.child_by_field_name("function") else {
                    return false;
                };
                if function.kind() != "field_expression" {
                    return false;
                }
                match function.child_by_field_name("value") {
                    Some(next) => current = next,
                    None => return false,
                }
            }
            // `a.b` access on a partial chain (rare): descend into the value.
            "field_expression" => match current.child_by_field_name("value") {
                Some(next) => current = next,
                None => return false,
            },
            _ => return false,
        }
    }
}

/// `call`'s function is a `field_expression` whose field is one of the
/// `debug_*` builder constructors.
fn call_is_debug_builder_ctor(call: tree_sitter::Node, source: &[u8]) -> bool {
    call.child_by_field_name("function")
        .filter(|function| function.kind() == "field_expression")
        .and_then(|function| function.child_by_field_name("field"))
        .and_then(|field| field.utf8_text(source).ok())
        .is_some_and(|name| DEBUG_BUILDER_CTORS.contains(&name))
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

/// True when the `format!` produces an owned `String` *value* — bound to a
/// `let` or returned from a closure — rather than streamed into a formatter.
/// In those positions there is no `f` to `write!` to: a closure returning
/// `String` (`unwrap_or_else(|e| format!(...))`, `.map(|i| { ...; format!(...) })`)
/// and a `let s = format!(...)` binding must yield an owned `String`, so the
/// `format!` cannot be rewritten as `write!`.
///
/// The macro may reach its value slot directly, behind a `&` reference, or via
/// a `format!(...).as_str()`-style adapter, mirroring the climb in
/// [`is_debug_builder_name_arg`].
fn format_is_owned_string_value(format_node: tree_sitter::Node) -> bool {
    let value = climb_value_wrappers(format_node);
    let Some(parent) = value.parent() else {
        return false;
    };
    // `let s = format!(...);` — the macro is the initializer of a binding.
    if parent.kind() == "let_declaration"
        && parent.child_by_field_name("value") == Some(value)
    {
        return true;
    }
    // `|e| format!(...)` — expression-body closure returning the value.
    if parent.kind() == "closure_expression"
        && parent.child_by_field_name("body") == Some(value)
    {
        return true;
    }
    // `|i| { ...; format!(...) }` — the value is the tail expression of a block
    // (last named child, no trailing `;`) whose parent is a closure.
    if parent.kind() == "block"
        && is_block_tail_expression(parent, value)
        && parent
            .parent()
            .is_some_and(|grand| grand.kind() == "closure_expression")
    {
        return true;
    }
    false
}

/// Walks up from `node` through value-flow wrappers — a `&` reference and
/// `.as_str()`-style adapter chains — returning the outermost node that still
/// denotes the same string value. Mirrors the wrapper climb in
/// [`is_debug_builder_name_arg`].
fn climb_value_wrappers(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "reference_expression" => current = parent,
            "field_expression" if parent.child_by_field_name("value") == Some(current) => {
                current = parent;
            }
            "call_expression" if parent.child_by_field_name("function") == Some(current) => {
                current = parent;
            }
            _ => break,
        }
    }
    current
}

/// `child` is the tail expression of `block` — its last named child, with no
/// trailing `;` (a trailing-semicolon statement would not be the block's value).
fn is_block_tail_expression(block: tree_sitter::Node, child: tree_sitter::Node) -> bool {
    let count = block.named_child_count();
    count > 0 && block.named_child(count - 1) == Some(child)
}

fn collect_format_macros_in(
    body: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let body_id = body.id();
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        // A `function_item` nested below the impl body is a helper function
        // declared inside a method (e.g. inside `fmt`). It opens its own scope
        // where the method's formatter is not visible, so a `format!` there is
        // the helper's concern, not the method's formatting flow — don't
        // descend into it. The impl's own methods are direct children of the
        // impl body (`declaration_list`) and must still be scanned.
        if node.kind() == "function_item"
            && node.parent().map(|parent| parent.id()) != Some(body_id)
        {
            continue;
        }
        if node.kind() == "macro_invocation"
            && let Some(macro_node) = node.child_by_field_name("macro")
            && let Ok(name) = macro_node.utf8_text(source)
            && name == "format"
            && !is_debug_builder_name_arg(node, source)
            && !is_debug_builder_field_value(node, source)
            && !format_args_contain_truncation_signal(node, source)
            && !format_is_owned_string_value(node)
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
                severity: Severity::Error,
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
    fn allows_format_name_and_field_value_on_local_debug_struct() {
        // sled src/db.rs: `debug_struct` is bound to a local. The
        // `debug_struct(&format!(..))` name arg (a const generic can only be
        // spelled at runtime) and the `.field(.., &format!(..))` value (a
        // `&dyn Debug` slot `write!` cannot fill) are both exempt. See #1326,
        // #7244.
        let source = r#"impl<const LEAF_FANOUT: usize> fmt::Debug for Db<LEAF_FANOUT> {
            fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut debug_struct = w.debug_struct(&format!("Db<{}>", LEAF_FANOUT));
                debug_struct
                    .field("data", &format!("{:?}", self.iter().collect::<Vec<_>>()))
                    .finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_as_inline_debug_struct_field_value() {
        // The `.field(..)` value is a `&dyn Debug`; `write!`-ing into the
        // formatter would bypass the `debug_struct` builder. With the receiver
        // chain rooting inline at `debug_struct`, this `format!` is exempt.
        // See #4694.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_struct("Foo")
                    .field("x", &format!("{}-{}", self.a, self.b))
                    .finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
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

    #[test]
    fn allows_format_as_debug_struct_field_value_issue_4694() {
        // georust/geo edge_end.rs: two coords combined into one debug-display
        // string for a `.field(..)` value. `DebugStruct::field` needs a
        // `&dyn Debug`; `write!` would bypass the builder. See #4694.
        let source = r#"impl<F: GeoFloat> fmt::Debug for EdgeEndKey<F> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct("EdgeEndKey")
                    .field(
                        "coords",
                        &format!("{:?} -> {:?}", &self.coord_0, &self.coord_1),
                    )
                    .field("quadrant", &self.quadrant)
                    .finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_as_debug_tuple_field_value() {
        // A `debug_tuple` builder's `.field(&format!(...))` value is exempt for
        // the same reason. See #4694.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple("Foo")
                    .field(&format!("{:?}/{:?}", self.a, self.b))
                    .finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_as_debug_map_entry_value() {
        // `debug_map().entry(key, &format!(...))` value is exempt. See #4694.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_map()
                    .entry(&"k", &format!("{:?}->{:?}", self.a, self.b))
                    .finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_field_value_format_on_non_builder_receiver() {
        // `.field(&format!(...))` on a receiver that does NOT root at a `debug_*`
        // builder is not blanket-exempted — the precision guard for #4694.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let _ = self.builder.field("x", &format!("{}-{}", self.a, self.b));
                write!(f, "Foo")
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_format_in_field_value_on_local_debug_builder_issue_7244() {
        // meilisearch types.rs: the builder is bound to a local and `.field(..)`
        // is called on it inside `if let Some(..)` — the conditional form a
        // fluent chain can't express. The receiver identifier resolves to a
        // `debug_struct` binding, so the un-rewritable `.field` value `format!`
        // (no `.len()` truncation signal in the args) is exempt. See #7244.
        let source = r#"impl fmt::Debug for FederatedSearchResult {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut debug = f.debug_struct("SearchResult");
                if let Some(query_vectors) = self.query_vectors {
                    let known = query_vectors.len();
                    debug.field("query_vectors", &format!("[{known} known vectors]"));
                }
                debug.finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_in_entry_value_on_local_debug_list_builder() {
        // A `debug_list` builder bound to a local: `.entry(&format!(..))` is
        // exempt once the receiver identifier resolves to the `debug_list`
        // binding. See #7244.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut list = f.debug_list();
                list.entry(&format!("{:?}", self.x));
                list.finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_field_value_format_on_local_non_builder_binding() {
        // A `.field(..)` receiver identifier that resolves to a NON-debug-builder
        // binding must not be exempted by the local-binding resolution path: the
        // `format!` is still the waste the rule targets. See #7244.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut thing = SomeStruct::new();
                thing.field("k", &format!("{}-{}", self.a, self.b));
                write!(f, "Foo")
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_format_as_unwrap_or_else_closure_return() {
        // databend: the `format!` is the fallback value of a fallible
        // conversion. The closure must return an owned `String`; there is no
        // formatter in scope to `write!` to. See #3798.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let geom = conv(self.s).unwrap_or_else(|e| format!("err: {:?}", e));
                write!(f, "{geom:?}")
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_as_map_closure_block_tail() {
        // databend: the `format!` is the tail expression of a `.map(|i| {...})`
        // closure yielding `String` items to `debug_list().entries(...)`. See
        // #3798.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.debug_list().entries((0..self.n).map(|i| {
                    let s = self.get(i);
                    format!("0x{}", s)
                })).finish()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_as_let_initializer() {
        // An owned `String` bound for later use is not a write sink. See #3798.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let s = format!("{:?}", self.x);
                write!(f, "{s}")
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_format_in_nested_helper_fn_inside_fmt() {
        // criterion.rs src/report.rs: the `format!` is inside `fn format_opt`,
        // a helper declared inside `fmt`. The outer formatter `f` is not in
        // scope there; the helper must return an owned `String`. See #5174.
        let source = r#"impl fmt::Debug for BenchmarkId {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fn format_opt(opt: &Option<String>) -> String {
                    match *opt {
                        Some(ref string) => format!("\"{}\"", string),
                        None => "None".to_owned(),
                    }
                }
                write!(
                    f,
                    "BenchmarkId {{ value_str: {} }}",
                    format_opt(&self.value_str),
                )
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_direct_fmt_format_alongside_nested_helper_fn() {
        // The nested-fn helper's `format!` is exempt, but a `format!` written
        // directly in the `fmt` body is still the waste the rule targets. See
        // #5174 — the exclusion must not leak to the method's own flow.
        let source = r#"impl fmt::Debug for Foo {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fn helper(x: u32) -> String {
                    format!("{x}")
                }
                f.write_str(&format!("Foo({})", helper(self.x)))
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_format_statement_in_closure_block_not_tail() {
        // A `format!(...);` statement (trailing `;`, not the block tail) inside
        // a closure is a discarded throwaway allocation — the owned-value
        // exemption must not reach it.
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.debug_list().entries((0..self.n).map(|i| {
                    format!("discarded {}", i);
                    self.get(i)
                })).finish()
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }
}
