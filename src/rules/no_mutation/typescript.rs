//! no-mutation backend — flag mutations on `const` bindings.
//!
//! Detects:
//! - Property assignments: `obj.prop = value`, `obj[key] = value`
//! - Compound assignments: `obj.prop += 1`
//! - Mutating array method calls: `arr.push(x)`, `arr.sort()`
//! - Update expressions: `obj.count++`, `--obj.count`
//! - Delete operator: `delete obj.prop`
//! - Object mutators: `Object.assign(obj, ...)`, `Object.defineProperty(obj, ...)`
//!
//! Scope resolution is lightweight: we walk up looking for `const <name>`.

use crate::diagnostic::{Diagnostic, Severity};

const SENTRY_HOOKS: &[&str] = &["beforeSend", "beforeBreadcrumb", "beforeSendTransaction"];

/// True when the mutation is inside a Sentry hook callback — either an inline
/// lambda assigned to `beforeSend`/`beforeBreadcrumb`, or a named function that
/// is registered as one of those hooks somewhere in the same file.
fn is_inside_sentry_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        match parent.kind() {
            "pair" => {
                if let Some(key) = parent.child_by_field_name("key") {
                    let key_text = key.utf8_text(source).unwrap_or("");
                    if SENTRY_HOOKS.contains(&key_text) {
                        return true;
                    }
                }
            }
            "method_definition" => {
                if let Some(name) = parent.child_by_field_name("name") {
                    let name_text = name.utf8_text(source).unwrap_or("");
                    if SENTRY_HOOKS.contains(&name_text) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        cur = parent.parent();
    }

    if let Some(fn_name) = nearest_enclosing_function_name(node, source) {
        let root = {
            let mut n = node;
            while let Some(p) = n.parent() {
                n = p;
            }
            n
        };
        return function_is_sentry_hook(root, source, fn_name);
    }
    false
}

fn nearest_enclosing_function_name<'a>(
    node: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        if parent.kind() == "function_declaration" {
            return parent
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok());
        }
        cur = parent.parent();
    }
    None
}

fn function_is_sentry_hook(root: tree_sitter::Node, source: &[u8], fn_name: &str) -> bool {
    if root.kind() == "pair" {
        if let (Some(key), Some(value)) = (
            root.child_by_field_name("key"),
            root.child_by_field_name("value"),
        ) {
            let key_text = key.utf8_text(source).unwrap_or("");
            if SENTRY_HOOKS.contains(&key_text) && value.kind() == "identifier" {
                let value_text = value.utf8_text(source).unwrap_or("");
                if value_text == fn_name {
                    return true;
                }
            }
        }
    }
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if function_is_sentry_hook(child, source, fn_name) {
            return true;
        }
    }
    false
}

const MUTATING_ARRAY_METHODS: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

const OBJECT_MUTATOR_FUNCTIONS: &[&str] = &[
    "assign",
    "defineProperty",
    "defineProperties",
    "setPrototypeOf",
];

/// Walk down the LHS of an assignment to find the root identifier of
/// a member/subscript chain. Returns `None` if the LHS is a plain
/// identifier (that's a reassignment, not a property mutation) or an
/// unsupported shape (destructuring pattern, etc).
fn root_identifier_of_member_chain<'a>(
    mut node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    // The LHS must be a member/subscript access — a plain identifier
    // means reassignment, which this rule doesn't handle.
    if node.kind() != "member_expression" && node.kind() != "subscript_expression" {
        return None;
    }
    // Walk left-most object until we hit an identifier.
    loop {
        match node.kind() {
            "member_expression" | "subscript_expression" => {
                node = node.child_by_field_name("object")?;
            }
            "identifier" => {
                return node.utf8_text(source).ok();
            }
            _ => return None,
        }
    }
}

