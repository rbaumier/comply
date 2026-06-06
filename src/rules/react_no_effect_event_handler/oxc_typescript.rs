//! react-no-effect-event-handler OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_effect_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => {
            id.name == "useEffect" || id.name == "useLayoutEffect"
        }
        Expression::StaticMemberExpression(mem) => {
            if let Expression::Identifier(obj) = &mem.object {
                obj.name == "React"
                    && (mem.property.name == "useEffect"
                        || mem.property.name == "useLayoutEffect")
            } else {
                false
            }
        }
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect", "useLayoutEffect"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_effect_callee(&call.callee) {
            return;
        }
        if call.arguments.len() < 2 {
            return;
        }

        // First argument must be arrow/function. `as_expression` yields None for
        // a spread argument (`useEffect(...x)`), which we then skip.
        let Some(callback_expr) = call.arguments[0].as_expression() else {
            return;
        };
        let callback_body = match callback_expr {
            Expression::ArrowFunctionExpression(arrow) => &arrow.body.statements,
            Expression::FunctionExpression(func) => {
                let Some(body) = &func.body else { return };
                &body.statements
            }
            _ => return,
        };

        // Second argument must be an array.
        let Some(deps_expr) = call.arguments[1].as_expression() else {
            return;
        };
        let Expression::ArrayExpression(deps_arr) = deps_expr else {
            return;
        };

        // Collect dep names (identifiers only). Spread and elision elements are
        // not expressions, so `as_expression` yields None and they're skipped.
        let dep_names: Vec<&str> = deps_arr
            .elements
            .iter()
            .filter_map(|el| match el.as_expression() {
                Some(Expression::Identifier(id)) => Some(id.name.as_str()),
                _ => None,
            })
            .collect();

        if dep_names.is_empty() {
            return;
        }

        // Body must have exactly one statement, and it must be an if.
        if callback_body.len() != 1 {
            return;
        }
        let oxc_ast::ast::Statement::IfStatement(if_stmt) = &callback_body[0] else {
            return;
        };

        // The condition must be a single identifier that matches a dep.
        let test_name = match &if_stmt.test {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::ParenthesizedExpression(p) => {
                if let Expression::Identifier(id) = &p.expression {
                    id.name.as_str()
                } else {
                    return;
                }
            }
            _ => return,
        };

        if !dep_names.contains(&test_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`useEffect` simulating an event handler — `{test_name}` change should be handled where it is set."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    // Regression for #895: a spread element in the dependency array made
    // `ArrayExpressionElement::to_expression()` panic (oxc unreachable!),
    // crashing the whole run on payload's `useThrottledEffect`/`useDebouncedEffect`.
    #[test]
    fn does_not_panic_on_spread_deps() {
        assert!(run("useEffect(() => {}, [...deps, delay])").is_empty());
    }

    // A spread element is skipped, but real identifier deps are still detected.
    #[test]
    fn flags_real_dep_alongside_spread() {
        let src = "function App() { useEffect(() => { if (x) { f(); } }, [...deps, x]); }";
        assert_eq!(run(src).len(), 1);
    }
}
