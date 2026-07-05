//! rust-prefer-cow backend.
//!
//! Walks `function_item` nodes, keeps those with a `pub` visibility
//! modifier (callers outside the crate are the ones that suffer from a
//! forced `.to_string()` at the call site). For each `parameter` child
//! of the signature, flag when:
//!
//! - the `type` field text is exactly `String`, and
//! - the parameter pattern is not `mut` (mutable ownership is a real
//!   signal the function rewrites the buffer; leave those alone).
//!
//! Generic `String` aliases (`std::string::String`), `&String`, and
//! `Option<String>` are deliberately not flagged — keeping the match
//! shallow mirrors the other conservative Rust rules in this crate.
//!
//! A parameter is also left alone when the body moves it by value, either
//! into a struct/enum-variant literal (`Thing { name }` / `Variant { error }`
//! / `Thing { name: name }`), as a bare argument of a call — a function
//! call, a method call, or an enum tuple-variant constructor
//! (`some_fn(name)` / `Variant::String(name)`) — as a bare `identifier`
//! element of an array literal (`[name]`) or a `vec!` macro (`vec![name]`),
//! both of which move their elements into the owned collection, by a bare
//! assignment that stores it in an owned place (`self.field = name`), or by
//! being referenced
//! anywhere inside a `move` closure (`move || { … name … }`), which captures
//! every used variable by value. There the function genuinely needs ownership,
//! so taking `String` is the correct API — switching to `&str` would only
//! shift the allocation into the body, and a `move` closure that must be
//! `'static` (e.g. `thread::spawn`) cannot capture a borrow at all. A borrow
//! (`&name`) or a clone (`name.clone()`) outside a `move` closure does not
//! consume the owned value and still warrants the warning.
//!
//! The parameter is also moved out when it is returned by value: a bare
//! `identifier` that is the operand of a `return` (`return name;`) or the tail
//! expression the function body evaluates to — reached through the tail of any
//! `if`/`else`/`match`/`if let` that itself flows to the return. Returning the
//! owned value by name hands the caller's allocation straight back; an `&str`
//! parameter would force a `.to_owned()` in that branch, so `String` is correct.
//!
//! A function whose name is referenced as a bare value elsewhere in the file
//! (e.g. `Some(gdbus_parse_color)` stored in a `fn(String) -> …` field, or
//! `iter.map(gdbus_parse_color)`) is also left alone: such a reference uses the
//! function as a pointer, so its signature is locked by the pointer type and
//! relaxing `String` to `&str`/`Cow` would no longer match it (a compile error).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, is_pub};

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    if is_in_test_context(node, source) { return; }
    if !is_pub(node, source) { return; }

    let Some(params) = node.child_by_field_name("parameters") else { return; };
    let body = node.child_by_field_name("body");
    // Lazily computed at most once: whether this function is referenced as a
    // function-pointer value (which locks its signature). Only needed when a
    // parameter would otherwise be flagged, so most functions never pay for it.
    let mut referenced_as_pointer: Option<bool> = None;
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        if param.kind() != "parameter" { continue; }
        let Some(type_node) = param.child_by_field_name("type") else { continue; };
        let Ok(type_text) = type_node.utf8_text(source) else { continue; };
        if type_text.trim() != "String" { continue; }
        // Skip `mut s: String` — author explicitly wants ownership to mutate in place.
        // tree-sitter-rust exposes `mut` on a parameter as an anonymous
        // `mutable_specifier` child, not via the `pattern` field.
        let mut param_cursor = param.walk();
        let has_mut = param.children(&mut param_cursor)
            .any(|c| c.kind() == "mutable_specifier");
        if has_mut { continue; }

        // Skip params the body moves by value (into a struct/enum literal or
        // as a bare call argument) — ownership is genuinely needed there.
        let Some(pattern) = param.child_by_field_name("pattern") else { continue; };
        if pattern.kind() != "identifier" { continue; }
        let Ok(param_name) = pattern.utf8_text(source) else { continue; };
        if let Some(body) = body
            && (param_is_moved(body, source, param_name)
                || tail_expr_moves_param(body, source, param_name))
        {
            continue;
        }

        // Skip when the function itself is referenced as a bare value (a
        // function pointer): the pointer type fixes its signature, so `String`
        // cannot be relaxed. Computed once for the whole function.
        let is_pointer = *referenced_as_pointer.get_or_insert_with(|| {
            node.child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
                .is_some_and(|fn_name| {
                    let mut root = node;
                    while let Some(parent) = root.parent() {
                        root = parent;
                    }
                    referenced_as_value(root, source, fn_name)
                })
        });
        if is_pointer {
            return;
        }

        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &param,
            super::META.id,
            "Public fn takes owned `String` — forces every caller to allocate. Prefer `&str` (no ownership) or `impl Into<Cow<'_, str>>` (conditional ownership).".into(),
            Severity::Warning,
        ));
    }
}

