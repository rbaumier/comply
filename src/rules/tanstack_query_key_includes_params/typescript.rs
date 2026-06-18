//! tanstack-query-key-includes-params backend.
//!
//! Detection: on every `pair` whose key is `queryFn` and that lives
//! inside a query-hook / `queryOptions` call, collect the identifiers
//! referenced in the function body that are NOT bound by the function's
//! own parameter list and NOT well-known globals. Every such identifier
//! must also appear as an identifier anywhere inside the sibling
//! `queryKey` array — otherwise queries silently share cache slots.
//!
//! Heuristic limits (pragmatic, not a full scope resolver):
//! - We only look at the immediate queryFn function (arrow / function
//!   expression). Deeper nested closures still have their identifiers
//!   collected.
//! - We treat object-property keys (`obj.foo`) as NOT a reference to a
//!   free variable `foo` — only the object root counts.
//! - Built-in globals (fetch, console, Math, JSON, window, document,
//!   Promise, Object, Array, Number, String, Boolean, undefined, null,
//!   this, globalThis, localStorage, sessionStorage) are ignored.
//! - Identifiers that start with an uppercase letter (components, types,
//!   enums, imported API clients) are ignored: they're typically stable
//!   module-scope bindings, not request parameters.

use rustc_hash::FxHashSet;
use crate::diagnostic::{Diagnostic, Severity};

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
    "queryOptions",
    "infiniteQueryOptions",
];

const IGNORED_GLOBALS: &[&str] = &[
    "fetch",
    "console",
    "Math",
    "JSON",
    "window",
    "document",
    "Promise",
    "Object",
    "Array",
    "Number",
    "String",
    "Boolean",
    "Date",
    "Error",
    "Symbol",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "RegExp",
    "undefined",
    "null",
    "true",
    "false",
    "this",
    "globalThis",
    "localStorage",
    "sessionStorage",
    "URL",
    "URLSearchParams",
    "FormData",
    "Headers",
    "Request",
    "Response",
    "AbortController",
    "AbortSignal",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "NaN",
    "Infinity",
];

