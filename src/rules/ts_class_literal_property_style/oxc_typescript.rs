//! ts-class-literal-property-style OXC backend — default "fields" mode:
//! flag getter methods that return a literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, Expression, MethodDefinitionKind, Statement};
use std::sync::Arc;

pub struct Check;

fn is_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::TemplateLiteral(_)
    )
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

        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != MethodDefinitionKind::Get {
                continue;
            }
            let Some(body) = &method.value.body else {
                continue;
            };
            // Must have exactly one statement: a return statement
            if body.statements.len() != 1 {
                continue;
            }
            let Statement::ReturnStatement(ret) = &body.statements[0] else {
                continue;
            };
            let Some(arg) = &ret.argument else {
                continue;
            };
            if !is_literal(arg) {
                continue;
            }

            let name = method
                .key
                .name()
                .unwrap_or_default();

            let (line, column) =
                byte_offset_to_line_col(ctx.source, method.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Getter `{name}` returns a literal — use a `readonly` field instead."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_getter_returning_string_literal() {
        let diags = run_on(
            r#"
class Foo {
    get name() { return "hello"; }
}
"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("readonly"));
    }

    #[test]
    fn flags_getter_returning_number_literal() {
        let diags = run_on(
            r#"
class Foo {
    get count() { return 42; }
}
"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_getter_returning_expression() {
        let diags = run_on(
            r#"
class Foo {
    get name() { return this._name; }
}
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_readonly_field() {
        let diags = run_on(
            r#"
class Foo {
    readonly name = "hello";
}
"#,
        );
        assert!(diags.is_empty());
    }
}