/// Whether `param_name` is moved by value anywhere in `node`'s subtree, either
/// as a bare `identifier` field value of a struct/enum-variant literal
/// (`{ x }` shorthand, or `{ x: x }`), as a bare `identifier` argument of a
/// call (`some_fn(x)`, `obj.method(x)`, `Variant::String(x)`), as a bare
/// `identifier` element of an array literal (`[x]`) or a `vec!` macro
/// (`vec![x]`), as the bare `identifier` right-hand side of an assignment
/// (`self.field = x`), or as the bare `identifier` operand of a `return`
/// (`return x;`, which moves the owned value out as the function's return
/// value). A bare identifier consumes the owned value; `&x`
/// (`reference_expression`) and `x.clone()` (a method call whose `arguments`
/// list does not contain `x`) do not, so they are ignored and still warrant the
/// warning.
///
/// A parameter returned by value as the body's tail expression (no `return`
/// keyword) is positional, not a subtree property, so it is handled separately
/// by [`tail_expr_moves_param`].
fn param_is_moved(node: tree_sitter::Node, source: &[u8], param_name: &str) -> bool {
    match node.kind() {
        "struct_expression" => {
            if let Some(fields) = node.child_by_field_name("body") {
                let mut cursor = fields.walk();
                for field in fields.named_children(&mut cursor) {
                    let value = match field.kind() {
                        "shorthand_field_initializer" => {
                            let mut field_cursor = field.walk();
                            field
                                .named_children(&mut field_cursor)
                                .find(|c| c.kind() == "identifier")
                        }
                        "field_initializer" => field.child_by_field_name("value"),
                        _ => None,
                    };
                    if is_param_identifier(value, source, param_name) {
                        return true;
                    }
                }
            }
        }
        "call_expression" => {
            // A bare `identifier` argument transfers ownership. `&x` is a
            // `reference_expression` (borrow) and `x.method()` puts `x` under
            // a `field_expression` callee, not in this `arguments` list, so
            // neither matches.
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cursor = args.walk();
                for arg in args.named_children(&mut cursor) {
                    if is_param_identifier(Some(arg), source, param_name) {
                        return true;
                    }
                }
            }
        }
        "assignment_expression" => {
            // `self.field = x` stores the owned value in a place the struct
            // owns; ownership is genuinely needed, so `String` is correct. Only
            // a bare `identifier` right-hand side moves — `&x` is a
            // `reference_expression` and `x.clone()` a `call_expression`.
            if is_param_identifier(node.child_by_field_name("right"), source, param_name) {
                return true;
            }
        }
        "closure_expression" => {
            // A `move` closure captures every used variable BY VALUE, so a
            // `String` referenced anywhere inside `move || { … }` is moved into
            // the closure (which often must be `'static`, e.g. `thread::spawn`,
            // where a borrow cannot compile). Ownership transfer — the same
            // class as moving into a struct/enum literal. A non-`move` closure
            // borrows instead, so it is left to the child recursion below.
            if closure_is_move(node)
                && subtree_references_identifier(node, source, param_name)
            {
                return true;
            }
        }
        "array_expression" => {
            // Array elements move by value into the owned array, so a bare
            // `identifier` element (`[x]`, `[a, x]`) consumes the param. `&x`
            // is a `reference_expression` element and `x.clone()` a
            // `call_expression` element — distinct node kinds — so a borrow or
            // clone stays flagged.
            let mut cursor = node.walk();
            for element in node.named_children(&mut cursor) {
                if is_param_identifier(Some(element), source, param_name) {
                    return true;
                }
            }
        }
        "macro_invocation" => {
            // `vec![x]` moves its elements into the owned `Vec`. Scope to the
            // `vec` std collection macro: a format macro (`println!`, `write!`,
            // `format!`) only borrows its substitution arguments, so those stay
            // flagged. A macro `token_tree` holds flat tokens (no expression
            // shape), so a bare-`identifier` element move is one whose
            // neighbours are element boundaries — `&x` leaves a `&` sibling and
            // `x.clone()` a `.` sibling, both of which stay flagged.
            if macro_name_is_vec(node, source)
                && vec_moves_identifier(node, source, param_name)
            {
                return true;
            }
        }
        "return_expression" => {
            // `return x;` moves the owned value out as the function's return
            // value. The returned expression is the sole named child; a bare
            // `identifier` there transfers ownership, whereas `return &x` is a
            // `reference_expression` and `return x.clone()` a `call_expression`.
            if is_param_identifier(node.named_child(0), source, param_name) {
                return true;
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if param_is_moved(child, source, param_name) {
            return true;
        }
    }
    false
}

/// Whether `param_name` is moved out as the function's return value by being the
/// bare `identifier` the body evaluates to. Descends only *tail* positions from
/// the body block: a block's trailing expression, and the arms of an
/// `if`/`else`/`match`/`if let` that is itself in tail position (`if let` is an
/// `if_expression` whose condition is a `let_condition`). Only positions whose
/// value flows to the return count, so a bare identifier used elsewhere
/// (`s.len()`, `println!("{}", s)`) is not treated as a move here.
fn tail_expr_moves_param(node: tree_sitter::Node, source: &[u8], param_name: &str) -> bool {
    match node.kind() {
        "identifier" => node.utf8_text(source) == Ok(param_name),
        "block" => {
            block_tail_expr(node).is_some_and(|tail| tail_expr_moves_param(tail, source, param_name))
        }
        "if_expression" => {
            // An `if` yields a value only with an `else`; then both the
            // consequence and the alternative are in tail position.
            let Some(alt) = node.child_by_field_name("alternative") else {
                return false;
            };
            node.child_by_field_name("consequence")
                .is_some_and(|c| tail_expr_moves_param(c, source, param_name))
                || tail_expr_moves_param(alt, source, param_name)
        }
        "else_clause" => node
            .named_child(0)
            .is_some_and(|c| tail_expr_moves_param(c, source, param_name)),
        "match_expression" => node.child_by_field_name("body").is_some_and(|body| {
            let mut cursor = body.walk();
            body.named_children(&mut cursor).any(|arm| {
                arm.kind() == "match_arm"
                    && arm
                        .child_by_field_name("value")
                        .is_some_and(|v| tail_expr_moves_param(v, source, param_name))
            })
        }),
        _ => false,
    }
}

/// The trailing expression a `block` evaluates to, or `None` when it evaluates
/// to `()` (empty, or ending in a semicolon-terminated statement).
///
/// A bare trailing expression (`… s`) is a direct child. A block-like trailing
/// expression (`… if c { … } else { … }`) has no semicolon, yet the grammar
/// still wraps it in an `expression_statement` (`prec(1, …_ending_with_block)`);
/// that wrapper *is* the block's value, so it is unwrapped. A wrapper that ends
/// in `;` discards its value, so the block carries no tail.
fn block_tail_expr(block: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = block.walk();
    let last = block
        .named_children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "line_comment" | "block_comment"))
        .last()?;
    if last.kind() != "expression_statement" {
        return Some(last);
    }
    match last.child(last.child_count().saturating_sub(1)) {
        Some(semi) if semi.kind() == ";" => None,
        _ => last.named_child(0),
    }
}