/// Iterative subtree walker — visits every node under `root` (inclusive).
/// We can't reuse `walker::walk_tree` because that takes a full `Tree`,
/// not an arbitrary node. Implementation mirrors the iterative walker
/// in `walker.rs`, scoped to stop once we walk back above `root`.
fn walk_subtree<F>(root: tree_sitter::Node, visit: &mut F)
where
    F: FnMut(tree_sitter::Node),
{
    let root_id = root.id();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            // Skip this subtree entirely.
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

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    // Key must be queryFn.
    let Some(key_node) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key_node.utf8_text(source) else { return; };
    let key_unquoted = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_unquoted != "queryFn" { return; }

    // Must live inside a query-hook / queryOptions call.
    if !inside_query_hook(node, source) { return; }

    // The value should be a function-like node. Accept arrow_function,
    // function_expression, function_declaration. Anything else (bare
    // identifier referencing a factory, etc.) we skip — we can't analyze
    // a body we don't see.
    let Some(value_node) = node.child_by_field_name("value") else { return; };
    let fn_node = match value_node.kind() {
        "arrow_function" | "function_expression" | "function" | "function_declaration"
            | "generator_function" | "generator_function_declaration" => value_node,
        _ => return,
    };

    // Collect the function's own parameter names — these are bound,
    // not free.
    let mut param_names: Vec<String> = Vec::new();
    if let Some(params) = fn_node.child_by_field_name("parameters") {
        collect_binding_identifiers(params, source, &mut param_names);
    } else if let Some(param) = fn_node.child_by_field_name("parameter") {
        // Arrow with a single unparenthesized param: `x => …`.
        collect_binding_identifiers(param, source, &mut param_names);
    }

    // Collect identifiers referenced inside the function body that are NOT
    // the function's params, NOT ignored globals, NOT PascalCase, NOT
    // locally declared inside the body.
    let Some(body_node) = fn_node.child_by_field_name("body") else { return; };
    let mut local_decls: Vec<String> = Vec::new();
    collect_local_declarations(body_node, source, &mut local_decls);

    let mut free_refs: Vec<String> = Vec::new();
    collect_free_references(body_node, source, &mut free_refs);

    let bound: FxHashSet<&str> = param_names
        .iter()
        .chain(local_decls.iter())
        .map(String::as_str)
        .collect();

    // Module-level imports and `const` declarations are stable across
    // every call of the queryFn — they can't cause cache-key collisions.
    // Common shape: `import { api } from "./client"; useQuery({ queryFn:
    // () => api.foo() })`.
    let module_bindings = collect_module_scope_bindings(node, source);

    let mut needed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for name in &free_refs {
        if bound.contains(name.as_str()) { continue; }
        if IGNORED_GLOBALS.contains(&name.as_str()) { continue; }
        // Skip PascalCase (imported types, classes, API clients).
        if name.chars().next().is_some_and(char::is_uppercase) { continue; }
        if module_bindings.contains(name) { continue; }
        needed.insert(name.clone());
    }
    if needed.is_empty() { return; }

    // Find the sibling queryKey pair and the identifiers named inside
    // its array.
    let Some(parent_obj) = node.parent() else { return; };
    let mut key_idents: FxHashSet<String> = FxHashSet::default();
    let mut saw_query_key = false;
    let mut cursor = parent_obj.walk();
    for child in parent_obj.children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        let Some(k) = child.child_by_field_name("key") else { continue; };
        let Ok(k_text) = k.utf8_text(source) else { continue; };
        if k_text.trim_matches(|c| c == '"' || c == '\'') != "queryKey" { continue; }
        saw_query_key = true;
        let Some(v) = child.child_by_field_name("value") else { continue; };
        collect_all_identifiers(v, source, &mut key_idents);
    }
    // If there's no queryKey at all, another rule (tanstack-query-array-key)
    // covers that — don't pile on.
    if !saw_query_key { return; }

    let missing: Vec<&String> = needed.iter().filter(|n| !key_idents.contains(*n)).collect();
    if missing.is_empty() { return; }

    let pos = value_node.start_position();
    let list = missing
        .iter()
        .map(|s| format!("`{s}`"))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "tanstack-query-key-includes-params".into(),
        message: format!(
            "`queryFn` references {list} but `queryKey` does not include it — \
             different values will collide on the same cache slot. Add the \
             identifier(s) to the `queryKey` array."
        ),
        severity: Severity::Error,
        span: None,
    });
}

/// Walk up the tree and return true if `node` is inside a call to a known
/// query hook or `queryOptions` factory.
fn inside_query_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression"
            && let Some(function) = parent.child_by_field_name("function")
        {
            let name = function_callee_name(function, source);
            if let Some(name) = name
                && QUERY_HOOKS.contains(&name.as_str())
            {
                return true;
            }
        }
        current = parent;
    }
    false
}

/// Extract the rightmost identifier from a callee expression.
/// - `useQuery` → "useQuery"
/// - `tsq.useQuery` → "useQuery"
/// - `x.y.useQuery` → "useQuery"
fn function_callee_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "property_identifier" => node.utf8_text(source).ok().map(String::from),
        "member_expression" => {
            let prop = node.child_by_field_name("property")?;
            prop.utf8_text(source).ok().map(String::from)
        }
        _ => None,
    }
}

/// Walk the parameter list and collect every identifier that names a
/// bound parameter. Handles destructuring patterns recursively.
fn collect_binding_identifiers(node: tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    walk_subtree(node, &mut |n| {
        // `required_parameter`, `optional_parameter`, `identifier` inside
        // a pattern. We collect every `identifier` that is a pattern leaf.
        if n.kind() == "identifier" {
            // Skip identifiers that are property keys in destructuring
            // without rename (e.g. `{ a }` binds `a`, `{ a: b }` binds `b`).
            if let Some(parent) = n.parent()
                && parent.kind() == "pair_pattern"
                && parent.child_by_field_name("key").is_some_and(|k| k == n)
            {
                return;
            }
            if let Ok(text) = n.utf8_text(source) {
                out.push(text.to_string());
            }
        } else if n.kind() == "shorthand_property_identifier_pattern"
            && let Ok(text) = n.utf8_text(source)
        {
            out.push(text.to_string());
        }
    });
}

