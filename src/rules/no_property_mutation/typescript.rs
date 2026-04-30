use crate::diagnostic::{Diagnostic, Severity};

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

            // Allow: ref.current = ... (React useRef pattern)
            let prop_text = left.child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok())
                .unwrap_or("");
            if prop_text == "current" { return; }

            // Allow: document.cookie = ... (only Web API for client-side cookies)
            if obj_text == "document" && prop_text == "cookie" { return; }

            // Allow: set.* = ... (Elysia response context)
            if root_object_name(left, source) == Some("set") { return; }

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
}
