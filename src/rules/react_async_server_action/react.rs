//! react-async-server-action backend — server actions must be async.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is a "use server" string literal.
fn is_use_server(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "expression_statement" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string" {
            let Ok(text) = child.utf8_text(source) else {
                continue;
            };
            let inner = text.trim_matches(|c| c == '"' || c == '\'' || c == ';');
            if inner == "use server" {
                return true;
            }
        }
    }
    false
}

/// Check if a function node has the `async` keyword.
fn is_async_func(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    text.starts_with("async ")
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Check for file-level "use server" directive (in the first few statements)
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    let file_level_use_server = children.iter().take(5).any(|c| is_use_server(*c, source));

    if file_level_use_server {
        // All exported functions must be async
        for child in &children {
            if child.kind() == "export_statement" {
                let mut ec = child.walk();
                for inner in child.children(&mut ec) {
                    if inner.kind() == "function_declaration" && !is_async_func(inner, source) {
                        let pos = inner.start_position();
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "react-async-server-action".into(),
                            message: "Server action must be `async`. This file has \
                                      `\"use server\"` at the top \u{2014} all exported \
                                      functions must be async."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
            }
        }
    }

    // Check for inline "use server" inside function bodies
    // Walk all function declarations/expressions and check their first statement
    let mut stack: Vec<tree_sitter::Node> = children.clone();
    while let Some(n) = stack.pop() {
        let is_func = n.kind() == "function_declaration"
            || n.kind() == "function"
            || n.kind() == "function_expression"
            || n.kind() == "arrow_function";

        if is_func
            && let Some(body) = n.child_by_field_name("body")
                && body.kind() == "statement_block" {
                    let mut bc = body.walk();
                    for stmt in body.children(&mut bc) {
                        if is_use_server(stmt, source) && !is_async_func(n, source) {
                            let pos = n.start_position();
                            diagnostics.push(Diagnostic {
                                path: std::sync::Arc::clone(&ctx.path_arc),
                                line: pos.row + 1,
                                column: pos.column + 1,
                                rule_id: "react-async-server-action".into(),
                                message: "Server action must be `async`. This function \
                                          contains `\"use server\"` but is not async."
                                    .into(),
                                severity: Severity::Error,
                                span: None,
                            });
                            break;
                        }
                    }
                }

        // Keep walking into children to find nested functions
        let mut child_cursor = n.walk();
        for child in n.children(&mut child_cursor) {
            stack.push(child);
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_non_async_with_file_directive() {
        let src = r#"
"use server"

export function createPost(data: FormData) {
    // ...
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_with_file_directive() {
        let src = r#"
"use server"

export async function createPost(data: FormData) {
    // ...
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_non_async_inline_use_server() {
        let src = r#"
function Component() {
    return <form>ok</form>;
}

function submitForm() {
    "use server"
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_inline_use_server() {
        let src = r#"
function Component() {
    return <form>ok</form>;
}

async function submitForm() {
    "use server"
}
"#;
        assert!(run_on(src).is_empty());
    }
}
