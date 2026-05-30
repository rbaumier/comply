use crate::diagnostic::{Diagnostic, Severity};

const SENTRY_HOOKS: &[&str] = &["beforeSend", "beforeBreadcrumb", "beforeSendTransaction"];

/// True when the mutation is inside a Sentry hook callback — either an inline
/// lambda assigned to `beforeSend`/`beforeBreadcrumb`, or a named function that
/// is registered as one of those hooks somewhere in the same file.
fn is_inside_sentry_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk up: if we pass through a pair/method_definition with a Sentry hook
    // key, the mutation is inside an inline callback.
    let mut cur = node.parent();
    while let Some(parent) = cur {
        match parent.kind() {
            // { beforeSend: (event) => { ... } }
            "pair" => {
                if let Some(key) = parent.child_by_field_name("key") {
                    let key_text = key.utf8_text(source).unwrap_or("");
                    if SENTRY_HOOKS.contains(&key_text) {
                        return true;
                    }
                }
            }
            // { beforeSend(event) { ... } }
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

    // Named function case: find the nearest enclosing function_declaration name,
    // then scan the file for that name used as a Sentry hook value.
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

/// Scan `root` recursively for `pair` nodes where the key is a Sentry hook
/// and the value is an identifier equal to `fn_name`.
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

fn root_object_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cur = node;
    loop {
        match cur.kind() {
            "member_expression" | "subscript_expression" => {
                cur = cur.child_by_field_name("object")?;
            }
            "identifier" => return cur.utf8_text(source).ok(),
            _ => return None,
        }
    }
}

/// True when `name` is bound in scope via `const/let/var name = document.createElement(...)`
/// or `document.createElementNS(...)`. DOM elements created this way are unattached and
/// must be configured by property assignment before insertion — that's not a state mutation.
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

const TEST_GLOBALS: &[&str] = &["console", "window", "global", "globalThis", "process"];
const TEST_HOOKS: &[&str] = &["beforeEach", "afterEach", "beforeAll", "afterAll"];

fn is_test_setup_mutation(
    node: tree_sitter::Node,
    mutated: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
) -> bool {
    if !ctx.file.path_segments.in_test_dir {
        return false;
    }
    if root_object_name(mutated, source).is_some_and(|name| TEST_GLOBALS.contains(&name)) {
        return true;
    }
    is_inside_test_hook(node, source)
}

fn is_inside_test_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        if parent.kind() == "call_expression"
            && let Some(function) = parent.child_by_field_name("function")
        {
            let callee = function.utf8_text(source).unwrap_or("");
            let name = callee.rsplit('.').next().unwrap_or(callee);
            if TEST_HOOKS.contains(&name) {
                return true;
            }
        }
        cur = parent.parent();
    }
    false
}

