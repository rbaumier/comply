//! next-prefer-next-env oxc backend — flag member access ending in `__NEXT_DATA__`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["__NEXT_DATA__"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.project.framework != Framework::NextJs {
            return;
        }

        let AstKind::StaticMemberExpression(member) = node.kind() else { return };

        if member.property.name.as_str() != "__NEXT_DATA__" {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Don't read `__NEXT_DATA__`. Use `process.env.NEXT_PUBLIC_*` for build-time configuration.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;



    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }


    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(
            source,
            &Check,
            project)
    }


    #[test]
    fn flags_window_next_data() {
        let src = "const v = window.__NEXT_DATA__.props;";
        assert_eq!(run(src, &next_project()).len(), 1);
    }


    #[test]
    fn allows_process_env_next_public() {
        let src = "const v = process.env.NEXT_PUBLIC_API_URL;";
        assert!(run(src, &next_project()).is_empty());
    }


    #[test]
    fn ignores_non_nextjs_project() {
        let src = "const v = window.__NEXT_DATA__;";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
