//! rust-vec-with-capacity backend.
//!
//! Matches `let [mut] X = Vec::new()` declarations and checks whether a
//! following sibling `for_expression` pushes into `X` unconditionally:
//! the `X.push(...)` must be a direct statement of the loop body (not
//! nested inside an `if`/`match`) and the body must contain no `continue`
//! that would skip iterations nor an early `break` that would exit before the
//! iterable is exhausted. Only then does the Vec's final length equal
//! the iterable's length, making `Vec::with_capacity(n)` the right call —
//! it avoids the log2(n) reallocation chain from doubling. A conditional
//! push, a `continue`, or an early `break` makes the final length unknowable up front, so
//! `with_capacity` would mis-size. Likewise a body that reassigns the
//! accumulator (`X = ...`) resets it each iteration, so its final length is
//! one segment's size rather than the iteration count, and the rule stays
//! silent.
//!
//! The iterable itself must be length-bearing — a bare binding or field of a
//! collection type (`v`, `self.items`), optionally behind one reference
//! (`&v`). Every other iterable shape is skipped: lazy/fallible ones in
//! particular (`make_items()`, `Iter::new(r)?`, `v.iter().filter(..)`) have no
//! cheaply known length to size the capacity from, so `with_capacity(n)` can't
//! be written. A bare identifier that is a generic `IntoIterator` function
//! parameter (`fn new<I>(input: I)` or `fn new(input: impl IntoIterator)`) is
//! likewise skipped: it has no `.len()`, so `Vec::with_capacity(input.len())`
//! would not compile. A bare identifier bound by a local `let` to one of those
//! same lazy shapes (`let it = xs.iter().map(..); for x in it`) is skipped too —
//! the lazy iterator has no cheap length whether written inline or hoisted into
//! a local.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["let_declaration"] => |node, source, ctx, diagnostics|
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "call_expression" { return; }
    let Some(fn_node) = value.child_by_field_name("function") else { return; };
    let fn_text = fn_node.utf8_text(source).unwrap_or("");
    if fn_text != "Vec::new" && fn_text != "std::vec::Vec::new" { return; }

    let Some(pattern) = node.child_by_field_name("pattern") else { return; };
    let Some(var_name) = extract_var_name(pattern, source) else { return; };

    let Some(parent) = node.parent() else { return; };
    let mut cursor = parent.walk();
    let mut after_us = false;
    let mut has_for_with_push = false;
    for sib in parent.children(&mut cursor) {
        if sib.id() == node.id() {
            after_us = true;
            continue;
        }
        if !after_us { continue; }
        let for_node = if sib.kind() == "for_expression" {
            sib
        } else if sib.kind() == "expression_statement"
            && let Some(inner) = sib.named_child(0)
            && inner.kind() == "for_expression"
        {
            inner
        } else {
            continue;
        };
        if iterable_has_known_length(for_node, source)
            && let Some(body) = for_node.child_by_field_name("body")
            && body_directly_pushes(body, var_name, source)
            && !body_has_continue(body)
            && !body_has_break(body)
            && !body_extends_or_appends(body, var_name, source)
            && !body_reassigns(body, var_name, source)
        {
            has_for_with_push = true;
            break;
        }
    }

    if has_for_with_push {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &value,
            super::META.id,
            format!("Use `Vec::with_capacity(...)` instead of `Vec::new()` when `{var_name}` is populated in a for-loop."),
            Severity::Warning,
        ));
    }
}

fn extract_var_name<'a>(pattern: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if pattern.kind() == "identifier" {
        return pattern.utf8_text(source).ok();
    }
    if pattern.kind() == "mut_pattern" {
        let mut cursor = pattern.walk();
        for child in pattern.children(&mut cursor) {
            if child.kind() == "identifier" {
                return child.utf8_text(source).ok();
            }
        }
    }
    None
}

