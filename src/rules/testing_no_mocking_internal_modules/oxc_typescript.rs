//! testing-no-mocking-internal-modules OXC backend — detect `vi.mock`/`jest.mock`
//! calls whose first argument is a relative path (`./` or `../`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jest.mock", "vi.mock"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // `test/internal/` tests deliberately exercise internal implementation
        // details, so mocking internal modules there is intentional (unlike
        // `test/public/` tests that verify the public API contract).
        if ctx.file.path_segments.in_test_internal_dir {
            return;
        }

        // Callee must be vi.mock or jest.mock
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "mock" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        let obj_name = obj.name.as_str();
        if obj_name != "vi" && obj_name != "jest" {
            return;
        }

        // First argument must be a string literal starting with "./" or "../"
        let Some(first_arg) = call.arguments.first() else { return };
        let raw = &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        let path = unquote(raw);

        if path.starts_with("./") || path.starts_with("../") {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, first_arg.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Mocking internal module '{path}' couples tests to implementation details — mock boundaries, not internals."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Language;
    use crate::rules::file_ctx::FileCtx;

    fn run(s: &str, path: &str) -> Vec<Diagnostic> {
        let lang = Language::from_path(std::path::Path::new(path)).unwrap_or(Language::TypeScript);
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(std::path::Path::new(path), s, lang, project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, s, path, project, &file)
    }

    #[test]
    fn flags_vi_mock_relative_in_plain_test_file() {
        assert_eq!(run("vi.mock('../utils/helpers');", "test/foo.spec.ts").len(), 1);
    }

    #[test]
    fn flags_vi_mock_relative_in_public_test_dir() {
        // `test/public/` verifies the public API contract — still flagged.
        assert_eq!(run("vi.mock('../../src/service.js');", "test/public/foo.spec.ts").len(), 1);
    }

    #[test]
    fn allows_mocking_external_package() {
        assert!(run("vi.mock('axios');", "test/foo.spec.ts").is_empty());
    }

    #[test]
    fn allows_internal_mock_in_test_internal_dir_issue1150() {
        // `test/internal/` tests deliberately mock internal modules. (Closes #1150)
        assert!(
            run(
                "vi.mock('../../../src/util/userAgentPlatform.js', async () => ({}));",
                "sdk/core/core-rest-pipeline/test/internal/node/userAgent.spec.ts"
            )
            .is_empty()
        );
    }
}