/// Whether a `closure_expression` is a `move` closure (`move || …`). The
/// grammar exposes the `move` keyword as a dedicated anonymous child token.
fn closure_is_move(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| c.kind() == "move")
}

/// Whether `node`, a `macro_invocation`, invokes the std `vec!` macro (its
/// `macro` name is exactly `vec`). Scoped to `vec` so format macros
/// (`println!`, `write!`, `format!`), which only borrow their arguments, are
/// not treated as moves.
fn macro_name_is_vec(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("macro")
        .and_then(|m| m.utf8_text(source).ok())
        .is_some_and(|name| name == "vec")
}

/// Whether `param_name` appears as a bare `identifier` element of the `vec!`
/// macro's `token_tree` (`vec![name]`, `vec![a, name]`). A `token_tree` holds
/// flat tokens, so an element move is a bare `identifier` flanked by
/// element-boundary tokens; a leading `&` (`vec![&name]`, a borrow) or a
/// trailing `.` (`vec![name.clone()]`, a method/field access) leaves a
/// non-boundary sibling, so those stay flagged.
fn vec_moves_identifier(node: tree_sitter::Node, source: &[u8], param_name: &str) -> bool {
    let mut macro_cursor = node.walk();
    let Some(tree) = node
        .children(&mut macro_cursor)
        .find(|c| c.kind() == "token_tree")
    else {
        return false;
    };
    let mut cursor = tree.walk();
    tree.children(&mut cursor).any(|token| {
        token.kind() == "identifier"
            && token.utf8_text(source) == Ok(param_name)
            && is_element_boundary(token.prev_sibling())
            && is_element_boundary(token.next_sibling())
    })
}

