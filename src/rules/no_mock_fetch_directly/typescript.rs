//! no-mock-fetch-directly backend — detect direct mocking of HTTP clients
//! in test files.
//!
//! Flags `vi.mock('axios')`, `jest.mock('node-fetch')`,
//! `global.fetch = vi.fn()`, `globalThis.fetch = jest.fn()`, and similar.

use crate::diagnostic::{Diagnostic, Severity};

const MOCKED_MODULES: &[&str] = &["axios", "node-fetch"];
const FETCH_GLOBALS: &[&str] = &["global.fetch", "globalThis.fetch"];

crate::ast_check! { on ["call_expression", "assignment_expression"] prefilter = ["fetch"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    match node.kind() {
        "call_expression" => check_mock_call(node, source, ctx, diagnostics),
        "assignment_expression" => check_fetch_assignment(node, source, ctx, diagnostics),
        _ => {}
    }
}

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Check for `vi.mock('axios')` / `jest.mock('node-fetch')`.
fn check_mock_call(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };
    let func_text = match func.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    let framework = if func_text == "vi.mock" {
        "vi"
    } else if func_text == "jest.mock" {
        "jest"
    } else {
        return;
    };

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if arg.kind() != "string" {
            continue;
        }
        let arg_text = match arg.utf8_text(source) {
            Ok(t) => t,
            Err(_) => continue,
        };
        // Strip quotes
        let module = arg_text.trim_matches(|c| c == '\'' || c == '"');
        if MOCKED_MODULES.contains(&module) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-mock-fetch-directly".into(),
                message: format!(
                    "Direct mock of `{module}` via `{framework}.mock` — \
                     use MSW to intercept at the network level instead."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Check for `global.fetch = vi.fn()` / `globalThis.fetch = jest.fn()`.
fn check_fetch_assignment(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let left_text = match left.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !FETCH_GLOBALS.contains(&left_text) {
        return;
    }
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    let right_text = match right.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if right_text.contains("vi.fn()") || right_text.contains("jest.fn()") {
        let mock_fn = if right_text.contains("vi.fn()") {
            "vi.fn()"
        } else {
            "jest.fn()"
        };
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-mock-fetch-directly".into(),
            message: format!(
                "Reassigning `{left_text}` with `{mock_fn}` — \
                 use MSW to intercept at the network level instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let ctx = crate::rules::backend::CheckCtx::for_test(
            std::path::Path::new("src/api.test.ts"),
            source,
        );
        use crate::rules::backend::AstCheck;
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_vi_mock_axios() {
        let d = run_on("vi.mock('axios')");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("axios"));
    }

    #[test]
    fn flags_jest_mock_axios_double_quotes() {
        let d = run_on("jest.mock(\"axios\")");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_global_fetch_vi_fn() {
        let d = run_on("global.fetch = vi.fn()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("global.fetch"));
    }

    #[test]
    fn allows_msw_import() {
        assert!(run_on("import { setupServer } from 'msw/node'").is_empty());
    }
}
