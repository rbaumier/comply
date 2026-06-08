//! import-dynamic-import-chunkname oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["import("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("webpack") {
            return;
        }

        let AstKind::ImportExpression(import) = node.kind() else { return };

        let call_text = &ctx.source[import.span.start as usize..import.span.end as usize];
        if call_text.contains("webpackChunkName") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Dynamic imports require a leading comment with the webpack chunkname.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "webpack")
    }


    #[test]
    fn flags_missing_chunkname() {
        let d = run_on("const Foo = import('./foo');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("chunkname"));
    }


    #[test]
    fn allows_chunkname_comment() {
        let src = r#"const Foo = import(/* webpackChunkName: "foo" */ './foo');"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_wrong_comment() {
        let d = run_on("const Foo = import(/* some comment */ './foo');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn ignores_non_webpack_projects() {
        let d = crate::rules::test_helpers::run_oxc_ts("const Foo = import('./foo');", &Check);
        assert!(
            d.is_empty(),
            "webpack-only rule must be silent without webpack"
        );
    }
}
