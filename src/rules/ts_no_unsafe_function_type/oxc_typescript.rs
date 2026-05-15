//! ts-no-unsafe-function-type oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSTypeName;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeReference]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeReference(type_ref) = node.kind() else {
            return;
        };
        let name = match &type_ref.type_name {
            TSTypeName::IdentifierReference(id) => id.name.as_str(),
            _ => return,
        };
        if name != "Function" {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, type_ref.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Built-in `Function` type loses signature info — replace with \
                      a precise call signature like `(arg: T) => U`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_function_type_annotation() {
        let src = "function call(cb: Function) { cb(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_typed_callback() {
        let src = "function call(cb: () => void) { cb(); }";
        assert!(run(src).is_empty());
    }
}