/// Collect identifiers declared with `const` / `let` / `var` / `function` /
/// `class` / `import` inside the function body. Treated as locally bound.
fn collect_local_declarations(node: tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    walk_subtree(node, &mut |n| match n.kind() {
        "variable_declarator" => {
            if let Some(name) = n.child_by_field_name("name") {
                collect_binding_identifiers(name, source, out);
            }
        }
        "function_declaration" | "generator_function_declaration" | "class_declaration" => {
            if let Some(name) = n.child_by_field_name("name")
                && let Ok(text) = name.utf8_text(source)
            {
                out.push(text.to_string());
            }
        }
        _ => {}
    });
}

/// Collect every identifier that looks like a VALUE reference (not a
/// property key, not a property access on something else, not a type
/// annotation). Best-effort — used for the free-variable set.
fn collect_free_references(node: tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    walk_subtree(node, &mut |n| {
        if n.kind() != "identifier" {
            return;
        }
        let Some(parent) = n.parent() else {
            return;
        };
        match parent.kind() {
            // `obj.foo` → only `obj` counts; skip the `foo` property ident.
            // tree-sitter-typescript uses `property_identifier` for .foo,
            // but we only see `identifier` here. `member_expression` with
            // `property` field pointing to this node shouldn't happen
            // (property is its own kind), but guard anyway.
            "member_expression" => {
                if parent
                    .child_by_field_name("property")
                    .is_some_and(|p| p == n)
                {
                    return;
                }
            }
            // `{ foo: bar }` — `foo` is a key, not a reference.
            "pair" => {
                if parent.child_by_field_name("key").is_some_and(|k| k == n) {
                    return;
                }
            }
            // TS type annotations / type references — skip.
            "type_annotation" | "type_identifier" | "predefined_type" | "type_reference"
            | "generic_type" => return,
            // Property key in destructuring patterns — skip.
            "pair_pattern" => {
                if parent.child_by_field_name("key").is_some_and(|k| k == n) {
                    return;
                }
            }
            // Parameter name declaration inside a nested function — skip;
            // those are bindings, not references.
            "formal_parameters" | "required_parameter" | "optional_parameter" | "rest_pattern" => {
                return;
            }
            // Callee of a call expression — imported/module-scope
            // functions don't vary per-render, so they aren't cache
            // inputs. `fetchUser(userId)` should flag only `userId`.
            "call_expression" => {
                if parent
                    .child_by_field_name("function")
                    .is_some_and(|f| f == n)
                {
                    return;
                }
            }
            _ => {}
        }
        if let Ok(text) = n.utf8_text(source) {
            out.push(text.to_string());
        }
    });
}

/// Collect every identifier text appearing anywhere under `node`.
/// Used to harvest queryKey contents — we want ANY identifier that
/// appears inside the array, treating each as potentially covering the
/// closure variable. Includes object-property values, template-string
/// interpolations, etc.
fn collect_all_identifiers(
    node: tree_sitter::Node,
    source: &[u8],
    out: &mut FxHashSet<String>,
) {
    walk_subtree(node, &mut |n| {
        if n.kind() == "identifier"
            && let Ok(text) = n.utf8_text(source)
        {
            out.insert(text.to_string());
        }
    });
}

