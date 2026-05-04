use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, TSType};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameter(param) = node.kind() else {
            return;
        };
        // Check if the type annotation is an object type literal (TSTypeLiteral).
        let Some(annotation) = &param.type_annotation else {
            return;
        };
        if !matches!(annotation.type_annotation, TSType::TSTypeLiteral(_)) {
            return;
        }
        let name = match &param.pattern {
            BindingPattern::BindingIdentifier(id) => id.name.as_str(),
            _ => "<param>",
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Parameter '{name}' has an inline object type — extract \
                 it to a named `type` declaration above the function so \
                 the shape has an identity and can't silently drift."
            ),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_inline_object_param() {
        assert_eq!(
            run_on("function f(opts: { name: string; age: number }) {}").len(),
            1
        );
    }

    #[test]
    fn allows_named_type_param() {
        assert!(run_on("function f(opts: UserOptions) {}").is_empty());
    }

    #[test]
    fn allows_primitive_type_param() {
        assert!(run_on("function f(name: string) {}").is_empty());
    }

    #[test]
    fn flags_inline_on_arrow_function() {
        assert_eq!(
            run_on("const f = (opts: { a: number }) => opts.a;").len(),
            1
        );
    }
}