/// Whether the `for_expression`'s iterable is a value whose length is cheaply
/// known, so `Vec::with_capacity(n)` has an `n` to supply. Length-bearing means
/// a bare `identifier` or `field_expression` (`v`, `self.items`), optionally
/// behind a single `reference_expression` (`&v`). Every other shape is skipped,
/// notably the lazy/fallible iterators that have no cheaply available length: a
/// `call_expression` (`make_items()`), a `try_expression` (`Iter::new(r)?`), or
/// an iterator-adaptor chain (`v.iter().filter(..)`, parsed as a
/// `call_expression` whose function is a `field_expression`).
///
/// A bare `identifier` is also skipped in two further cases, both because the
/// resolved iterable has no cheaply-known `.len()`. First, when it resolves to a
/// function parameter whose declared type is a generic `IntoIterator` — a bare
/// type parameter of the enclosing function (`fn new<I>(input: I)` with
/// `I: IntoIterator`) or an argument-position `impl IntoIterator`. Second, when
/// it resolves to a local `let` binding whose initializer is itself one of the
/// lazy shapes skipped inline — an iterator-adaptor chain or a fallible
/// `try_expression` (`let it = xs.iter().map(..); for x in it`): the same
/// reasoning applies whether the lazy iterator is written inline or hoisted into
/// a local. A local bound to a concrete collection (`Vec`, `vec![..]`, a plain
/// call) keeps its known length and still flags.
fn iterable_has_known_length(for_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(value) = for_node.child_by_field_name("value") else { return false; };
    let inner = if value.kind() == "reference_expression" {
        match value.child_by_field_name("value") {
            Some(n) => n,
            None => return false,
        }
    } else {
        value
    };
    if !matches!(inner.kind(), "identifier" | "field_expression") {
        return false;
    }
    if inner.kind() == "identifier" {
        if iterable_is_generic_param(inner, source) {
            return false;
        }
        if let Some(init) = resolve_local_binding(inner, source)
            && is_lazy_iterable_expr(init)
        {
            return false;
        }
    }
    true
}

/// Whether `expr` is a lazy/fallible iterator shape with no cheaply-known
/// `.len()`: an iterator-adaptor chain — a `call_expression` whose function is a
/// `field_expression` (`v.iter().filter(..)`, `xs.into_iter().kmerge_by(..)`) —
/// or a fallible `try_expression` (`Iter::new(r)?`). These are exactly the
/// shapes `iterable_has_known_length` treats as non-length-bearing when they
/// appear inline in the loop header; the same test applied to a resolved local
/// binding's initializer catches the hoisted-into-a-local form. A plain
/// `call_expression` (`build()`) is not matched: its result may be a concrete
/// collection, so the local keeps its known length.
fn is_lazy_iterable_expr(expr: tree_sitter::Node) -> bool {
    match expr.kind() {
        "try_expression" => true,
        "call_expression" => matches!(
            expr.child_by_field_name("function").map(|f| f.kind()),
            Some("field_expression")
        ),
        _ => false,
    }
}

/// The initializer of the nearest preceding `let <name> = <value>` binding for a
/// bare-identifier iterable, resolving it to what it was bound to. Walks outward
/// from `ident` through each enclosing `block`, scanning only the statements
/// before the loop, so the closest binding wins when a name is rebound
/// (shadowing). Stops at the enclosing function or a closure boundary (an
/// outer-scope capture is left unresolved). Returns `None` when the name is a
/// parameter, a field, or otherwise not bound by a local `let`.
fn resolve_local_binding<'a>(
    ident: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    let name = ident.utf8_text(source).ok()?;
    let mut node = ident;
    loop {
        let parent = node.parent()?;
        if parent.kind() == "block" {
            let mut found = None;
            let mut cursor = parent.walk();
            for sib in parent.children(&mut cursor) {
                if sib.id() == node.id() {
                    break;
                }
                if sib.kind() == "let_declaration"
                    && let Some(pattern) = sib.child_by_field_name("pattern")
                    && extract_var_name(pattern, source) == Some(name)
                    && let Some(value) = sib.child_by_field_name("value")
                {
                    found = Some(value);
                }
            }
            if found.is_some() {
                return found;
            }
        }
        if matches!(parent.kind(), "function_item" | "closure_expression") {
            return None;
        }
        node = parent;
    }
}