/// Return true when any ancestor of `node` declares `name` via `const`.
///
/// We look for a `lexical_declaration` whose first child token is
/// `const` and that contains a `variable_declarator` whose `name` is
/// the target identifier. That matches simple cases and `const { a } =
/// ...` destructuring where `a` appears as an identifier inside the
/// declarator's pattern.
fn declared_as_const(start: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut ancestor = start.parent();
    while let Some(scope) = ancestor {
        let mut cursor = scope.walk();
        for child in scope.named_children(&mut cursor) {
            if is_const_decl_of(child, source, name) {
                return true;
            }
            // `export const x = ...` wraps the lexical_declaration.
            if child.kind() == "export_statement"
                && let Some(decl) = child.child_by_field_name("declaration")
                && is_const_decl_of(decl, source, name)
            {
                return true;
            }
        }
        ancestor = scope.parent();
    }
    false
}

/// True when `name` is bound via `const name = document.createElement(...)` (or `createElementNS`).
/// DOM elements created this way are unattached and must be configured by property assignment
/// before insertion — that's not a state mutation.
fn is_created_dom_element_binding(start: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut ancestor = start.parent();
    while let Some(scope) = ancestor {
        let mut cursor = scope.walk();
        for child in scope.named_children(&mut cursor) {
            if decl_initializer_is_create_element(child, source, name) {
                return true;
            }
            if child.kind() == "export_statement"
                && let Some(decl) = child.child_by_field_name("declaration")
                && decl_initializer_is_create_element(decl, source, name)
            {
                return true;
            }
        }
        ancestor = scope.parent();
    }
    false
}

fn decl_initializer_is_create_element(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if node.kind() != "lexical_declaration" && node.kind() != "variable_declaration" {
        return false;
    }
    let mut cursor = node.walk();
    for decl in node.named_children(&mut cursor) {
        if decl.kind() != "variable_declarator" {
            continue;
        }
        let Some(pat) = decl.child_by_field_name("name") else {
            continue;
        };
        if pat.kind() != "identifier" || pat.utf8_text(source).unwrap_or("") != name {
            continue;
        }
        let Some(value) = decl.child_by_field_name("value") else {
            continue;
        };
        if is_create_element_call(value, source) {
            return true;
        }
    }
    false
}

fn is_create_element_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let obj = callee
        .child_by_field_name("object")
        .and_then(|o| o.utf8_text(source).ok())
        .unwrap_or("");
    if obj != "document" {
        return false;
    }
    let method = callee
        .child_by_field_name("property")
        .and_then(|p| p.utf8_text(source).ok())
        .unwrap_or("");
    method == "createElement" || method == "createElementNS"
}

/// True when `name` is bound to a function expression, arrow function, or
/// function declaration in an ancestor scope.
fn is_function_binding(start: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut ancestor = start.parent();
    while let Some(scope) = ancestor {
        let mut cursor = scope.walk();
        for child in scope.named_children(&mut cursor) {
            if node_binds_function(child, source, name) {
                return true;
            }
            if child.kind() == "export_statement"
                && let Some(decl) = child.child_by_field_name("declaration")
                && node_binds_function(decl, source, name)
            {
                return true;
            }
        }
        ancestor = scope.parent();
    }
    false
}

fn node_binds_function(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if matches!(
        node.kind(),
        "function_declaration" | "generator_function_declaration"
    ) {
        return node
            .child_by_field_name("name")
            .map_or(false, |id| id.utf8_text(source).unwrap_or("") == name);
    }
    if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
        let mut cursor = node.walk();
        return node.named_children(&mut cursor).any(|decl| {
            if decl.kind() != "variable_declarator" {
                return false;
            }
            let Some(pat) = decl.child_by_field_name("name") else {
                return false;
            };
            if pat.kind() != "identifier" || pat.utf8_text(source).unwrap_or("") != name {
                return false;
            }
            decl.child_by_field_name("value").map_or(false, |v| {
                matches!(
                    v.kind(),
                    "arrow_function" | "function_expression" | "generator_function"
                )
            })
        });
    }
    false
}

