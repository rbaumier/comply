//! zod-no-schema-in-hot-path backend — flag `z.*` calls whose nearest enclosing
//! function looks like a React component or a request handler.
//!
//! Heuristic: walk upward from the `z.*` call and look for:
//! * a `function_declaration` / arrow `variable_declarator` whose name starts
//!   with an uppercase letter (React component), OR
//! * a function whose parameter list mentions `req`, `request`, or `ctx` (handler).
//!
//! If we find such a scope before we hit the program root, the call is in a
//! hot path.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};

fn starts_uppercase(name: &str) -> bool {
    name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}

fn function_name<'a>(func: Node<'a>, source: &'a [u8]) -> Option<String> {
    // function_declaration name field.
    if let Some(n) = func.child_by_field_name("name")
        && let Ok(t) = n.utf8_text(source) {
            return Some(t.to_string());
        }
    // arrow function: parent is variable_declarator with a name field.
    if let Some(parent) = func.parent()
        && parent.kind() == "variable_declarator"
            && let Some(n) = parent.child_by_field_name("name")
                && let Ok(t) = n.utf8_text(source) {
                    return Some(t.to_string());
                }
    None
}

fn looks_like_handler<'a>(func: Node<'a>, source: &'a [u8]) -> bool {
    let Some(params) = func.child_by_field_name("parameters") else { return false };
    let Ok(text) = params.utf8_text(source) else { return false };
    // Cheap textual check on the parameter list.
    text.contains("req") || text.contains("request") || text.contains("ctx")
        || text.contains("res") || text.contains("response")
}

fn is_hot_scope<'a>(func: Node<'a>, source: &'a [u8]) -> bool {
    if let Some(name) = function_name(func, source)
        && starts_uppercase(&name) && name != "Check" {
            return true;
        }
    looks_like_handler(func, source)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(obj) = func.child_by_field_name("object") else { return };
    // Match `z.<something>(...)` — the root of the chain must be the identifier `z`.
    let mut root = obj;
    loop {
        match root.kind() {
            "identifier" => break,
            "member_expression" => {
                let Some(next) = root.child_by_field_name("object") else { return };
                root = next;
            }
            "call_expression" => {
                let Some(f) = root.child_by_field_name("function") else { return };
                root = f;
            }
            _ => return,
        }
    }
    if root.utf8_text(source).map(|t| t != "z").unwrap_or(true) { return; }

    // Walk up to find the nearest enclosing function or loop scope.
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "do_statement" => {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: "Zod schema built inside a loop body — hoist it outside the \
                              loop so it is only constructed once.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition" => {
                if is_hot_scope(p, source) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: super::META.id.into(),
                        message: "Zod schema built inside a React component or request \
                                  handler — hoist it to module scope so it is only \
                                  constructed once.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                return;
            }
            "program" => return,
            _ => {}
        }
        cur = p.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_schema_in_react_component() {
        let src = "function MyForm() { const S = z.object({ a: z.string() }); return null; }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_schema_in_handler() {
        let src = "const handler = (req, res) => { const S = z.object({ a: z.string() }); };";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_module_level_schema() {
        let src = "const S = z.object({ a: z.string() });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_schema_in_plain_helper() {
        let src = "function helper() { const S = z.object({ a: z.string() }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_schema_in_for_loop() {
        let src = "for (let i = 0; i < 10; i++) { const S = z.string(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_schema_in_while_loop() {
        let src = "while (running) { const S = z.string(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_schema_in_for_in_loop() {
        let src = "for (const k in obj) { const S = z.string(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_schema_in_do_loop() {
        let src = "do { const S = z.string(); } while (running);";
        assert_eq!(run(src).len(), 1);
    }
}
