//! react-no-setstate-without-updater OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Extract `(state_name, setter_name, callee_is_bare_identifier)` from a
/// `useState` variable declarator. `callee_is_bare_identifier` is true for
/// `useState(...)` and false for the namespaced `React.useState(...)` form.
fn extract_usestate(decl: &oxc_ast::ast::VariableDeclarator) -> Option<(String, String, bool)> {
    let init = decl.init.as_ref()?;
    let Expression::CallExpression(call) = init else {
        return None;
    };
    let callee_is_bare_identifier = match &call.callee {
        Expression::Identifier(id) => {
            if id.name != "useState" {
                return None;
            }
            true
        }
        Expression::StaticMemberExpression(mem) => {
            if mem.property.name != "useState" {
                return None;
            }
            false
        }
        _ => return None,
    };
    let oxc_ast::ast::BindingPattern::ArrayPattern(arr) = &decl.id else {
        return None;
    };
    let elems: Vec<_> = arr.elements.iter().flatten().collect();
    if elems.len() < 2 {
        return None;
    }
    let oxc_ast::ast::BindingPattern::BindingIdentifier(state_id) = elems[0] else {
        return None;
    };
    let oxc_ast::ast::BindingPattern::BindingIdentifier(setter_id) = elems[1] else {
        return None;
    };
    Some((
        state_id.name.to_string(),
        setter_id.name.to_string(),
        callee_is_bare_identifier,
    ))
}

/// Check if an expression references the given identifier name (recursively),
/// but NOT inside arrow/function expressions.
fn references_name(expr: &Expression, name: &str) -> bool {
    match expr {
        Expression::Identifier(id) => id.name == name,
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,
        Expression::BinaryExpression(bin) => {
            references_name(&bin.left, name) || references_name(&bin.right, name)
        }
        Expression::UnaryExpression(un) => references_name(&un.argument, name),
        Expression::UpdateExpression(up) => {
            if let oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &up.argument {
                id.name == name
            } else {
                false
            }
        }
        Expression::ConditionalExpression(cond) => {
            references_name(&cond.test, name)
                || references_name(&cond.consequent, name)
                || references_name(&cond.alternate, name)
        }
        Expression::CallExpression(call) => {
            references_name(&call.callee, name)
                || call.arguments.iter().any(|arg| match arg {
                    oxc_ast::ast::Argument::SpreadElement(s) => {
                        references_name(&s.argument, name)
                    }
                    _ => {
                        references_name(arg.to_expression(), name)
                    }
                })
        }
        Expression::ArrayExpression(arr) => arr.elements.iter().any(|el| match el {
            oxc_ast::ast::ArrayExpressionElement::SpreadElement(s) => {
                references_name(&s.argument, name)
            }
            oxc_ast::ast::ArrayExpressionElement::Elision(_) => false,
            _ => references_name(el.to_expression(), name),
        }),
        Expression::ObjectExpression(obj) => obj.properties.iter().any(|prop| match prop {
            oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) => {
                references_name(&p.value, name)
            }
            oxc_ast::ast::ObjectPropertyKind::SpreadProperty(s) => {
                references_name(&s.argument, name)
            }
        }),
        Expression::StaticMemberExpression(mem) => references_name(&mem.object, name),
        Expression::ComputedMemberExpression(mem) => {
            references_name(&mem.object, name) || references_name(&mem.expression, name)
        }
        Expression::TemplateLiteral(tpl) => {
            tpl.expressions.iter().any(|e| references_name(e, name))
        }
        Expression::LogicalExpression(log) => {
            references_name(&log.left, name) || references_name(&log.right, name)
        }
        Expression::AssignmentExpression(assign) => references_name(&assign.right, name),
        Expression::SequenceExpression(seq) => {
            seq.expressions.iter().any(|e| references_name(e, name))
        }
        Expression::ParenthesizedExpression(p) => references_name(&p.expression, name),
        Expression::TSAsExpression(ts) => references_name(&ts.expression, name),
        Expression::TSNonNullExpression(ts) => references_name(&ts.expression, name),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };
        let Some((state_name, setter_name, callee_is_bare_identifier)) = extract_usestate(decl)
        else {
            return;
        };

        // A bare `useState(...)` is React's only when `import { useState } from "react"`.
        // Skip a `useState` bound to anything else (Hono's `../../hooks`, Preact's
        // `preact/hooks`, a local function); the namespaced `React.useState(...)` form
        // is already React-scoped and keeps firing.
        if callee_is_bare_identifier
            && !crate::oxc_helpers::is_imported_from_react("useState", semantic)
        {
            return;
        }

        // Find the enclosing function body and scan for setter calls.
        let mut current = node.id();
        loop {
            let parent_id = semantic.nodes().parent_id(current);
            if parent_id == current {
                return;
            }
            current = parent_id;
            let parent_node = semantic.nodes().get_node(current);
            match parent_node.kind() {
                AstKind::Function(func) => {
                    if let Some(body) = &func.body {
                        scan_stmts_for_setter(
                            &body.statements,
                            &state_name,
                            &setter_name,
                            ctx,
                            diagnostics,
                        );
                    }
                    return;
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    scan_stmts_for_setter(
                        &arrow.body.statements,
                        &state_name,
                        &setter_name,
                        ctx,
                        diagnostics,
                    );
                    return;
                }
                _ => continue,
            }
        }
    }
}