fn is_const_decl_of(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if node.kind() != "lexical_declaration" {
        return false;
    }
    let Some(kw) = node.child(0) else {
        return false;
    };
    if kw.utf8_text(source).unwrap_or("") != "const" {
        return false;
    }
    // Scan each variable_declarator for the name — covers both
    // `const x = ...` and `const { x } = ...` / `const [x] = ...`.
    let mut cursor = node.walk();
    for decl in node.named_children(&mut cursor) {
        if decl.kind() != "variable_declarator" {
            continue;
        }
        let Some(pat) = decl.child_by_field_name("name") else {
            continue;
        };
        if pattern_binds(pat, source, name) {
            return true;
        }
    }
    false
}

/// True when the destructuring (or identifier) pattern introduces a
/// binding named `name`. Recurses into object/array patterns.
fn pattern_binds(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    match node.kind() {
        "identifier" => node.utf8_text(source).unwrap_or("") == name,
        _ => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .any(|c| pattern_binds(c, source, name))
        }
    }
}

fn report(
    diagnostics: &mut Vec<Diagnostic>,
    path: &std::path::Path,
    node: &tree_sitter::Node,
    root: &str,
    kind: &str,
) {
    diagnostics.push(Diagnostic::at_node(
        path,
        node,
        "no-mutation",
        format!("{kind} `{root}` (declared with `const`) — build a new value instead of mutating."),
        Severity::Warning,
    ));
}

