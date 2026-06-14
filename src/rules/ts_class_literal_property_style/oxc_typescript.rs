//! ts-class-literal-property-style OXC backend — default "fields" mode:
//! flag getter methods that return a literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, Expression, MethodDefinitionKind, PropertyKey, Statement};
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

/// Whether a computed property key is a member access on the global `Symbol`,
/// e.g. `[Symbol.toStringTag]` or `[Symbol.iterator]`. Such getters define a
/// well-known symbol on the prototype; rewriting them as instance `readonly`
/// fields changes lookup semantics, so they must not be flagged.
fn is_symbol_member_key(key: &PropertyKey) -> bool {
    let PropertyKey::StaticMemberExpression(member) = key else {
        return false;
    };
    matches!(&member.object, Expression::Identifier(id) if id.name == "Symbol")
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
            if is_symbol_member_key(&method.key) {
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

    #[test]
    fn allows_computed_symbol_to_string_tag_getter() {
        let diags = run_on(
            r#"
class FakeGraphQLObjectType {
    get [Symbol.toStringTag]() {
        return 'GraphQLObjectType';
    }
}
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_computed_symbol_iterator_getter() {
        let diags = run_on(
            r#"
class Foo {
    get [Symbol.iterator]() { return "x"; }
}
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_plain_named_literal_getter() {
        let diags = run_on(
            r#"
class Foo {
    get foo() { return 1; }
}
"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("readonly"));
    }
}
