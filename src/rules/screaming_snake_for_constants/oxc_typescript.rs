use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["const "])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        if !decl.kind.is_const() {
            return;
        }

        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::Program(_) | AstKind::ExportNamedDeclaration(_)) {
            return;
        }

        for declarator in &decl.declarations {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };

            let name = id.name.as_str();

            if !is_primitive_init(declarator) {
                continue;
            }

            if super::is_screaming_snake(name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Top-level constant `{name}` is not in `SCREAMING_SNAKE_CASE`."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn is_primitive_init(declarator: &oxc_ast::ast::VariableDeclarator) -> bool {
    let Some(init) = &declarator.init else {
        return false;
    };
    matches!(
        init,
        oxc_ast::ast::Expression::NumericLiteral(_)
            | oxc_ast::ast::Expression::BooleanLiteral(_)
    ) || is_unary_numeric(init)
        || is_array_of_literals(init)
}

fn is_unary_numeric(expr: &oxc_ast::ast::Expression) -> bool {
    if let oxc_ast::ast::Expression::UnaryExpression(u) = expr {
        return matches!(u.argument, oxc_ast::ast::Expression::NumericLiteral(_));
    }
    false
}

fn is_array_of_literals(expr: &oxc_ast::ast::Expression) -> bool {
    let oxc_ast::ast::Expression::ArrayExpression(arr) = expr else {
        return false;
    };
    arr.elements.iter().all(|el| {
        matches!(
            el,
            oxc_ast::ast::ArrayExpressionElement::NumericLiteral(_)
                | oxc_ast::ast::ArrayExpressionElement::StringLiteral(_)
                | oxc_ast::ast::ArrayExpressionElement::BooleanLiteral(_)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::screaming_snake_for_constants::oxc_typescript::Check;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_camel_case_numeric() {
        let diags = run("const maxRetries = 3;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetries"));
    }


    #[test]
    fn allows_string_constant() {
        assert!(run("const apiUrl = \"https://example.com\";").is_empty());
    }


    #[test]
    fn allows_screaming_snake() {
        assert!(run("const MAX_RETRIES = 3;").is_empty());
    }


    #[test]
    fn allows_function_assignment() {
        assert!(run("const handleClick = () => {};").is_empty());
    }


    #[test]
    fn allows_call_expression() {
        assert!(run("const errorReporter = createReporter();").is_empty());
    }


    #[test]
    fn allows_object_literal() {
        assert!(run("const config = { a: 1 };").is_empty());
    }


    #[test]
    fn allows_local_const() {
        assert!(run("function f() { const localVar = 1; }").is_empty());
    }


    #[test]
    fn flags_exported_camel_case() {
        let diags = run("export const maxRetries = 3;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_exported_screaming_snake() {
        assert!(run("export const MAX_RETRIES = 3;").is_empty());
    }


    #[test]
    fn flags_negative_number() {
        let diags = run("const minValue = -1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_new_expression() {
        assert!(run("const instance = new Map();").is_empty());
    }
}