crate::ast_check! { on ["assignment_expression", "augmented_assignment_expression", "update_expression", "unary_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        // obj.prop = value or obj['prop'] = value
        "assignment_expression" | "augmented_assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else { return; };
            if !matches!(left.kind(), "member_expression" | "subscript_expression") { return; }

            // Allow: module.exports = ...
            let obj_text = left.child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            if obj_text == "module" || obj_text == "exports" { return; }

            if is_test_setup_mutation(node, left, source, ctx) { return; }
            if is_inside_sentry_hook(node, source) { return; }

            // Allow: ref.current = ... (React useRef pattern)
            let prop_text = left.child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok())
                .unwrap_or("");
            if prop_text == "current" { return; }

            // Allow: document.cookie = ... (only Web API for client-side cookies)
            if obj_text == "document" && prop_text == "cookie" { return; }

            // Allow: set.* = ... (Elysia response context)
            if root_object_name(left, source) == Some("set") { return; }

            // Allow: const el = document.createElement(...); el.href = ...
            if let Some(root) = root_object_name(left, source)
                && is_created_dom_element_binding(node, source, root)
            { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-property-mutation".into(),
                message: "Property mutation — use spread or immutable patterns.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // ++obj.prop or obj.prop++
        "update_expression" => {
            let Some(arg) = node.named_child(0) else { return; };
            if !matches!(arg.kind(), "member_expression" | "subscript_expression") { return; }
            if is_test_setup_mutation(node, arg, source, ctx) { return; }
            if is_inside_sentry_hook(node, source) { return; }
            if let Some(root) = root_object_name(arg, source)
                && is_created_dom_element_binding(node, source, root)
            { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-property-mutation".into(),
                message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // delete obj.prop
        "unary_expression" => {
            let op = node.child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            if op != "delete" { return; }

            let Some(arg) = node.child_by_field_name("argument") else { return; };
            if !matches!(arg.kind(), "member_expression" | "subscript_expression") { return; }
            if is_test_setup_mutation(node, arg, source, ctx) { return; }
            if is_inside_sentry_hook(node, source) { return; }
            if let Some(root) = root_object_name(arg, source)
                && is_created_dom_element_binding(node, source, root)
            { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-property-mutation".into(),
                message: "Property deletion — use destructuring or immutable patterns.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(code, &Check)
    }

    fn run_test(code: &str) -> Vec<Diagnostic> {
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        crate::rules::test_helpers::run_ts_with_file_ctx(code, &Check, &file)
    }

    #[test]
    fn flags_property_assignment() {
        assert_eq!(run("obj.prop = 1").len(), 1);
        assert_eq!(run("obj['prop'] = 1").len(), 1);
    }

    #[test]
    fn flags_compound_assignment() {
        assert_eq!(run("obj.count += 1").len(), 1);
        assert_eq!(run("obj.str += 'x'").len(), 1);
    }

    #[test]
    fn flags_increment() {
        assert_eq!(run("obj.count++").len(), 1);
        assert_eq!(run("++obj.count").len(), 1);
    }

    #[test]
    fn flags_delete() {
        assert_eq!(run("delete obj.prop").len(), 1);
    }

    #[test]
    fn allows_variable_assignment() {
        assert!(run("let x = 1").is_empty());
        assert!(run("x = 1").is_empty());
    }

    #[test]
    fn allows_module_exports() {
        assert!(run("module.exports = {}").is_empty());
        assert!(run("exports.foo = bar").is_empty());
    }

    #[test]
    fn allows_ref_current() {
        assert!(run("timerRef.current = setTimeout(() => {}, 100)").is_empty());
        assert!(run("newKeyRef.current = null").is_empty());
    }

    #[test]
    fn allows_elysia_set_status() {
        assert!(run("set.status = 404").is_empty());
    }

    #[test]
    fn allows_elysia_set_headers() {
        assert!(run(r#"set.headers["cache-control"] = "no-store""#).is_empty());
    }

    #[test]
    fn allows_elysia_set_nested() {
        assert!(run(r#"set.headers["x-content-type-options"] = "nosniff""#).is_empty());
    }

    #[test]
    fn allows_document_cookie() {
        assert!(run(r#"document.cookie = "name=value""#).is_empty());
    }

    #[test]
    fn still_flags_other_subscript_mutations() {
        assert_eq!(run(r#"obj.headers["key"] = "val""#).len(), 1);
    }

    #[test]
    fn still_flags_non_set_mutations() {
        assert_eq!(run("response.statusText = 'OK'").len(), 1);
    }

    #[test]
    fn allows_test_global_mutations() {
        assert!(run_test("console.error = vi.fn();").is_empty());
        assert!(run_test("window.localStorage = mockStorage;").is_empty());
        assert!(run_test("globalThis.fetch = vi.fn();").is_empty());
    }

    #[test]
    fn allows_mutations_inside_test_hooks() {
        let src = "beforeEach(() => { store.state = initialState; });";
        assert!(run_test(src).is_empty());
    }

    #[test]
    fn still_flags_regular_test_mutations() {
        assert_eq!(run_test("store.state = nextState;").len(), 1);
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
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_created_svg_element() {
        let src = r#"
            const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            svg.id = "chart";
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutation_on_unrelated_const() {
        let src = r#"
            const anchor = getAnchorFromDom();
            anchor.href = objectUrl;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Sentry beforeSend/beforeBreadcrumb — issue #581

    #[test]
    fn allows_mutation_inside_inline_before_send_arrow() {
        // Sentry.init({ beforeSend: (event) => { event.request.url = scrub(url); } })
        let src = r#"
            Sentry.init({
                beforeSend: (event) => {
                    event.request.url = scrubSensitiveQueryFromUrl(url);
                    return event;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_inside_inline_before_breadcrumb_method() {
        // { beforeBreadcrumb(breadcrumb) { breadcrumb.data = {}; } }
        let src = r#"
            Sentry.init({
                beforeBreadcrumb(breadcrumb) {
                    breadcrumb.data = sanitize(breadcrumb.data);
                    return breadcrumb;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_in_named_function_registered_as_before_send() {
        // Named function passed by reference to beforeSend — issue #581
        let src = r#"
            function scrubEventRequestUrl(event) {
                event.request.url = scrubSensitiveQueryFromUrl(event.request.url);
                return event;
            }
            Sentry.init({ beforeSend: scrubEventRequestUrl });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_subscript_mutation_in_named_function_registered_as_before_breadcrumb() {
        // Subscript mutation inside named helper for beforeBreadcrumb — issue #581
        let src = r#"
            function scrubStringField(bag, key) {
                bag[key] = scrubSensitiveQueryFromUrl(bag[key]);
            }
            Sentry.init({ beforeBreadcrumb: scrubStringField });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutation_outside_sentry_hook() {
        // A function with the same shape but NOT registered as a Sentry hook
        // must still be flagged.
        let src = r#"
            function scrubStringField(bag, key) {
                bag[key] = scrubSensitiveQueryFromUrl(bag[key]);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
