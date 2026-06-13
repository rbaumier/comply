//! react-no-initialize-state-in-effect OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// Global timer/scheduler functions that match the `set` + uppercase shape but
/// are not React state setters.
const TIMER_GLOBALS: &[&str] = &["setInterval", "setTimeout", "setImmediate"];

/// True if `name` looks like a React state setter (`setFoo`) and is not one of
/// the global timer functions.
fn is_setter_name(name: &str) -> bool {
    name.starts_with("set")
        && name.len() > 3
        && name.as_bytes()[3].is_ascii_uppercase()
        && !TIMER_GLOBALS.contains(&name)
}

/// True if a call expression is a setter like `setFoo(...)`.
fn is_setter_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    is_setter_name(callee.name.as_str())
}

/// Walk statements recursively (but not into nested functions) looking for
/// a setter call.
fn body_calls_setter(stmts: &[oxc_ast::ast::Statement]) -> bool {
    for stmt in stmts {
        if stmt_calls_setter(stmt) {
            return true;
        }
    }
    false
}

fn stmt_calls_setter(stmt: &oxc_ast::ast::Statement) -> bool {
    match stmt {
        oxc_ast::ast::Statement::ExpressionStatement(expr) => {
            expr_calls_setter(&expr.expression)
        }
        oxc_ast::ast::Statement::VariableDeclaration(decl) => {
            decl.declarations.iter().any(|d| {
                d.init.as_ref().is_some_and(|e| expr_calls_setter(e))
            })
        }
        oxc_ast::ast::Statement::IfStatement(if_stmt) => {
            if let oxc_ast::ast::Statement::BlockStatement(block) = &if_stmt.consequent
                && body_calls_setter(&block.body) {
                    return true;
                }
            if let Some(alt) = &if_stmt.alternate
                && stmt_calls_setter(alt) {
                    return true;
                }
            false
        }
        oxc_ast::ast::Statement::BlockStatement(block) => body_calls_setter(&block.body),
        oxc_ast::ast::Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().is_some_and(|e| expr_calls_setter(e))
        }
        _ => false,
    }
}

fn expr_calls_setter(expr: &Expression) -> bool {
    if is_setter_call(expr) {
        return true;
    }
    match expr {
        Expression::SequenceExpression(seq) => {
            seq.expressions.iter().any(|e| expr_calls_setter(e))
        }
        Expression::ConditionalExpression(cond) => {
            expr_calls_setter(&cond.consequent) || expr_calls_setter(&cond.alternate)
        }
        // Don't descend into nested functions.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect"])
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
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name != "useEffect" {
            return;
        }
        if call.arguments.len() != 2 {
            return;
        }

        // Second arg must be an empty array.
        let Some(deps_expr) = call.arguments[1].as_expression() else { return };
        let Expression::ArrayExpression(deps_arr) = deps_expr else {
            return;
        };
        if !deps_arr.elements.is_empty() {
            return;
        }

        // First arg must be arrow/function with a body that calls a setter.
        let Some(callback_expr) = call.arguments[0].as_expression() else { return };
        let has_setter = match callback_expr {
            Expression::ArrowFunctionExpression(arrow) => {
                body_calls_setter(&arrow.body.statements)
            }
            Expression::FunctionExpression(func) => {
                func.body.as_ref().is_some_and(|b| body_calls_setter(&b.statements))
            }
            _ => return,
        };

        if !has_setter {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useEffect` with empty deps sets state — initialize it in `useState(...)` directly instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    // Regression for #911: a spread as the deps argument made `call.arguments[1].to_expression()` panic.
    #[test]
    fn does_not_panic_on_spread_deps() {
        assert!(run("useEffect(cb, ...deps)").is_empty());
    }

    // Regression for #911: a spread as the callback argument made `call.arguments[0].to_expression()` panic.
    #[test]
    fn does_not_panic_on_spread_callback() {
        assert!(run("useEffect(...args, [])").is_empty());
    }

    // Regression for #1954: `setInterval` matches `set` + uppercase but is a timer, not a setter.
    #[test]
    fn ignores_set_interval_timer() {
        let src = r#"
function App() {
  const [okay, setOkay] = useState(true);
  useEffect(() => {
    const interval = setInterval(() => setOkay((okay) => !okay), 1000);
    return () => clearInterval(interval);
  }, []);
  return <div />;
}
"#;
        assert!(run(src).is_empty());
    }

    // Regression for #1954: `setTimeout` matches `set` + uppercase but is a timer, not a setter.
    #[test]
    fn ignores_set_timeout_timer() {
        let src = r#"
function App() {
  useEffect(() => {
    setTimeout(() => setSecondScene(true), 500);
  }, []);
  return <div />;
}
"#;
        assert!(run(src).is_empty());
    }

    // A direct setter call in the effect body remains the genuine antipattern.
    #[test]
    fn flags_direct_setter_in_effect_body() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(1);
  }, []);
  return <div />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }
}
