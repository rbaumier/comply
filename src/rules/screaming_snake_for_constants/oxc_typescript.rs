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

            if is_function_or_class_init(declarator) {
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

fn is_function_or_class_init(declarator: &oxc_ast::ast::VariableDeclarator) -> bool {
    let Some(init) = &declarator.init else {
        return false;
    };
    matches!(
        init,
        oxc_ast::ast::Expression::ArrowFunctionExpression(_)
            | oxc_ast::ast::Expression::FunctionExpression(_)
            | oxc_ast::ast::Expression::ClassExpression(_)
    )
}