/// Whether the iterable `ident` resolves to a parameter of the enclosing
/// function whose declared type is a generic `IntoIterator` rather than a
/// concrete collection, so it has no cheaply-known `.len()`. Two shapes match:
/// a bare type parameter (`fn new<I>(input: I)` — the type is a
/// `type_identifier` listed in the function's `<...>` generics) and an
/// argument-position `impl Trait` (`fn new(input: impl IntoIterator<..>)` —
/// the type is an `abstract_type`). A parameter typed as a concrete collection
/// (`Vec<T>`, `&[T]`) is neither and keeps its known length. The match is by
/// parameter name; a local `let` rebinding that name, or a closure boundary
/// between the loop and the function, falls back to treating the length as
/// known.
fn iterable_is_generic_param(ident: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(name) = ident.utf8_text(source) else { return false; };

    let mut node = ident;
    let fn_node = loop {
        let Some(parent) = node.parent() else { return false; };
        match parent.kind() {
            "closure_expression" => return false,
            "function_item" => break parent,
            _ => node = parent,
        }
    };

    let Some(params) = fn_node.child_by_field_name("parameters") else { return false; };
    let generics = generic_param_names(fn_node, source);

    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        if param.kind() != "parameter" {
            continue;
        }
        let Some(pattern) = param.child_by_field_name("pattern") else { continue; };
        if extract_var_name(pattern, source) != Some(name) {
            continue;
        }
        let Some(ty) = param.child_by_field_name("type") else { return false; };
        let unsized_iterable = match ty.kind() {
            "abstract_type" => true,
            "type_identifier" => ty
                .utf8_text(source)
                .map(|t| generics.contains(&t))
                .unwrap_or(false),
            _ => false,
        };
        if !unsized_iterable {
            return false;
        }
        // A local `let <name> = ...` shadows the parameter: the iterable is then
        // the local, whose length we don't reason about here, so keep current
        // behavior. Checked only on a confirmed match to avoid the body walk in
        // the common concrete-parameter case.
        if let Some(body) = fn_node.child_by_field_name("body")
            && body_binds_name(body, name, source)
        {
            return false;
        }
        return true;
    }
    false
}

/// The names of the enclosing function's generic type parameters (the
/// `type_identifier` in each `type_parameter` of its `<...>` list). Lifetimes
/// and const params are excluded.
fn generic_param_names<'a>(fn_node: tree_sitter::Node, source: &'a [u8]) -> Vec<&'a str> {
    let Some(type_params) = fn_node.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = type_params.walk();
    type_params
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "type_parameter")
        .filter_map(|child| child.child_by_field_name("name"))
        .filter_map(|name| name.utf8_text(source).ok())
        .collect()
}

/// Whether any `let` declaration in the subtree binds `name`, shadowing a
/// same-named function parameter so the iterable is the local, not the param.
fn body_binds_name(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if node.kind() == "let_declaration"
        && let Some(pattern) = node.child_by_field_name("pattern")
        && extract_var_name(pattern, source) == Some(name)
    {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| body_binds_name(child, name, source))
}

fn is_push_call(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    if node.kind() == "call_expression"
        && let Some(fn_node) = node.child_by_field_name("function")
        && fn_node.kind() == "field_expression"
    {
        let val = fn_node
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        let field = fn_node
            .child_by_field_name("field")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        return val == var && field == "push";
    }
    false
}

/// Whether `node` is a `<var>.extend(...)` or `<var>.append(...)` call. Both add
/// a statically-unknown number of elements, so the Vec's final length stops
/// equalling the iteration count and `with_capacity(n)` would under-allocate.
fn is_extend_or_append_call(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    if node.kind() == "call_expression"
        && let Some(fn_node) = node.child_by_field_name("function")
        && fn_node.kind() == "field_expression"
    {
        let val = fn_node
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        let field = fn_node
            .child_by_field_name("field")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        return val == var && (field == "extend" || field == "append");
    }
    false
}

fn body_directly_pushes(body: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        let call = if child.kind() == "call_expression" {
            child
        } else if child.kind() == "expression_statement" {
            match child.named_child(0) {
                Some(inner) if inner.kind() == "call_expression" => inner,
                _ => continue,
            }
        } else {
            continue;
        };
        if is_push_call(call, var, source) {
            return true;
        }
    }
    false
}

fn body_has_continue(node: tree_sitter::Node) -> bool {
    if node.kind() == "continue_expression" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(body_has_continue)
}

