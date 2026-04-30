//! elysia-static-inline-value backend — flag arrow handlers that just return a string literal.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];

fn is_string_literal(node: tree_sitter::Node) -> bool {
    let kind = node.kind();
    kind == "string" || kind == "template_string"
}

/// Return Some(str_node) if `arrow` is `() => "literal"` — body is a bare
/// string expression (either as direct expression body or as a single
/// `return "x";` inside a block).
fn arrow_returns_only_string<'a>(arrow: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let body = arrow.child_by_field_name("body")?;
    if is_string_literal(body) {
        return Some(body);
    }
    if body.kind() == "statement_block" {
        // Find a single return_statement with a string argument; ignore empty/comment-only blocks.
        let mut return_stmt: Option<tree_sitter::Node> = None;
        let mut other_stmts = 0;
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else { continue };
            let kind = child.kind();
            if kind == "{" || kind == "}" || kind == "comment" {
                continue;
            }
            if kind == "return_statement" {
                return_stmt = Some(child);
            } else {
                other_stmts += 1;
            }
        }
        if other_stmts > 0 {
            return None;
        }
        let ret = return_stmt?;
        for i in 0..ret.child_count() {
            let Some(child) = ret.child(i) else { continue };
            if child.kind() == "return" || child.kind() == ";" {
                continue;
            }
            if is_string_literal(child) {
                return Some(child);
            }
        }
    }
    None
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let prop_text = property.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&prop_text) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut real_args: Vec<tree_sitter::Node> = Vec::new();
    for i in 0..args.child_count() {
        let Some(child) = args.child(i) else { continue };
        let kind = child.kind();
        if kind == "(" || kind == ")" || kind == "," {
            continue;
        }
        real_args.push(child);
    }
    if real_args.len() < 2 {
        return;
    }
    let handler = real_args[1];
    if handler.kind() != "arrow_function" {
        return;
    }

    // Bail if the arrow takes any parameters — handler may rely on context.
    if let Some(params) = handler.child_by_field_name("parameters") {
        if params.kind() == "formal_parameters" {
            let mut has_param = false;
            for i in 0..params.child_count() {
                let Some(child) = params.child(i) else { continue };
                let kind = child.kind();
                if kind != "(" && kind != ")" && kind != "," {
                    has_param = true;
                    break;
                }
            }
            if has_param {
                return;
            }
        }
    }

    if arrow_returns_only_string(handler).is_none() {
        return;
    }

    let pos = handler.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-static-inline-value".into(),
        message: "Handler returns only a static string — pass the literal directly so Elysia can compile it ahead of time.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_arrow_returning_string_literal() {
        let src = "import { Elysia } from 'elysia';\napp.get('/health', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_arrow_with_block_returning_string() {
        let src = "import { Elysia } from 'elysia';\napp.get('/health', () => { return 'ok'; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_handler_using_context() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ body }) => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_string_arg() {
        let src = "import { Elysia } from 'elysia';\napp.get('/health', 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/health', () => 'ok');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
