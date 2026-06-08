//! tanstack-query-prefer-suspense-query backend.
//!
//! Detection pattern:
//!
//!   const { isPending | isLoading, data, … } = useQuery(…);
//!   if (isPending | isLoading) return <Spinner />;
//!
//! When that shape appears inside the same function body as the
//! `useQuery` call, flag it and suggest migrating to
//! `useSuspenseQuery`.
//!
//! We also handle the non-destructured form:
//!
//!   const q = useQuery(…);
//!   if (q.isPending) return null;
//!
//! False-positive guards:
//! - `useInfiniteQuery` is NOT flagged — it has no suspense equivalent
//!   yet (use `useSuspenseInfiniteQuery` exists but semantics differ;
//!   we'd need additional checks).
//! - If the early-return branch ALSO checks `error`/`isError` we still
//!   suggest suspense because Suspense + ErrorBoundary is the canonical
//!   replacement.

use crate::diagnostic::{Diagnostic, Severity};

const PENDING_FLAGS: &[&str] = &["isPending", "isLoading"];

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    // Anchor on the variable_declarator `<pattern> = useQuery(…)`.
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "call_expression" { return; }
    let Some(callee) = value.child_by_field_name("function") else { return; };
    let Ok(callee_text) = callee.utf8_text(source) else { return; };
    if callee_text != "useQuery" { return; }

    let Some(pattern) = node.child_by_field_name("name") else { return; };

    // Figure out the name we'll scan for. Two shapes:
    //   - `const { isPending, ... } = useQuery()` → scan for `isPending`
    //     as a bare identifier reference.
    //   - `const q = useQuery()` → scan for `q.isPending` member access.
    let mut pending_names: Vec<String> = Vec::new();
    let mut bound_object_name: Option<String> = None;
    match pattern.kind() {
        "object_pattern" => {
            let mut cursor = pattern.walk();
            for child in pattern.children(&mut cursor) {
                let Some(text) = destructured_property_name(child, source) else { continue; };
                if PENDING_FLAGS.contains(&text.as_str()) {
                    pending_names.push(text);
                }
            }
            if pending_names.is_empty() { return; }
        }
        "identifier" => {
            let Ok(name) = pattern.utf8_text(source) else { return; };
            bound_object_name = Some(name.to_string());
        }
        _ => return,
    }

    // Find the enclosing function body — scan for the guard there.
    let Some(fn_body) = enclosing_function_body(node) else { return; };

    let guarded = if let Some(obj_name) = bound_object_name {
        has_member_guard(fn_body, source, &obj_name)
    } else {
        has_identifier_guard(fn_body, source, &pending_names)
    };

    if !guarded { return; }

    let pos = value.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "tanstack-query-prefer-suspense-query".into(),
        message: "`useQuery` with an `if (isPending|isLoading) return …` guard \
                  should use `useSuspenseQuery` and a `<Suspense>` boundary — \
                  `data` will be guaranteed defined.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// For each child of an object destructuring pattern, extract the name
/// that will be bound in scope. Handles shorthand, renamed, and default
/// forms:
///   `{ foo }`        → "foo"
///   `{ foo: bar }`   → "bar"
///   `{ foo = 1 }`    → "foo"
fn destructured_property_name(child: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match child.kind() {
        "shorthand_property_identifier_pattern" => child.utf8_text(source).ok().map(String::from),
        "pair_pattern" => {
            // Renamed: `{ key: binding }` — we want `key` because that is
            // the property name exposed by useQuery (isPending). The
            // binding name may differ but we don't track that here.
            let key = child.child_by_field_name("key")?;
            key.utf8_text(source).ok().map(String::from)
        }
        "object_assignment_pattern" => {
            // `{ foo = 1 }` — wraps a shorthand or pair pattern.
            let left = child.child_by_field_name("left")?;
            destructured_property_name(left, source)
        }
        _ => None,
    }
}

/// Walk up to the nearest enclosing function's body (`statement_block`
/// or the expression body of an arrow function). Returns None if none
/// found.
fn enclosing_function_body(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if matches!(
            parent.kind(),
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
                | "generator_function"
                | "generator_function_declaration"
        ) {
            return parent.child_by_field_name("body");
        }
        current = parent;
    }
    None
}

/// True if `body` contains an `if (<one of names>) return …;` guard, or
/// `if (!<one of names>) return …;`, directly in its statement list.
fn has_identifier_guard(body: tree_sitter::Node, source: &[u8], names: &[String]) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "if_statement" {
            continue;
        }
        let Some(cond) = child.child_by_field_name("condition") else {
            continue;
        };
        if !condition_mentions_any(cond, source, |ident| names.iter().any(|n| n == ident)) {
            continue;
        }
        if consequent_has_return(child) {
            return true;
        }
    }
    false
}