/// Collect identifier names bound at the file's module scope:
/// - `import { foo, bar as baz } from "..."` → `foo`, `baz`
/// - `import foo from "..."` → `foo`
/// - `import * as ns from "..."` → `ns`
/// - top-level `const foo = ...` / `let foo = ...` / `var foo = ...`
/// - top-level `function foo() {}` / `class Foo {}` / `type Foo = ...`
///
/// These are stable across every call of the queryFn and cannot vary
/// between cache-key resolutions, so omitting them from queryKey can
/// never produce a collision.
fn collect_module_scope_bindings(
    node: tree_sitter::Node,
    source: &[u8],
) -> FxHashSet<String> {
    // Walk up to find the program root.
    let mut current = node;
    while let Some(parent) = current.parent() {
        current = parent;
    }
    let program = current;

    let mut out: FxHashSet<String> = FxHashSet::default();
    let mut cursor = program.walk();
    for child in program.named_children(&mut cursor) {
        match child.kind() {
            "import_statement" => {
                let mut inner = child.walk();
                for c in child.named_children(&mut inner) {
                    collect_binding_identifiers(c, source, &mut Vec::new());
                    walk_subtree(c, &mut |n| match n.kind() {
                        "identifier" | "type_identifier" => {
                            if let Ok(text) = n.utf8_text(source) {
                                out.insert(text.to_string());
                            }
                        }
                        _ => {}
                    });
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut inner = child.walk();
                for decl in child.named_children(&mut inner) {
                    if decl.kind() != "variable_declarator" {
                        continue;
                    }
                    if let Some(id) = decl.child_by_field_name("name") {
                        let mut buf: Vec<String> = Vec::new();
                        collect_binding_identifiers(id, source, &mut buf);
                        out.extend(buf);
                    }
                }
            }
            "function_declaration"
            | "generator_function_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "type_alias_declaration"
            | "interface_declaration"
            | "enum_declaration" => {
                if let Some(name) = child.child_by_field_name("name")
                    && let Ok(text) = name.utf8_text(source)
                {
                    out.insert(text.to_string());
                }
            }
            "export_statement" => {
                let mut inner = child.walk();
                for c in child.named_children(&mut inner) {
                    walk_subtree(c, &mut |n| match n.kind() {
                        "identifier" => {
                            if let Ok(text) = n.utf8_text(source) {
                                out.insert(text.to_string());
                            }
                        }
                        _ => {}
                    });
                }
            }
            _ => {}
        }
    }
    out
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
    fn flags_closure_var_missing_from_key() {
        let diags = run_on("useQuery({ queryKey: ['user'], queryFn: () => fetchUser(userId) });");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("userId"));
    }

    #[test]
    fn allows_closure_var_present_in_key() {
        let diags =
            run_on("useQuery({ queryKey: ['user', userId], queryFn: () => fetchUser(userId) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn flags_only_missing_when_multiple_vars() {
        let diags =
            run_on("useQuery({ queryKey: ['user', userId], queryFn: () => api(userId, filter) });");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("filter"));
        assert!(!diags[0].message.contains("`userId`"));
    }

    #[test]
    fn ignores_param_references() {
        // queryFn receives a context arg — referencing ctx is not a closure.
        let diags = run_on(
            "useQuery({ queryKey: ['user'], queryFn: ({ signal }) => fetch('/x', { signal }) });",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_globals_and_pascal_case() {
        let diags =
            run_on("useQuery({ queryKey: ['x'], queryFn: () => fetch(URL).then(JSON.parse) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn handles_query_options_factory() {
        let diags =
            run_on("queryOptions({ queryKey: ['user'], queryFn: () => fetchUser(userId) });");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_non_query_hooks() {
        let diags = run_on("someOther({ queryKey: ['user'], queryFn: () => fetchUser(userId) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_module_level_imported_singleton() {
        // Regression for rbaumier/comply#65 — Eden / oRPC / tRPC clients
        // declared as module-level imports are stable singletons.
        let src = r#"
            import { api } from "./client";
            export function usersQueryOptions(query: string) {
                return queryOptions({
                    queryKey: ["users", query],
                    queryFn: async ({ signal }) => api.users.get({ query, fetch: { signal } }),
                });
            }
        "#;
        let diags = run_on(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_local_const_in_body() {
        let diags = run_on(
            "useQuery({ queryKey: ['user', userId], queryFn: () => { const x = 1; return fetchUser(userId, x); } });",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn template_string_interpolation_in_key_counts() {
        let diags =
            run_on("useQuery({ queryKey: [`user-${userId}`], queryFn: () => fetchUser(userId) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn object_property_in_key_counts() {
        let diags = run_on(
            "useQuery({ queryKey: ['user', { id: userId }], queryFn: () => fetchUser(userId) });",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }
}