/// Whether `sibling` delimits a `token_tree` element: a bracket/paren/brace or
/// an element separator (`,` / `;`). A missing sibling counts as a boundary.
fn is_element_boundary(sibling: Option<tree_sitter::Node>) -> bool {
    match sibling {
        None => true,
        Some(node) => matches!(node.kind(), "[" | "(" | "{" | "]" | ")" | "}" | "," | ";"),
    }
}

/// Whether `param_name` appears as a bare `identifier` anywhere in `node`'s
/// subtree.
fn subtree_references_identifier(
    node: tree_sitter::Node,
    source: &[u8],
    param_name: &str,
) -> bool {
    if node.kind() == "identifier" && node.utf8_text(source) == Ok(param_name) {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| subtree_references_identifier(child, source, param_name))
}

/// Whether `node` is a bare `identifier` whose text equals `param_name`.
fn is_param_identifier(
    node: Option<tree_sitter::Node>,
    source: &[u8],
    param_name: &str,
) -> bool {
    node.is_some_and(|n| n.kind() == "identifier" && n.utf8_text(source) == Ok(param_name))
}

/// Whether `fn_name` is referenced as a bare value (a function-pointer value)
/// anywhere in `root`'s subtree — e.g. `Some(gdbus_parse_color)` or
/// `iter.map(gdbus_parse_color)`. Such a reference locks the function's
/// signature to the pointer type, so its `String` parameter cannot be relaxed.
///
/// Every `identifier` node whose text equals `fn_name` counts, EXCEPT the
/// definition/call positions reported by [`is_definition_or_call_callee`]
/// (a `function_item` name or a direct call's callee). Method and field names
/// are `field_identifier` nodes — not `identifier` — so `x.f` / `x.f()` never
/// match. Suppression-only: an over-broad name match can only mute a warning
/// (a harmless false negative), never raise a new one.
fn referenced_as_value(root: tree_sitter::Node, source: &[u8], fn_name: &str) -> bool {
    if root.kind() == "identifier"
        && root.utf8_text(source) == Ok(fn_name)
        && !is_definition_or_call_callee(root)
    {
        return true;
    }
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .any(|child| referenced_as_value(child, source, fn_name))
}

