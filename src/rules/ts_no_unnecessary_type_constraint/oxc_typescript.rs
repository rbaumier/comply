//! ts-no-unnecessary-type-constraint oxc backend — flag `<T extends any>` or
//! `<T extends unknown>`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeParameter(param) = node.kind() else { return };
        let Some(constraint) = &param.constraint else { return };
        let keyword = match constraint {
            TSType::TSAnyKeyword(_) => "any",
            TSType::TSUnknownKeyword(_) => "unknown",
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, constraint.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Unnecessary `extends {keyword}` constraint — \
                 all types already extend `{keyword}`."
            ),
            severity: Severity::Warning,
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
    fn flags_extends_any() {
        let diags = run_on("function f<T extends any>(x: T): T { return x; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`any`"));
    }


    #[test]
    fn flags_extends_unknown() {
        let diags = run_on("function f<T extends unknown>(x: T): T { return x; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`unknown`"));
    }


    #[test]
    fn allows_extends_string() {
        assert!(run_on("function f<T extends string>(x: T): T { return x; }").is_empty());
    }
}