/// Whether the loop body can `break` early anywhere in its subtree. A `break`
/// exits the loop before the iterable is exhausted, so the accumulator's final
/// length is not the iteration count and `with_capacity(n)` would over-allocate.
/// Uses the same whole-subtree walk as `body_has_continue`.
fn body_has_break(node: tree_sitter::Node) -> bool {
    if node.kind() == "break_expression" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(body_has_break)
}

/// Whether the loop body contains any `<var>.extend(...)`/`<var>.append(...)`
/// anywhere — including nested inside an `if`/`if let` — using the same
/// whole-subtree walk as `body_has_continue` so a conditional extend is caught.
fn body_extends_or_appends(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    if is_extend_or_append_call(node, var, source) {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| body_extends_or_appends(child, var, source))
}

/// Whether the loop body reassigns the accumulator (`<var> = ...`) anywhere —
/// including inside an `if`. A reassignment resets the Vec, so its final length
/// no longer equals the iteration count and `with_capacity(n)` would mis-size.
fn body_reassigns(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    if node.kind() == "assignment_expression"
        && let Some(left) = node.child_by_field_name("left")
        && left.utf8_text(source).map(|t| t == var).unwrap_or(false)
    {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| body_reassigns(child, var, source))
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
    fn flags_vec_new_then_push_in_for() {
        let src = "fn f(items: Vec<i32>) {\n    let mut result = Vec::new();\n    for item in items {\n        result.push(item);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_capacity() {
        let src = "fn f(items: Vec<i32>) {\n    let mut result = Vec::with_capacity(items.len());\n    for item in items {\n        result.push(item);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_vec_new_no_for() {
        assert!(run("fn f() {\n    let mut v = Vec::new();\n    v.push(1);\n}").is_empty());
    }

    #[test]
    fn allows_conditional_push_in_if_issue_1024() {
        let src = "fn f(items: Vec<i32>) {\n    let mut v = Vec::new();\n    for x in items {\n        if x > 0 { v.push(x); }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_with_continue_in_body_issue_1024() {
        let src = "fn f(items: Vec<i32>) {\n    let mut ok = Vec::new();\n    for x in items {\n        if x < 0 { continue; }\n        ok.push(x);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_nested_in_double_if_issue_1024() {
        let src = "fn f(items: Vec<Option<i32>>) {\n    let mut names = Vec::new();\n    for x in items {\n        if true {\n            if let Some(v) = x {\n                names.push(v);\n            }\n        }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_unconditional_push_with_unrelated_if() {
        let src = "fn f(items: Vec<i32>) {\n    let mut out = Vec::new();\n    for x in items {\n        if x > 0 { println!(\"{x}\"); }\n        out.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_fallible_iterator_iterable_issue_3983() {
        let src = "fn read<'a>(r: &mut Reader<'a>) -> Result<Vec<CertificateDer<'a>>, InvalidMessage> {\n    let mut ret = Vec::new();\n    for item in TlsListIter::<CertificateDer<'a>>::new(r)? {\n        ret.push(item?);\n    }\n    Ok(ret)\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_iterator_adaptor_chain_iterable() {
        let src = "fn f(v: Vec<i32>) {\n    let mut out = Vec::new();\n    for x in v.iter().filter(|x| **x > 0) {\n        out.push(*x);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_plain_call_iterable() {
        let src = "fn f() {\n    let mut out = Vec::new();\n    for x in make_items() {\n        out.push(x);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_field_iterable() {
        let src = "struct S { items: Vec<i32> }\nimpl S {\n    fn f(&self) {\n        let mut out = Vec::new();\n        for x in self.items {\n            out.push(x);\n        }\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_reference_iterable() {
        let src = "fn f(v: &Vec<i32>) {\n    let mut out = Vec::new();\n    for x in &v {\n        out.push(*x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_push_with_extend_same_vec_issue_3947() {
        let src = "fn f(xs: Vec<i32>, other: Vec<i32>) {\n    let mut v = Vec::new();\n    for x in xs {\n        v.push(x);\n        v.extend(other.clone());\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_with_append_same_vec_issue_3947() {
        let src = "fn f(xs: Vec<i32>) {\n    let mut v = Vec::new();\n    let mut more = vec![1];\n    for x in xs {\n        v.push(x);\n        v.append(&mut more);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_push_with_conditional_extend_same_vec_issue_3947() {
        let src = "fn f(summaries: Vec<S>) {\n    let mut ids = Vec::new();\n    for summary in summaries {\n        ids.push(summary.package_id());\n        if let Some(lock) = summary.lock {\n            ids.extend(lock.alt);\n        }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_extend_on_different_var_issue_3947() {
        let src = "fn f(xs: Vec<i32>, z: Vec<i32>) {\n    let mut v = Vec::new();\n    let mut other = Vec::new();\n    for x in xs {\n        v.push(x);\n        other.extend(z.clone());\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_group_by_comma_reassigned_accumulator_issue_3792() {
        let src = "fn f<T>(items: Vec<T>, is_comma: impl Fn(&T) -> bool) -> Vec<Vec<T>> {\n    let mut groups: Vec<Vec<T>> = Vec::new();\n    let mut current_group: Vec<T> = Vec::new();\n    for element in items {\n        let comma = is_comma(&element);\n        current_group.push(element);\n        if comma {\n            groups.push(current_group);\n            current_group = Vec::new();\n        }\n    }\n    groups\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_minimal_reassigned_accumulator_issue_3792() {
        let src = "fn f(items: Vec<i32>) {\n    let mut v = Vec::new();\n    for x in items {\n        v.push(x);\n        v = Vec::new();\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_assignment_to_different_var_issue_3792() {
        let src = "fn f(items: Vec<i32>) {\n    let mut v = Vec::new();\n    let mut n = 0;\n    for x in items {\n        v.push(x);\n        n = n + 1;\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_generic_into_iterator_param_issue_6554() {
        let src = "fn new<I, S>(input: I) -> Result<()>\nwhere\n    I: IntoIterator<Item = S>,\n    S: AsRef<str>,\n{\n    let mut args = Vec::new();\n    for arg in input {\n        args.push(arg);\n    }\n    Ok(())\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_impl_into_iterator_param_issue_6554() {
        let src = "fn new(input: impl IntoIterator<Item = u32>) {\n    let mut args = Vec::new();\n    for arg in input {\n        args.push(arg);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_concrete_vec_param_in_generic_fn() {
        let src = "fn f<T>(items: Vec<T>) {\n    let mut v = Vec::new();\n    for x in items {\n        v.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_concrete_slice_param_in_generic_fn() {
        let src = "fn g<T>(items: &[T]) {\n    let mut v = Vec::new();\n    for x in items {\n        v.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_push_with_conditional_break_in_body_issue_7209() {
        let src = "fn f(statuses: Vec<i32>, check_dirty: bool) {\n    let mut changes = Vec::new();\n    for change in statuses {\n        changes.push(change);\n        if check_dirty {\n            break;\n        }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_plain_push_loop_no_break_no_continue_issue_7209() {
        let src = "fn f(items: Vec<i32>) {\n    let mut v = Vec::new();\n    for x in items {\n        v.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_local_shadows_generic_param() {
        let src = "fn f<I>(input: I) {\n    let input = vec![1, 2, 3];\n    let mut v = Vec::new();\n    for x in input {\n        v.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_local_kmerge_adaptor_binding_issue_7640() {
        let src = "fn f(entry_groups: Vec<Vec<E>>) {\n    let merged = entry_groups.into_iter().kmerge_by(|a, b| a < b);\n    let mut current: Vec<E> = Vec::new();\n    for e in merged {\n        current.push(e);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_local_map_adaptor_binding_issue_7640() {
        let src = "fn f(xs: Vec<i32>) {\n    let it = xs.iter().map(|x| x + 1);\n    let mut out = Vec::new();\n    for x in it {\n        out.push(x);\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_local_try_expression_binding_issue_7640() {
        let src = "fn f(r: &mut R) -> Result<(), E> {\n    let it = Iter::new(r)?;\n    let mut out = Vec::new();\n    for x in it {\n        out.push(x);\n    }\n    Ok(())\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_local_concrete_vec_binding_issue_7640() {
        let src = "fn f() {\n    let v: Vec<i32> = build();\n    let mut out = Vec::new();\n    for x in v {\n        out.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_local_vec_macro_binding_issue_7640() {
        let src = "fn f() {\n    let v = vec![1, 2, 3];\n    let mut out = Vec::new();\n    for x in v {\n        out.push(x);\n    }\n}";
        assert_eq!(run(src).len(), 1);
    }
}