/// Whether `node` (an `identifier`) sits in a position that is a definition or a
/// direct call rather than a value reference: the `name` field of a
/// `function_item` (a definition site, including the function's own name), or
/// the `function` field of a `call_expression` (a direct call `f(…)`).
fn is_definition_or_call_callee(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else { return false; };
    let field = match parent.kind() {
        "function_item" => parent.child_by_field_name("name"),
        "call_expression" => parent.child_by_field_name("function"),
        _ => return false,
    };
    field.map(|f| f.id()) == Some(node.id())
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_pub_fn_with_owned_string() {
        // The body only borrows `name` (method receiver), so `&str` would
        // compile — the warning holds.
        assert_eq!(
            run("pub fn greet(name: String) -> String { name.to_uppercase() }").len(),
            1
        );
    }

    #[test]
    fn flags_pub_fn_with_two_string_params() {
        assert_eq!(
            run("pub fn join(a: String, b: String) -> String { a + &b }").len(),
            2
        );
    }

    #[test]
    fn allows_private_fn_with_owned_string() {
        assert!(run("fn greet(name: String) -> String { name }").is_empty());
    }

    #[test]
    fn allows_pub_crate_fn_with_owned_string() {
        // `pub(crate)` is crate-internal, not external API — the rule's premise
        // (external callers forced to allocate) does not apply. Reproduces #3913.
        assert!(run("pub(crate) fn display(query: String) -> usize { query.len() }").is_empty());
    }

    #[test]
    fn allows_pub_super_fn_with_owned_string() {
        assert!(run("pub(super) fn g(s: String) -> usize { s.len() }").is_empty());
    }

    #[test]
    fn allows_string_slice_param() {
        assert!(run("pub fn greet(name: &str) -> String { name.into() }").is_empty());
    }

    #[test]
    fn allows_string_ref_param() {
        assert!(run("pub fn greet(name: &String) -> String { name.clone() }").is_empty());
    }

    #[test]
    fn allows_cow_param() {
        assert!(run("pub fn greet(name: Cow<'_, str>) -> String { name.into_owned() }").is_empty());
    }

    #[test]
    fn allows_mut_string_param() {
        assert!(run("pub fn fill(mut buf: String) -> String { buf.push('x'); buf }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests { pub fn f(s: String) {} }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_enum_variant() {
        assert!(
            run("pub fn volt_installing(&self, error: String) { self.notification(CoreNotification::VoltInstalling { error }); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_param_moved_into_struct_shorthand() {
        assert!(run("fn make(name: String) -> Thing { Thing { name } }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_struct_explicit_field() {
        assert!(run("pub fn make(name: String) -> Thing { Thing { name: name } }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_field_assignment() {
        // Repro of #4891: a setter that moves the param into an owned struct
        // field stores ownership, so `String` is correct — `&str` would force a
        // `.to_string()` in the body and `Cow` would change the field type too.
        assert!(
            run("pub fn set_title(&mut self, value: String) { self.title = value; }").is_empty()
        );
    }

    #[test]
    fn flags_param_borrowed_into_field_assignment() {
        // `self.field = &value` does not consume the owned value, so the param
        // is only borrowed — `&str` would compile and the warning still holds.
        assert_eq!(
            run("pub fn set_title(&mut self, value: String) { self.title = &value; }").len(),
            1
        );
    }

    #[test]
    fn flags_param_only_read_in_body() {
        assert_eq!(
            run(r#"pub fn log(msg: String) { println!("{}", msg); }"#).len(),
            1
        );
    }

    #[test]
    fn flags_param_read_only_len() {
        assert_eq!(run("pub fn count(s: String) -> usize { s.len() }").len(), 1);
    }

    #[test]
    fn flags_param_borrowed_into_struct() {
        assert_eq!(
            run("pub fn make(name: String) -> Thing { Thing { name: &name } }").len(),
            1
        );
    }

    #[test]
    fn flags_param_cloned_into_struct() {
        assert_eq!(
            run("pub fn make(name: String) -> Thing { Thing { name: name.clone() } }").len(),
            1
        );
    }

    #[test]
    fn allows_param_moved_into_enum_tuple_variant() {
        assert!(
            run("pub(crate) fn unrecognized_subcommand(subcmd: String) -> Self { let mut err = Self::new(); err = err.extend([(ContextKind::InvalidSubcommand, ContextValue::String(subcmd))]); err }")
                .is_empty()
        );
    }

    #[test]
    fn allows_param_forwarded_into_call() {
        assert!(run("pub fn f(s: String) { some_fn(s) }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_method_call() {
        assert!(run("pub fn f(s: String) { self.push(s) }").is_empty());
    }

    #[test]
    fn flags_param_borrowed_into_call() {
        assert_eq!(run("pub fn f(s: String) { g(&s) }").len(), 1);
    }

    #[test]
    fn flags_param_read_via_method_into_call() {
        assert_eq!(run("pub fn f(s: String) { g(s.len()) }").len(), 1);
    }

    #[test]
    fn flags_param_cloned_into_call() {
        assert_eq!(run("pub fn f(s: String) { g(s.clone()) }").len(), 1);
    }

    #[test]
    fn allows_param_moved_into_vec_macro() {
        // Repro of #7205 (rust-lang/cargo `compile_filter.rs`): `bin` is moved
        // by value into `vec![bin]`, which becomes an owned `Vec<String>`, so
        // taking `bin: String` by value is the correct API.
        assert!(run("pub fn single_bin(bin: String) -> X { X::new(vec![bin], false) }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_multi_element_vec() {
        assert!(run("pub fn f(a: String, b: String) -> Vec<String> { vec![a, b] }").is_empty());
    }

    #[test]
    fn allows_param_moved_into_array_literal() {
        assert!(run("pub fn f(bin: String) -> [String; 1] { [bin] }").is_empty());
    }

    #[test]
    fn flags_param_borrowed_into_vec_element() {
        // `vec![&s]` borrows `s` into a `Vec<&String>` — the param is not
        // consumed, so `&str` would compile and the warning still holds.
        assert_eq!(run("pub fn f(s: String) { let _ = vec![&s]; }").len(), 1);
    }

    #[test]
    fn flags_param_cloned_into_vec_element() {
        // `vec![s.clone()]` only borrows `s` to clone it; the param is not moved.
        assert_eq!(run("pub fn f(s: String) { let _ = vec![s.clone()]; }").len(), 1);
    }

    #[test]
    fn flags_param_borrowed_into_array_element() {
        // `[&s]` borrows `s`; the array element is a `reference_expression`, not
        // a bare `identifier`, so the param is not moved.
        assert_eq!(run("pub fn f(s: String) { let _ = [&s]; }").len(), 1);
    }

    #[test]
    fn flags_param_in_format_macro() {
        // The `vec!` move recognition is scoped to `vec`; a format macro only
        // borrows its substitution arguments, so `format!` stays flagged.
        assert_eq!(run(r#"pub fn f(s: String) -> String { format!("{}", s) }"#).len(), 1);
    }

    #[test]
    fn allows_param_referenced_in_move_closure() {
        // Repro of #4399: `thread::spawn(move || …)` requires a `'static`
        // closure, so the captured `String` must be owned — a borrow or `Cow`
        // would not compile. The param is referenced as `&output_display`
        // inside the move closure, which still captures it by value.
        assert!(
            run("pub fn spawn(output_display: String) { std::thread::spawn(move || { let _ = format!(\"{}\", &output_display); }); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_param_consumed_in_move_closure() {
        assert!(
            run("pub fn run(s: String) { std::thread::spawn(move || { consume(s); }); }")
                .is_empty()
        );
    }

    #[test]
    fn flags_param_borrowed_in_non_move_closure() {
        // A non-`move` closure borrows the param, so `&str` would compile.
        assert_eq!(
            run("pub fn f(s: String) { let c = || println!(\"{}\", s); c(); }").len(),
            1
        );
    }

    #[test]
    fn flags_param_borrowed_when_move_closure_skips_it() {
        // The move closure does not reference the param; the param is only
        // borrowed elsewhere, so `&str` would compile.
        assert_eq!(
            run("pub fn f(s: String) { std::thread::spawn(move || { unrelated(); }); show(&s); }").len(),
            1
        );
    }

    #[test]
    fn allows_fn_referenced_as_function_pointer_value() {
        // Repro of #6651 (sharkdp/pastel): `gdbus_parse_color` is stored as
        // `Some(gdbus_parse_color)` in a `fn(String) -> …` field, so the pointer
        // type fixes its signature — `&str`/`Cow` would not type-check. The body
        // only borrows `raw` (`.split('(')`), yet the function must stay silent.
        assert!(
            run(
                "pub struct ColorPickerTool { pub post_process: Option<fn(String) -> Result<String, &'static str>> }\n\
                 pub fn gdbus_parse_color(raw: String) -> Result<String, &'static str> { let _ = raw.split('('); Err(\"x\") }\n\
                 fn make() -> ColorPickerTool { ColorPickerTool { post_process: Some(gdbus_parse_color) } }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fn_referenced_as_value_in_map() {
        // Passed by value to an adapter — same function-pointer locking.
        assert!(
            run(
                "pub fn upcase(s: String) -> String { s.to_uppercase() }\n\
                 fn run(it: Vec<String>) -> Vec<String> { it.into_iter().map(upcase).collect() }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_borrow_only_fn_that_is_only_called() {
        // Negative control: `f` is only ever CALLED (`f(...)`), never referenced
        // as a value, and its param is only borrowed — the warning still holds.
        assert_eq!(
            run(
                "pub fn f(s: String) -> usize { s.len() }\n\
                 pub fn caller() -> usize { f(String::new()) }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_borrow_only_fn_referenced_nowhere() {
        // Negative control: no value reference anywhere — still flags.
        assert_eq!(run("pub fn f(s: String) -> usize { s.len() }").len(), 1);
    }

    #[test]
    fn allows_param_returned_by_value_in_if_let_else_tail() {
        // Repro of #7421 (gitui-org/gitui `emoji.rs`): when the input has no
        // emoji, `replace_all` yields `Cow::Borrowed` and the `else` branch
        // hands the caller's own allocation straight back by returning `s` by
        // value. An `&str` param would force `s.to_owned()` there, so owning is
        // correct.
        assert!(
            run("pub fn emojifi_string(s: String) -> String { if let Cow::Owned(a) = EMOJI_REPLACER.replace_all(&s) { a } else { s } }")
                .is_empty()
        );
    }

    #[test]
    fn allows_param_moved_out_by_return_statement() {
        // `return s;` moves the owned value out as the function's return value.
        assert!(run("pub fn f(s: String) -> String { return s; }").is_empty());
    }

    #[test]
    fn allows_param_returned_by_value_in_if_else_tail() {
        // The param is returned by value in the `else` arm of the body's tail
        // `if`; the arm flows to the function return, so `s` is moved out.
        assert!(
            run("pub fn g(s: String, cond: bool) -> String { if cond { other() } else { s } }")
                .is_empty()
        );
    }

    #[test]
    fn allows_param_returned_by_value_in_match_arm_tail() {
        assert!(
            run("pub fn h(s: String, n: u8) -> String { match n { 0 => other(), _ => s } }")
                .is_empty()
        );
    }

    #[test]
    fn flags_param_only_borrowed_via_len() {
        // Control: the param is only borrowed (`s.len()` takes `&self`), never
        // moved out — `&str` would compile, so the warning holds.
        assert_eq!(run("pub fn h(s: String) -> usize { s.len() }").len(), 1);
    }

    #[test]
    fn flags_param_only_read_in_println() {
        // Control: the param is only borrowed by the format macro, never moved
        // out — the warning holds.
        assert_eq!(run(r#"pub fn i(s: String) { println!("{}", s); }"#).len(), 1);
    }

    #[test]
    fn flags_param_method_call_in_tail_position() {
        // `s.clone()` in tail position borrows `s` as the method receiver — the
        // tail is a `call_expression`, not a bare `identifier`, so the param is
        // not moved out and the warning holds.
        assert_eq!(
            run("pub fn f(s: String) -> String { s.clone() }").len(),
            1
        );
    }

    #[test]
    fn flags_param_borrowed_in_middle_statement_not_tail() {
        // The `if` that mentions `s` is a middle statement (trailing `;`), so its
        // value is discarded, not returned; the real tail is `g()`. `s` is only
        // borrowed, so the warning holds.
        assert_eq!(
            run("pub fn f(s: String) -> usize { if s.is_empty() { one() } else { two() }; g() }").len(),
            1
        );
    }
}
