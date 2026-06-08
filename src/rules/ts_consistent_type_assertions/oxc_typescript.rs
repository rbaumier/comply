//! ts-consistent-type-assertions oxc backend — flag angle-bracket `<T>expr`
//! in favour of `expr as T`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAssertion]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAssertion(assertion) = node.kind() else { return };

        // Ignore `<const>` assertions — they are idiomatic.
        let type_text = &ctx.source
            [assertion.type_annotation.span().start as usize..assertion.type_annotation.span().end as usize];
        if type_text.trim() == "const" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assertion.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Use `as {type_text}` instead of `<{type_text}>`.",),
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
    fn flags_angle_bracket_assertion() {
        let diags = run_on("const x = <string>value;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("as"));
    }


    #[test]
    fn allows_as_assertion() {
        let diags = run_on("const x = value as string;");
        assert!(diags.is_empty());
    }
}
