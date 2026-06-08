//! public-static-readonly oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::PropertyDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::PropertyDefinition(prop) = node.kind() else { return };

        // Must have an initializer (field with `=`), not a method
        if prop.value.is_none() {
            return;
        }

        let text = &ctx.source[prop.span.start as usize..prop.span.end as usize];

        let has_public_static =
            text.contains("public static") || text.contains("static public");
        if !has_public_static {
            return;
        }

        if text.contains("readonly") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`public static` field is missing `readonly` \u{2014} add it to prevent mutation."
                .into(),
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
    fn flags_public_static_without_readonly() {
        let src = "class C { public static MAX = 100; }";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_static_public_without_readonly() {
        let src = "class C { static public MAX = 100; }";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_public_static_readonly() {
        let src = "class C { public static readonly MAX = 100; }";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_public_static_method() {
        let src = "class C { public static getInstance() { return new C(); } }";
        assert!(run_on(src).is_empty());
    }
}