/// True if `body` contains an `if (<obj>.isPending|isLoading) return …;`
/// guard on the bound variable.
fn has_member_guard(body: tree_sitter::Node, source: &[u8], obj_name: &str) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "if_statement" {
            continue;
        }
        let Some(cond) = child.child_by_field_name("condition") else {
            continue;
        };
        if !condition_has_pending_member(cond, source, obj_name) {
            continue;
        }
        if consequent_has_return(child) {
            return true;
        }
    }
    false
}

/// Scan the if-condition (including `parenthesized_expression` and unary
/// `!x`) for any identifier passing `pred`.
fn condition_mentions_any<F>(cond: tree_sitter::Node, source: &[u8], pred: F) -> bool
where
    F: Fn(&str) -> bool,
{
    let mut found = false;
    walk_subtree(cond, &mut |n| {
        if found {
            return;
        }
        if n.kind() != "identifier" {
            return;
        }
        let Some(parent) = n.parent() else {
            return;
        };
        // Skip the property side of `a.b`.
        if parent.kind() == "member_expression"
            && parent
                .child_by_field_name("property")
                .is_some_and(|p| p == n)
        {
            return;
        }
        if let Ok(text) = n.utf8_text(source)
            && pred(text)
        {
            found = true;
        }
    });
    found
}

/// Scan the if-condition for a `<obj>.isPending` or `<obj>.isLoading`
/// member access.
fn condition_has_pending_member(cond: tree_sitter::Node, source: &[u8], obj_name: &str) -> bool {
    let mut found = false;
    walk_subtree(cond, &mut |n| {
        if found {
            return;
        }
        if n.kind() != "member_expression" {
            return;
        }
        let Some(obj) = n.child_by_field_name("object") else {
            return;
        };
        let Some(prop) = n.child_by_field_name("property") else {
            return;
        };
        let Ok(obj_text) = obj.utf8_text(source) else {
            return;
        };
        let Ok(prop_text) = prop.utf8_text(source) else {
            return;
        };
        if obj_text == obj_name && PENDING_FLAGS.contains(&prop_text) {
            found = true;
        }
    });
    found
}

/// True when the consequent branch of an `if_statement` contains a
/// `return …` statement (either directly or as the sole statement of a
/// block).
fn consequent_has_return(if_node: tree_sitter::Node) -> bool {
    let Some(cons) = if_node.child_by_field_name("consequence") else {
        return false;
    };
    let mut found = false;
    walk_subtree(cons, &mut |n| {
        if n.kind() == "return_statement" {
            found = true;
        }
    });
    found
}

/// Iterative subtree walker — mirrors `walker::walk_tree` but bounded to
/// a subtree rooted at `root`.
fn walk_subtree<F>(root: tree_sitter::Node, visit: &mut F)
where
    F: FnMut(tree_sitter::Node),
{
    let root_id = root.id();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            if !goto_next(&mut cursor, root_id) {
                return;
            }
            continue;
        }
        visit(node);
        if cursor.goto_first_child() {
            continue;
        }
        if !goto_next(&mut cursor, root_id) {
            return;
        }
    }
}

fn goto_next(cursor: &mut tree_sitter::TreeCursor, root_id: usize) -> bool {
    loop {
        if cursor.node().id() == root_id {
            return false;
        }
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_destructured_is_pending_with_early_return() {
        let diags = run_on(
            "function C() {
                const { isPending, data } = useQuery({ queryKey: ['x'], queryFn: f });
                if (isPending) return null;
                return data;
            }",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn flags_destructured_is_loading() {
        let diags = run_on(
            "function C() {
                const { isLoading, data } = useQuery({ queryKey: ['x'], queryFn: f });
                if (isLoading) return null;
                return data;
            }",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn flags_non_destructured_member_access() {
        let diags = run_on(
            "function C() {
                const q = useQuery({ queryKey: ['x'], queryFn: f });
                if (q.isPending) return null;
                return q.data;
            }",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn allows_use_suspense_query() {
        let diags = run_on(
            "function C() {
                const { data } = useSuspenseQuery({ queryKey: ['x'], queryFn: f });
                return data;
            }",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn allows_use_query_without_early_return_guard() {
        let diags = run_on(
            "function C() {
                const { isPending, data } = useQuery({ queryKey: ['x'], queryFn: f });
                return isPending ? null : data;
            }",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_use_infinite_query() {
        let diags = run_on(
            "function C() {
                const { isPending, data } = useInfiniteQuery({ queryKey: ['x'], queryFn: f });
                if (isPending) return null;
                return data;
            }",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn allows_use_query_without_pending_destructure() {
        let diags = run_on(
            "function C() {
                const { data } = useQuery({ queryKey: ['x'], queryFn: f });
                return data ?? null;
            }",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }
}
