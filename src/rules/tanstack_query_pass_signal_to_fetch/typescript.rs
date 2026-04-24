//! tanstack-query-pass-signal-to-fetch backend.
//!
//! Detects a `queryFn: ({ signal }) => ...` arrow whose body calls
//! `fetch(...)` without forwarding the `signal`. TanStack Query gives
//! each query an `AbortSignal`; dropping it means cancelled queries
//! still hit the network.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Anchor on the `queryFn: <arrow>` pair.
    let Some((key, _)) = crate::rules::object_literal::object_pair(node, source) else {
        return;
    };
    if key != "queryFn" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "arrow_function" { return; }

    if !destructures_signal(value, source) { return; }

    let Some(body) = value.child_by_field_name("body") else { return; };
    if !has_bad_fetch(body, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &value,
        super::META.id,
        "`queryFn` destructures `{ signal }` but does not pass it to `fetch`. \
         Forward it: `fetch(url, { signal })` so cancellation aborts the request.".into(),
        Severity::Warning,
    ));
}

/// True when the arrow's first parameter is an object pattern binding
/// a `signal` property.
fn destructures_signal(arrow: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(params) = arrow.child_by_field_name("parameters") else { return false; };
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        let target = if param.kind() == "required_parameter" || param.kind() == "optional_parameter" {
            param.child_by_field_name("pattern").unwrap_or(param)
        } else {
            param
        };
        if target.kind() != "object_pattern" { continue; }
        let mut c2 = target.walk();
        for field in target.named_children(&mut c2) {
            let name = match field.kind() {
                "shorthand_property_identifier_pattern" => field.utf8_text(source).ok(),
                "pair_pattern" => field
                    .child_by_field_name("key")
                    .and_then(|k| k.utf8_text(source).ok()),
                _ => None,
            };
            if name == Some("signal") { return true; }
        }
    }
    false
}

/// True when `body` contains a `fetch(...)` call that either has no
/// options argument, or has one without a `signal` key.
fn has_bad_fetch(body: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut found = false;
    walk_subtree(body, &mut |n| {
        if found { return; }
        if n.kind() != "call_expression" { return; }
        let Some(func) = n.child_by_field_name("function") else { return; };
        if func.utf8_text(source).ok() != Some("fetch") { return; }
        let Some(args) = n.child_by_field_name("arguments") else { return; };
        let opts = args.named_child(1);
        match opts {
            None => { found = true; }
            Some(opt) if opt.kind() == "object" => {
                let mut c = opt.walk();
                let mut has_signal = false;
                for child in opt.named_children(&mut c) {
                    match child.kind() {
                        "pair" => {
                            let Some(k) = child.child_by_field_name("key") else { continue; };
                            let Ok(raw) = k.utf8_text(source) else { continue; };
                            if raw.trim_matches(|c| c == '"' || c == '\'') == "signal" {
                                has_signal = true;
                                break;
                            }
                        }
                        "shorthand_property_identifier" => {
                            if child.utf8_text(source).ok() == Some("signal") {
                                has_signal = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if !has_signal { found = true; }
            }
            Some(_) => {
                // Spread / variable — we can't be sure; don't flag to avoid FPs.
            }
        }
    });
    found
}

fn walk_subtree<F: FnMut(tree_sitter::Node<'_>)>(root: tree_sitter::Node<'_>, visit: &mut F) {
    let root_id = root.id();
    let mut cursor = root.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() { continue; }
        loop {
            if cursor.node().id() == root_id { return; }
            if cursor.goto_next_sibling() { break; }
            if !cursor.goto_parent() { return; }
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
    fn flags_fetch_without_signal() {
        let src = "useQuery({ queryKey: ['x'], queryFn: ({ signal }) => fetch('/api') });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_fetch_options_without_signal() {
        let src = "useQuery({ queryKey: ['x'], queryFn: ({ signal }) => fetch('/api', { method: 'GET' }) });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_fetch_with_signal() {
        let src = "useQuery({ queryKey: ['x'], queryFn: ({ signal }) => fetch('/api', { signal }) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_query_fn_without_signal_destructure() {
        let src = "useQuery({ queryKey: ['x'], queryFn: () => fetch('/api') });";
        assert!(run(src).is_empty());
    }
}
