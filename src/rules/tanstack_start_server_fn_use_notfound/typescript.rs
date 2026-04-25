//! Flag `throw new Error('...not found...')` inside a `createServerFn(...)`
//! callback. Outside server-fn scope the signal is not specific enough to
//! warrant a diagnostic.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "throw_statement" { return; }
    let Some(expr) = node.named_child(0) else { return; };
    if expr.kind() != "new_expression" { return; }
    let Some(ctor) = expr.child_by_field_name("constructor") else { return; };
    let Ok(ctor_name) = ctor.utf8_text(source) else { return; };
    if ctor_name != "Error" { return; }

    let Some(args) = expr.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let has_notfound_msg = args.children(&mut cursor).any(|c| {
        if !matches!(c.kind(), "string" | "template_string") { return false; }
        c.utf8_text(source)
            .ok()
            .map(|s| s.to_ascii_lowercase().contains("not found"))
            .unwrap_or(false)
    });
    if !has_notfound_msg { return; }
    if !is_inside_create_server_fn(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Throw `notFound()` instead of `new Error('...not found...')` so the \
         router can render the 404 boundary."
            .into(),
        Severity::Warning,
    ));
}

/// Walk up ancestors looking for an enclosing `createServerFn(...).handler(cb)`
/// or `createServerFn(...)(cb)` callback. We accept the throw if any ancestor
/// chain starts at a `createServerFn` call.
fn is_inside_create_server_fn(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if matches!(
            p.kind(),
            "arrow_function" | "function_expression" | "function" | "method_definition"
        ) && callback_belongs_to_create_server_fn(p, source) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// True when `func_node` is an argument (directly or via member-call chain)
/// of a `createServerFn(...)` call expression.
fn callback_belongs_to_create_server_fn(func_node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    // Walk up from the function until we exit the call_expression that owns
    // it; check whether any function in the call chain is `createServerFn`.
    let mut cur = func_node.parent();
    while let Some(p) = cur {
        if p.kind() == "call_expression"
            && call_chain_root_is_create_server_fn(p, source)
        {
            return true;
        }
        // Don't bail — outer chained calls may still be createServerFn.
        // Stop at function/program boundaries above the immediate call chain.
        if matches!(
            p.kind(),
            "function_declaration" | "program" | "statement_block"
        ) {
            return false;
        }
        cur = p.parent();
    }
    false
}

fn call_chain_root_is_create_server_fn(call: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cur = call;
    loop {
        let Some(callee) = cur.child_by_field_name("function") else { return false; };
        match callee.kind() {
            "identifier" => {
                return callee.utf8_text(source).map(|t| t == "createServerFn").unwrap_or(false);
            }
            "member_expression" => {
                let Some(obj) = callee.child_by_field_name("object") else { return false; };
                if obj.kind() == "call_expression" {
                    cur = obj;
                    continue;
                }
                if obj.kind() == "identifier" {
                    return obj
                        .utf8_text(source)
                        .map(|t| t == "createServerFn")
                        .unwrap_or(false);
                }
                return false;
            }
            "call_expression" => {
                cur = callee;
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_not_found_error_in_server_fn() {
        let src = "const fn = createServerFn().handler(async () => { \
                   if (!user) { throw new Error('user not found'); } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_case_insensitive_in_server_fn() {
        let src = "const fn = createServerFn()(async () => { throw new Error('Not Found'); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_notfound_helper_in_server_fn() {
        let src = "const fn = createServerFn().handler(async () => { throw notFound(); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_error_in_server_fn() {
        let src = "const fn = createServerFn().handler(async () => { throw new Error('permission denied'); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_not_found_outside_server_fn() {
        // Outside a createServerFn callback we don't have enough signal.
        assert!(run("if (!user) { throw new Error('user not found'); }").is_empty());
    }
}
