//! tanstack-query-pass-signal-to-fetch backend.
//!
//! Detects a `queryFn: ({ signal }) => ...` arrow whose body calls
//! `fetch(...)` without forwarding the `signal`. TanStack Query gives
//! each query an `AbortSignal`; dropping it means cancelled queries
//! still hit the network.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["queryFn"] => |node, source, ctx, diagnostics|
    // Anchor on the `queryFn: <arrow>` pair.
    let Some((key, _)) = crate::rules::object_literal::object_pair(node, source) else {
        return;
    };
    if key != "queryFn" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "arrow_function" { return; }

    let Some(body) = value.child_by_field_name("body") else { return; };

    if destructures_signal(value, source) {
        // Destructured `{ signal }` but never forwarded to fetch.
        if !has_bad_fetch(body, source) { return; }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &value,
            super::META.id,
            "`queryFn` destructures `{ signal }` but does not pass it to `fetch`. \
             Forward it: `fetch(url, { signal })` so cancellation aborts the request.".into(),
            Severity::Warning,
        ));
    } else if let Some(param_name) = single_identifier_param(value, source) {
        // Context parameter (e.g. `ctx`) — check fetch passes `<param>.signal`.
        if !has_bad_fetch_for_ctx(body, source, param_name) { return; }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &value,
            super::META.id,
            "`queryFn` receives the query context but does not pass its `signal` to `fetch`. \
             Forward it: `fetch(url, { signal: ctx.signal })` so cancellation aborts the request.".into(),
            Severity::Warning,
        ));
    }
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

/// If the arrow has exactly one parameter that is a plain identifier
/// (not destructured), return its name.
fn single_identifier_param<'a>(arrow: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let params = arrow.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    let named: Vec<_> = params.named_children(&mut cursor).collect();
    if named.len() != 1 { return None; }
    let param = named[0];
    let target = if param.kind() == "required_parameter" || param.kind() == "optional_parameter" {
        param.child_by_field_name("pattern").unwrap_or(param)
    } else {
        param
    };
    if target.kind() == "identifier" {
        return target.utf8_text(source).ok();
    }
    None
}

/// Like `has_bad_fetch`, but for the context-parameter form.
/// True when `body` contains a `fetch(...)` call whose options don't
/// contain a `signal` key referencing `<ctx_name>.signal`.
fn has_bad_fetch_for_ctx(body: tree_sitter::Node<'_>, source: &[u8], ctx_name: &str) -> bool {
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
                    if child.kind() == "pair" {
                        let Some(k) = child.child_by_field_name("key") else { continue; };
                        let Ok(raw) = k.utf8_text(source) else { continue; };
                        if raw.trim_matches(|c| c == '"' || c == '\'') == "signal" {
                            // Check value references ctx_name.signal
                            let Some(v) = child.child_by_field_name("value") else { continue; };
                            let Ok(val) = v.utf8_text(source) else { continue; };
                            let expected = format!("{}.signal", ctx_name);
                            if val == expected {
                                has_signal = true;
                                break;
                            }
                        }
                    }
                }
                if !has_signal { found = true; }
            }
            Some(_) => {
                // Spread / variable — can't be sure; don't flag.
            }
        }
    });
    found
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

    #[test]
    fn flags_ctx_param_fetch_without_signal() {
        let src = "useQuery({ queryKey: ['user'], queryFn: (ctx) => fetch('/api/user') });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ctx_param_fetch_with_ctx_signal() {
        let src = "useQuery({ queryKey: ['user'], queryFn: (ctx) => fetch('/api/user', { signal: ctx.signal }) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_ctx_param_fetch_options_without_signal() {
        let src = "useQuery({ queryKey: ['user'], queryFn: (context) => fetch('/api/user', { method: 'GET' }) });";
        assert_eq!(run(src).len(), 1);
    }
}