fn scan_stmts_for_setter(
    stmts: &[oxc_ast::ast::Statement],
    state_name: &str,
    setter_name: &str,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        scan_stmt_for_setter(stmt, state_name, setter_name, ctx, diagnostics);
    }
}

fn scan_stmt_for_setter(
    stmt: &oxc_ast::ast::Statement,
    state_name: &str,
    setter_name: &str,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        oxc_ast::ast::Statement::ExpressionStatement(expr) => {
            check_expr_for_setter(&expr.expression, state_name, setter_name, ctx, diagnostics);
        }
        oxc_ast::ast::Statement::VariableDeclaration(decl) => {
            for d in &decl.declarations {
                if let Some(init) = &d.init {
                    check_expr_for_setter(init, state_name, setter_name, ctx, diagnostics);
                }
            }
        }
        oxc_ast::ast::Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                check_expr_for_setter(arg, state_name, setter_name, ctx, diagnostics);
            }
        }
        oxc_ast::ast::Statement::IfStatement(if_stmt) => {
            if let oxc_ast::ast::Statement::BlockStatement(block) = &if_stmt.consequent {
                scan_stmts_for_setter(&block.body, state_name, setter_name, ctx, diagnostics);
            }
            if let Some(alt) = &if_stmt.alternate {
                scan_stmt_for_setter(alt, state_name, setter_name, ctx, diagnostics);
            }
        }
        oxc_ast::ast::Statement::BlockStatement(block) => {
            scan_stmts_for_setter(&block.body, state_name, setter_name, ctx, diagnostics);
        }
        _ => {}
    }
}

fn check_expr_for_setter(
    expr: &Expression,
    state_name: &str,
    setter_name: &str,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::Identifier(callee) = &call.callee
                && callee.name == setter_name && !call.arguments.is_empty() {
                    let first_arg = &call.arguments[0];
                    let Some(arg_expr) = first_arg.as_expression() else { return };
                    // If the argument is an arrow/function, that's the correct updater form.
                    if matches!(
                        arg_expr,
                        Expression::ArrowFunctionExpression(_)
                            | Expression::FunctionExpression(_)
                    ) {
                        return;
                    }
                    if references_name(arg_expr, state_name) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, call.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{setter_name}` called with an expression referencing `{state_name}` — \
                                 use the functional updater: `{setter_name}(prev => ...)`."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
        }
        Expression::ArrowFunctionExpression(arrow) => {
            // Walk into arrow bodies (event handlers like `() => setCount(count + 1)`)
            for stmt in &arrow.body.statements {
                scan_stmt_for_setter(stmt, state_name, setter_name, ctx, diagnostics);
            }
        }
        _ => {}
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

    // Regression for #911: a spread argument to the setter made `Argument::to_expression()` panic.
    #[test]
    fn does_not_panic_on_spread_arg_to_setter() {
        let src = "import { useState } from 'react';\nfunction C() { const [x, setX] = useState(0); setX(...args); }";
        assert!(run(src).is_empty());
    }

    // A setter called with an expression referencing its own state remains the
    // genuine antipattern.
    #[test]
    fn flags_setter_referencing_own_state() {
        let src = r#"
import { useState } from 'react';
function App() {
  const [count, setCount] = useState(0);
  const inc = () => setCount(count + 1);
  return <div />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #3254: Hono's hook runtime imports `useState` from a relative
    // path; its internal `setValues([values[0], value])` is not a React setter and
    // must not be flagged.
    #[test]
    fn skips_usestate_imported_from_hono_hooks() {
        let src = r#"
import { useState } from '../../hooks';
function App() {
  const [count, setCount] = useState(0);
  const inc = () => setCount(count + 1);
  return <div />;
}
"#;
        assert!(run(src).is_empty());
    }

    // Regression for #3254: Preact's `useState` (preact/hooks) is not React's.
    #[test]
    fn skips_usestate_imported_from_preact() {
        let src = r#"
import { useState } from 'preact/hooks';
function App() {
  const [count, setCount] = useState(0);
  const inc = () => setCount(count + 1);
  return <div />;
}
"#;
        assert!(run(src).is_empty());
    }
}
