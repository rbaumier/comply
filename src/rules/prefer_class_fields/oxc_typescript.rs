use std::sync::Arc;

use oxc_ast::ast::{AssignmentOperator, Expression};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

/// Returns true if the expression is a literal value (string, number,
/// boolean, null, undefined, template literal with no substitutions).
fn is_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
    ) || matches!(expr, Expression::Identifier(id) if id.name == "undefined")
        // An interpolated template (`${…}`) is a runtime-dynamic value that
        // cannot become a class-field initializer; only substitution-free
        // templates are static.
        || matches!(expr, Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty())
}

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
        // Find the constructor method.
        for element in &class.body.body {
            let oxc_ast::ast::ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != oxc_ast::ast::MethodDefinitionKind::Constructor {
                continue;
            }
            if method.r#static {
                continue;
            }
            let Some(func) = method.value.body.as_ref() else {
                continue;
            };

            // Scan statements for `this.x = <literal>`.
            for stmt in &func.statements {
                let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) = stmt else {
                    continue;
                };
                let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                    continue;
                };
                // Must be simple `=` assignment.
                if assign.operator != AssignmentOperator::Assign {
                    continue;
                }
                // Left must be `this.something` (static member).
                let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) =
                    &assign.left
                else {
                    continue;
                };
                let Expression::ThisExpression(_) = &member.object else {
                    continue;
                };
                // Right must be a literal.
                if !is_literal(&assign.right) {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer a class field declaration over `this` assignment in constructor for static values.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
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
    fn allows_interpolated_template() {
        // #7092: an interpolated template depends on a constructor param and
        // cannot be a class-field initializer → dynamic, not flagged.
        let src = "class Foo { name: string; constructor(prefix: string) { this.name = `${prefix}Error`; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_static_template() {
        let src = "class Foo { name: string; constructor() { this.name = `StaticValue`; } }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-class-fields");
    }

    #[test]
    fn flags_string_literal() {
        let d = run_on("class Foo { constructor() { this.x = \"literal\"; } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_numeric_literal() {
        let d = run_on("class Foo { constructor() { this.x = 5; } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_function_call() {
        assert!(run_on("class Foo { constructor() { this.x = someFn(); } }").is_empty());
    }
}
