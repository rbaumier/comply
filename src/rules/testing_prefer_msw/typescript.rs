//! testing-prefer-msw backend — flag direct HTTP-client mocks.
//!
//! Matches two shapes in test files only:
//!   - `vi.mock(<http-client>)` / `jest.mock(<http-client>)` where the
//!     module is `axios`, `node-fetch`, or `cross-fetch`.
//!   - `global.fetch = vi.fn()` / `globalThis.fetch = jest.fn()` and the
//!     `jest.spyOn(global, 'fetch')` variants.
//!
//! MSW intercepts at the network layer, so the same handler works with
//! any HTTP client — mocking the client itself couples the tests to the
//! library and re-breaks on every refactor.

use crate::diagnostic::{Diagnostic, Severity};

const HTTP_CLIENT_MODULES: &[&str] = &["axios", "node-fetch", "cross-fetch"];

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Strip surrounding quotes from a `string` / `template_string` literal
/// as emitted by tree-sitter (includes the quote chars in the text).
fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

fn push(
    diagnostics: &mut Vec<Diagnostic>,
    ctx: &crate::rules::backend::CheckCtx,
    node: tree_sitter::Node,
) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "testing-prefer-msw".into(),
        message: "Mocking the HTTP client directly is brittle — use MSW to intercept network requests at the handler level.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// `<obj>.<method>` where `method` matches `method_name`. Returns the
/// `<obj>` text if matched.
fn member_call_object<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    method_name: &str,
) -> Option<&'a str> {
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if func.kind() != "member_expression" {
        return None;
    }
    let object = func.child_by_field_name("object")?;
    let property = func.child_by_field_name("property")?;
    if property.utf8_text(source).ok()? != method_name {
        return None;
    }
    object.utf8_text(source).ok()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) { return; }

    // vi.mock('axios') / jest.mock('node-fetch')
    if let Some(obj) = member_call_object(node, source, "mock")
        && (obj == "vi" || obj == "jest")
    {
        let Some(args) = node.child_by_field_name("arguments") else { return };
        let Some(first) = args.named_child(0) else { return };
        if matches!(first.kind(), "string" | "template_string") {
            let raw = first.utf8_text(source).unwrap_or("");
            if HTTP_CLIENT_MODULES.contains(&unquote(raw)) {
                push(diagnostics, ctx, node);
            }
        }
        return;
    }

    // global.fetch = vi.fn()  /  globalThis.fetch = jest.fn()
    if node.kind() == "assignment_expression" {
        let Some(left) = node.child_by_field_name("left") else { return };
        if left.kind() != "member_expression" { return; }
        let Some(lobj) = left.child_by_field_name("object") else { return };
        let Some(lprop) = left.child_by_field_name("property") else { return };
        let obj_name = lobj.utf8_text(source).unwrap_or("");
        let prop_name = lprop.utf8_text(source).unwrap_or("");
        if !matches!(obj_name, "global" | "globalThis") || prop_name != "fetch" {
            return;
        }
        let Some(right) = node.child_by_field_name("right") else { return };
        // Right side should be vi.fn() / jest.fn().
        if member_call_object(right, source, "fn").is_some_and(|o| o == "vi" || o == "jest") {
            push(diagnostics, ctx, node);
        }
        return;
    }

    // jest.spyOn(global, 'fetch') / jest.spyOn(globalThis, 'fetch')
    if let Some(obj) = member_call_object(node, source, "spyOn")
        && (obj == "jest" || obj == "vi")
    {
        let Some(args) = node.child_by_field_name("arguments") else { return };
        let Some(first) = args.named_child(0) else { return };
        let Some(second) = args.named_child(1) else { return };
        let first_text = first.utf8_text(source).unwrap_or("");
        if !matches!(first_text, "global" | "globalThis") { return; }
        if matches!(second.kind(), "string" | "template_string") {
            let raw = second.utf8_text(source).unwrap_or("");
            if unquote(raw) == "fetch" {
                push(diagnostics, ctx, node);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_vi_mock_axios() {
        assert_eq!(run("a.test.ts", "vi.mock('axios')").len(), 1);
    }

    #[test]
    fn flags_jest_mock_node_fetch() {
        assert_eq!(run("a.test.ts", "jest.mock(\"node-fetch\")").len(), 1);
    }

    #[test]
    fn flags_global_fetch_assignment() {
        assert_eq!(run("a.test.ts", "global.fetch = vi.fn();").len(), 1);
    }

    #[test]
    fn flags_globalthis_fetch_assignment() {
        assert_eq!(run("a.spec.ts", "globalThis.fetch = jest.fn();").len(), 1);
    }

    #[test]
    fn flags_spy_on_global_fetch() {
        assert_eq!(run("a.test.ts", "jest.spyOn(global, 'fetch');").len(), 1);
    }

    #[test]
    fn allows_mock_of_local_module() {
        assert!(run("a.test.ts", "vi.mock('./utils')").is_empty());
    }

    #[test]
    fn allows_msw_handler() {
        assert!(run("a.test.ts", "server.use(http.get('/api', resolver));").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run("utils.ts", "vi.mock('axios')").is_empty());
    }
}
