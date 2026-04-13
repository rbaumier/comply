//! no-useless-undefined backend — flag explicit `undefined` where JS
//! already defaults to it: `return undefined`, `let x = undefined`,
//! `const {a = undefined} = obj`, `yield undefined`, `function f(a = undefined)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // tree-sitter parses `undefined` as its own node kind, not as `identifier`.
    if node.kind() != "undefined" {
        return;
    }

    let Some(parent) = node.parent() else { return };

    match parent.kind() {
        // `return undefined;`
        // return_statement has no field for the value — it's a direct named child.
        "return_statement" => {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-useless-undefined".into(),
                message: "Do not use useless `undefined`. `return` without \
                          a value already returns `undefined`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        // `yield undefined;`
        "yield_expression" => {
            // Exclude `yield* undefined` (delegate).
            let parent_text = parent.utf8_text(source).unwrap_or("");
            if !parent_text.contains("yield*") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-useless-undefined".into(),
                    message: "Do not use useless `undefined`. `yield` without \
                              a value already yields `undefined`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        // `let x = undefined` or `var x = undefined`
        // variable_declarator has field "value" pointing to the initializer.
        "variable_declarator" => {
            if parent.child_by_field_name("value").map(|v| v.id()) != Some(node.id()) {
                return;
            }
            let Some(decl) = parent.parent() else { return };
            // lexical_declaration = let/const, variable_declaration = var
            match decl.kind() {
                "lexical_declaration" => {
                    // Check for `const` vs `let`.
                    // lexical_declaration has field "kind" for the keyword.
                    let kind_text = decl
                        .child_by_field_name("kind")
                        .and_then(|k| k.utf8_text(source).ok())
                        .unwrap_or("");
                    if kind_text == "const" {
                        return; // const x = undefined is intentional
                    }
                }
                "variable_declaration" => {
                    // var — always flag
                }
                _ => return,
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-useless-undefined".into(),
                message: "Do not use useless `undefined`. `let`/`var` declarations \
                          default to `undefined`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        // `function f(a = undefined)` — tree-sitter: required_parameter
        // The required_parameter contains: identifier, =, undefined
        "required_parameter" | "optional_parameter" => {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-useless-undefined".into(),
                message: "Do not use useless `undefined`. Default parameter \
                          values already default to `undefined`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        // `const { a = undefined } = obj` — tree-sitter: object_assignment_pattern
        "object_assignment_pattern" => {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-useless-undefined".into(),
                message: "Do not use useless `undefined`. Default parameter \
                          values already default to `undefined`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        // `[a = undefined] = arr` — tree-sitter: assignment_pattern
        "assignment_pattern" => {
            // Only flag when undefined is the right side of the default.
            if parent.child_by_field_name("right").map(|v| v.id()) == Some(node.id()) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-useless-undefined".into(),
                    message: "Do not use useless `undefined`. Default parameter \
                              values already default to `undefined`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
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

    // ---- return undefined ----

    #[test]
    fn flags_return_undefined() {
        assert_eq!(run_on("function f() { return undefined; }").len(), 1);
    }

    #[test]
    fn allows_return_value() {
        assert!(run_on("function f() { return 42; }").is_empty());
    }

    #[test]
    fn allows_bare_return() {
        assert!(run_on("function f() { return; }").is_empty());
    }

    // ---- let/var = undefined ----

    #[test]
    fn flags_let_undefined() {
        assert_eq!(run_on("let x = undefined;").len(), 1);
    }

    #[test]
    fn flags_var_undefined() {
        assert_eq!(run_on("var x = undefined;").len(), 1);
    }

    #[test]
    fn allows_const_undefined() {
        assert!(run_on("const x = undefined;").is_empty());
    }

    #[test]
    fn allows_let_with_value() {
        assert!(run_on("let x = 1;").is_empty());
    }

    // ---- default parameter = undefined ----

    #[test]
    fn flags_default_param_undefined() {
        assert_eq!(run_on("function f(a = undefined) {}").len(), 1);
    }

    #[test]
    fn flags_destructuring_default_undefined() {
        assert_eq!(run_on("const { a = undefined } = obj;").len(), 1);
    }

    #[test]
    fn allows_default_param_with_value() {
        assert!(run_on("function f(a = 1) {}").is_empty());
    }

    // ---- yield undefined ----

    #[test]
    fn flags_yield_undefined() {
        assert_eq!(run_on("function* g() { yield undefined; }").len(), 1);
    }

    #[test]
    fn allows_yield_value() {
        assert!(run_on("function* g() { yield 42; }").is_empty());
    }
}