crate::ast_check! { on ["assignment_expression", "augmented_assignment_expression", "update_expression", "unary_expression", "call_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        // obj.prop = x, obj.prop += x
        "assignment_expression" | "augmented_assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else { return };
            // ref.current = ... (React useRef pattern)
            if left.kind() == "member_expression" {
                let prop = left.child_by_field_name("property")
                    .and_then(|p| p.utf8_text(source).ok())
                    .unwrap_or("");
                if prop == "current" { return; }
            }
            let Some(root) = root_identifier_of_member_chain(left, source) else { return };
            if is_created_dom_element_binding(node, source, root) { return; }
            if is_inside_sentry_hook(node, source) { return; }
            if declared_as_const(node, source, root) {
                report(diagnostics, ctx.path, &node, root, "Mutating property of");
            }
        }
        // obj.count++, --obj.count
        "update_expression" => {
            let Some(arg) = node.child_by_field_name("argument") else { return };
            let Some(root) = root_identifier_of_member_chain(arg, source) else { return };
            if is_created_dom_element_binding(node, source, root) { return; }
            if is_inside_sentry_hook(node, source) { return; }
            if declared_as_const(node, source, root) {
                report(diagnostics, ctx.path, &node, root, "Mutating property of");
            }
        }
        // delete obj.prop
        "unary_expression" => {
            let Some(op) = node.child_by_field_name("operator") else { return };
            if op.utf8_text(source).unwrap_or("") != "delete" { return }
            let Some(arg) = node.child_by_field_name("argument") else { return };
            let Some(root) = root_identifier_of_member_chain(arg, source) else { return };
            if is_created_dom_element_binding(node, source, root) { return; }
            if is_inside_sentry_hook(node, source) { return; }
            if declared_as_const(node, source, root) {
                report(diagnostics, ctx.path, &node, root, "Deleting property of");
            }
        }
        // arr.push(x), map.set(k, v), Object.assign(obj, ...)
        "call_expression" => {
            if is_inside_sentry_hook(node, source) { return; }
            let Some(callee) = node.child_by_field_name("function") else { return };
            if callee.kind() != "member_expression" { return }
            let Some(prop) = callee.child_by_field_name("property") else { return };
            let method = prop.utf8_text(source).unwrap_or("");

            // Object.assign(target, ...) — first argument is mutated
            if OBJECT_MUTATOR_FUNCTIONS.contains(&method)
                && let Some(obj) = callee.child_by_field_name("object")
                && obj.utf8_text(source).unwrap_or("") == "Object"
                && let Some(args) = node.child_by_field_name("arguments")
            {
                let mut cursor = args.walk();
                if let Some(first_arg) = args.named_children(&mut cursor).next()
                    && let Some(root) = root_identifier_of_member_chain(first_arg, source)
                        .or_else(|| (first_arg.kind() == "identifier").then(|| first_arg.utf8_text(source).ok()).flatten())
                    && declared_as_const(node, source, root)
                    && !is_function_binding(node, source, root)
                {
                    report(diagnostics, ctx.path, &node, root, "Mutating");
                }
                return;
            }

            let is_mutating = MUTATING_ARRAY_METHODS.contains(&method);
            if !is_mutating { return }

            let Some(obj) = callee.child_by_field_name("object") else { return };
            let root = if obj.kind() == "identifier" {
                obj.utf8_text(source).ok()
            } else {
                root_identifier_of_member_chain(obj, source)
            };
            let Some(root) = root else { return };
            if declared_as_const(node, source, root) {
                report(diagnostics, ctx.path, &node, root, &format!("Calling `{method}()` on"));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    // === Property assignments ===

    #[test]
    fn flags_property_mutation_on_const() {
        let d = run_on("const obj = {}; obj.prop = 1;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_subscript_mutation_on_const() {
        assert_eq!(run_on("const obj = {}; obj['prop'] = 1;").len(), 1);
    }

    #[test]
    fn flags_compound_assignment_on_const() {
        assert_eq!(run_on("const c = { n: 0 }; c.n += 1;").len(), 1);
    }

    #[test]
    fn flags_nested_member_on_const() {
        assert_eq!(run_on("const a = { b: { c: 0 } }; a.b.c = 1;").len(), 1);
    }

    #[test]
    fn flags_mutation_on_exported_const() {
        assert_eq!(run_on("export const obj = {}; obj.x = 1;").len(), 1);
    }

    // === Mutating method calls ===

    #[test]
    fn flags_array_push_on_const() {
        assert_eq!(run_on("const arr = []; arr.push(1);").len(), 1);
    }

    #[test]
    fn flags_array_pop_on_const() {
        assert_eq!(run_on("const arr = [1]; arr.pop();").len(), 1);
    }

    #[test]
    fn flags_array_splice_on_const() {
        assert_eq!(run_on("const arr = [1, 2, 3]; arr.splice(0, 1);").len(), 1);
    }

    #[test]
    fn flags_array_sort_on_const() {
        assert_eq!(run_on("const arr = [3, 1, 2]; arr.sort();").len(), 1);
    }

    #[test]
    fn flags_array_reverse_on_const() {
        assert_eq!(run_on("const arr = [1, 2]; arr.reverse();").len(), 1);
    }

    #[test]
    fn allows_map_set_on_const() {
        assert!(run_on("const map = new Map(); map.set('a', 1);").is_empty());
    }

    #[test]
    fn allows_map_delete_on_const() {
        assert!(run_on("const map = new Map(); map.delete('a');").is_empty());
    }

    #[test]
    fn allows_set_add_on_const() {
        assert!(run_on("const set = new Set(); set.add(1);").is_empty());
    }

    #[test]
    fn allows_set_clear_on_const() {
        assert!(run_on("const set = new Set([1]); set.clear();").is_empty());
    }

    #[test]
    fn flags_nested_array_push() {
        assert_eq!(
            run_on("const obj = { items: [] }; obj.items.push(1);").len(),
            1
        );
    }

    // === Update expressions ===

    #[test]
    fn flags_increment_on_const_property() {
        assert_eq!(run_on("const obj = { n: 0 }; obj.n++;").len(), 1);
    }

    #[test]
    fn flags_decrement_on_const_property() {
        assert_eq!(run_on("const obj = { n: 0 }; --obj.n;").len(), 1);
    }

    // === Delete operator ===

    #[test]
    fn flags_delete_on_const_property() {
        assert_eq!(run_on("const obj = { a: 1 }; delete obj.a;").len(), 1);
    }

    // === Object.assign and friends ===

    #[test]
    fn flags_object_assign_on_const() {
        assert_eq!(
            run_on("const obj = {}; Object.assign(obj, { a: 1 });").len(),
            1
        );
    }

    #[test]
    fn flags_object_define_property_on_const() {
        assert_eq!(
            run_on("const obj = {}; Object.defineProperty(obj, 'a', { value: 1 });").len(),
            1
        );
    }

    // === Allowed patterns ===

    #[test]
    fn allows_ref_current_assignment() {
        assert!(
            run_on("const timerRef = useRef(null); timerRef.current = setTimeout(() => {}, 100);")
                .is_empty()
        );
        assert!(
            run_on("const newKeyRef = useRef(null); newKeyRef.current = data?.key;").is_empty()
        );
    }

    #[test]
    fn allows_mutation_on_let_binding() {
        assert!(run_on("let obj = {}; obj.prop = 1;").is_empty());
    }

    #[test]
    fn allows_array_push_on_let() {
        assert!(run_on("let arr = []; arr.push(1);").is_empty());
    }

    #[test]
    fn allows_plain_reassignment() {
        assert!(run_on("let x = 1; x = 2;").is_empty());
    }

    #[test]
    fn allows_mutation_on_unknown_binding() {
        assert!(run_on("function f(obj: { x: number }) { obj.x = 1; }").is_empty());
    }

    #[test]
    fn allows_push_on_parameter() {
        assert!(run_on("function f(arr: number[]) { arr.push(1); }").is_empty());
    }

    #[test]
    fn allows_new_object_via_spread() {
        assert!(run_on("const obj = { a: 1 }; const next = { ...obj, b: 2 };").is_empty());
    }

    #[test]
    fn allows_non_mutating_methods() {
        assert!(run_on("const arr = [1, 2, 3]; const x = arr.map(n => n * 2);").is_empty());
        assert!(run_on("const arr = [1, 2, 3]; const x = arr.filter(n => n > 1);").is_empty());
        assert!(run_on("const arr = [1, 2, 3]; const x = arr.slice(0, 1);").is_empty());
    }

    #[test]
    fn allows_object_assign_to_new_object() {
        assert!(
            run_on("const obj = {}; const next = Object.assign({}, obj, { a: 1 });").is_empty()
        );
    }

    #[test]
    fn allows_object_assign_on_const_arrow_function() {
        // Attaching metadata to a named handler — not a data mutation (issue #364).
        assert!(run_on(
            r#"const handler = async (ctx) => { return ctx.body; };
Object.assign(handler, { displayName: "myHandler" });"#
        )
        .is_empty());
    }

    #[test]
    fn allows_object_assign_on_const_function_declaration() {
        assert!(run_on(
            r#"function handler(ctx) { return ctx.body; }
Object.assign(handler, { displayName: "myHandler" });"#
        )
        .is_empty());
    }

    #[test]
    fn still_flags_object_assign_on_const_plain_object() {
        // Object target — must still be flagged.
        assert_eq!(
            run_on("const obj = {}; Object.assign(obj, { a: 1 });").len(),
            1
        );
    }

    #[test]
    fn allows_property_assignment_on_created_dom_element() {
        let src = r#"
            const anchor = document.createElement("a");
            anchor.href = objectUrl;
            anchor.download = filename;
            anchor.rel = "noopener";
            document.body.append(anchor);
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_created_svg_element() {
        let src = r#"
            const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            svg.id = "chart";
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_mutation_on_unrelated_const() {
        let src = r#"
            const anchor = getAnchorFromDom();
            anchor.href = objectUrl;
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Sentry beforeSend/beforeBreadcrumb — issue #581

    #[test]
    fn allows_const_alias_mutation_inside_before_send() {
        // const alias of nested property mutated inside an inline beforeSend hook
        let src = r#"
            Sentry.init({
                beforeSend: (event) => {
                    const req = event.request;
                    if (req) req.url = scrubSensitiveQueryFromUrl(req.url);
                    return event;
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_const_alias_mutation_in_named_function_registered_as_before_send() {
        // Named function with const alias — registered as beforeSend — issue #581
        let src = r#"
            function scrubEvent(event) {
                const req = event.request;
                if (req) req.url = scrub(req.url);
                return event;
            }
            Sentry.init({ beforeSend: scrubEvent });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_const_mutation_outside_sentry_hook() {
        // Same shape but NOT a Sentry hook — must still be flagged.
        let src = r#"
            function scrubEvent(event) {
                const req = event.request;
                if (req) req.url = scrub(req.url);
                return event;
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
