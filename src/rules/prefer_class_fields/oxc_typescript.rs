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
            | Expression::TemplateLiteral(_)
    ) || matches!(expr, Expression::Identifier(id) if id.name == "undefined")
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
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_this_string_literal_in_constructor() {
        let code = r#"
class Foo {
    constructor() {
        this.name = 'hello';
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-class-fields");
    }


    #[test]
    fn flags_this_number_literal_in_constructor() {
        let code = "class Foo { constructor() { this.count = 0; } }";
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_this_boolean_literal_in_constructor() {
        let code = "class Foo { constructor() { this.active = true; } }";
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_this_with_non_literal() {
        let code = "class Foo { constructor(name) { this.name = name; } }";
        assert!(run_on(code).is_empty());
    }


    #[test]
    fn allows_this_with_function_call() {
        let code = "class Foo { constructor() { this.id = generateId(); } }";
        assert!(run_on(code).is_empty());
    }


    #[test]
    fn allows_class_field_declaration() {
        let code = "class Foo { name = 'hello'; }";
        assert!(run_on(code).is_empty());
    }


    #[test]
    fn flags_multiple_literal_assignments() {
        let code = r#"
class Foo {
    constructor() {
        this.a = 1;
        this.b = 'two';
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 2);
    }


    #[test]
    fn allows_compound_assignment() {
        let code = "class Foo { constructor() { this.count += 1; } }";
        assert!(run_on(code).is_empty());
    }
}
