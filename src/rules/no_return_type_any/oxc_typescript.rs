use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

/// True if the type resolves to `any` or `Promise<any>`.
fn resolves_to_any(ty: &TSType) -> bool {
    match ty {
        TSType::TSAnyKeyword(_) => true,
        TSType::TSTypeReference(type_ref) => {
            let name = match &type_ref.type_name {
                oxc_ast::ast::TSTypeName::IdentifierReference(id) => id.name.as_str(),
                _ => return false,
            };
            if name != "Promise" {
                return false;
            }
            let Some(params) = &type_ref.type_arguments else {
                return false;
            };
            params.params.iter().any(|p| resolves_to_any(p))
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let return_type = match node.kind() {
            AstKind::Function(func) => func.return_type.as_ref(),
            AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_ref(),
            _ => return,
        };
        let Some(type_ann) = return_type else { return };
        if !resolves_to_any(&type_ann.type_annotation) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, type_ann.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function has explicit `: any` return type — use a specific type or `unknown`."
                .into(),
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
    fn flags_any_return_function() {
        assert_eq!(run_on("function foo(): any {}").len(), 1);
    }

    #[test]
    fn flags_any_return_arrow() {
        assert_eq!(run_on("const foo = (): any => {};").len(), 1);
    }

    #[test]
    fn flags_promise_any_return() {
        assert_eq!(run_on("async function foo(): Promise<any> {}").len(), 1);
    }

    #[test]
    fn allows_specific_return_type() {
        assert!(run_on("function foo(): string {}").is_empty());
    }

    #[test]
    fn allows_unknown_return() {
        assert!(run_on("function foo(): unknown {}").is_empty());
    }
}
