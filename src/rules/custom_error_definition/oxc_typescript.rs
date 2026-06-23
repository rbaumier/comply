//! custom-error-definition OXC backend — flag `this.name = 'X'` and
//! `this.message = ...` in Error subclass constructors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, Expression, MethodDefinitionKind, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_error_name(name: &str) -> bool {
    name.ends_with("Error") && name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// True for an `Object.setPrototypeOf(this, ...)` call expression — the
/// documented ES5-compatible custom-error idiom that restores the prototype
/// chain after `super()`. When present in a constructor, `this.name = '...'`
/// is the intentional companion (a `name = '...'` class field needs ES2022),
/// so the rule must not flag it.
fn is_set_prototype_of_this(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(object) = &member.object else {
        return false;
    };
    if object.name != "Object" || member.property.name != "setPrototypeOf" {
        return false;
    }
    matches!(
        call.arguments.first().and_then(oxc_ast::ast::Argument::as_expression),
        Some(Expression::ThisExpression(_))
    )
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };

        // Must extend something that looks like an Error.
        let Some(super_class) = &class.super_class else {
            return;
        };
        let super_name = match super_class {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !is_error_name(super_name) {
            return;
        }

        // Get class name.
        let Some(class_id) = &class.id else {
            return;
        };
        let class_name = class_id.name.as_str();

        // Find the constructor.
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != MethodDefinitionKind::Constructor {
                continue;
            }
            let Some(func_body) = &method.value.body else {
                continue;
            };

            // ES5-compatible custom-error pattern: an `Object.setPrototypeOf(this, …)`
            // call in the constructor means `this.name = '…'` is the intentional
            // companion (the `name = '…'` class field requires ES2022).
            let has_es5_prototype_fix = func_body.statements.iter().any(|stmt| {
                let Statement::ExpressionStatement(s) = stmt else {
                    return false;
                };
                let Expression::CallExpression(call) = &s.expression else {
                    return false;
                };
                is_set_prototype_of_this(call)
            });

            // Walk statements in the constructor body.
            for stmt in &func_body.statements {
                let Statement::ExpressionStatement(expr_stmt) = stmt else {
                    continue;
                };
                let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                    continue;
                };
                // Get the left-hand side text from source.
                let left_span = assign.left.span();
                let left_text =
                    &ctx.source[left_span.start as usize..left_span.end as usize];

                if left_text == "this.name" && !has_es5_prototype_fix {
                    let (line, column) = byte_offset_to_line_col(
                        ctx.source,
                        expr_stmt.span.start as usize,
                    );
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Use a class field `name = '{class_name}';` instead \
                             of setting `this.name` in the constructor."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }

                if left_text == "this.message" {
                    let (line, column) = byte_offset_to_line_col(
                        ctx.source,
                        expr_stmt.span.start as usize,
                    );
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Pass the error message to `super()` instead \
                                  of setting `this.message`."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_this_name_in_error_constructor() {
        let d = run_on(
            "class MyError extends Error {\n\
             constructor(m: string) { super(m); this.name = 'MyError'; }\n\
             }",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "custom-error-definition");
    }

    #[test]
    fn allows_this_name_with_es5_set_prototype_of() {
        // #5858: the documented ES5-compatible custom-error idiom — `this.name`
        // is required alongside `Object.setPrototypeOf(this, X.prototype)`.
        let d = run_on(
            "class MyError extends Error {\n\
             constructor(m: string) {\n\
             super(m);\n\
             Object.setPrototypeOf(this, MyError.prototype);\n\
             this.name = 'MyError';\n\
             }\n\
             }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_this_message_even_with_set_prototype_of() {
        // The ES5 prototype fix justifies only `this.name`, never `this.message`.
        let d = run_on(
            "class MyError extends Error {\n\
             constructor(m: string) {\n\
             super();\n\
             Object.setPrototypeOf(this, MyError.prototype);\n\
             this.message = m;\n\
             }\n\
             }",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("this.message"), "got {:?}", d[0].message);
    }

    #[test]
    fn ignores_set_prototype_of_without_this() {
        // A `setPrototypeOf` call on something other than `this` is not the
        // ES5 idiom and must not suppress the `this.name` flag.
        let d = run_on(
            "class MyError extends Error {\n\
             constructor(m: string) {\n\
             super(m);\n\
             Object.setPrototypeOf(other, MyError.prototype);\n\
             this.name = 'MyError';\n\
             }\n\
             }",
        );
        assert_eq!(d.len(), 1);
    }
}
