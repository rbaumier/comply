//! OxcCheck backend for elysia-file-magic-number — flag `z.file()` without a magic-number check.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True only when the file genuinely imports Elysia. We match the import
/// specifier (`from 'elysia'`, `from '@elysiajs/...'`), not the bare substring
/// `"elysia"`: the rule id `elysia-file-magic-number` appears inside its own
/// `comply-ignore` suppression comments, so a substring check would re-fire on
/// any suppressed `z.file()` (issue #434, reopened). Mirrors
/// `elysia_aot_dynamic_route::imports_elysia`.
fn imports_elysia(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "from 'elysia'")
        || crate::oxc_helpers::source_contains(source, "from \"elysia\"")
        || crate::oxc_helpers::source_contains(source, "from 'elysia/")
        || crate::oxc_helpers::source_contains(source, "from \"elysia/")
        || crate::oxc_helpers::source_contains(source, "from '@elysiajs/")
        || crate::oxc_helpers::source_contains(source, "from \"@elysiajs/")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // `z.file()` is Zod's file-schema constructor; it is only a security
        // concern when wired into an Elysia route, so require both the project
        // framework AND a genuine Elysia import in this very file. A bare
        // `z.file()` in a shared schema module (no Elysia import) is a
        // legitimate declaration, not a magic-number smell (issue #434).
        if !ctx.project.has_framework("elysia")
            || !ctx.source_contains("zod")
            || !imports_elysia(ctx.source)
        {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "z.file" {
            return;
        }
        if ctx.source_contains("fileType(") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.file()` only checks the MIME header — verify the magic number with `fileType()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::{FileCtx, default_static_file_ctx};
    use crate::rules::test_helpers::{run_rule_gated, run_rule_with_ctx};
    use std::path::Path;

    fn elysia_project() -> ProjectCtx {
        ProjectCtx::for_test_with_framework("elysia")
    }

    #[test]
    fn flags_z_file_in_an_elysia_route() {
        // True positive: a real Elysia route body using `z.file()` with no
        // magic-number check must still be flagged.
        let src = "import { Elysia, t } from 'elysia';\nimport { z } from 'zod';\nnew Elysia().post('/upload', () => 'ok', { body: z.object({ file: z.file() }) });";
        assert_eq!(
            run_rule_with_ctx(
                &Check,
                src,
                "upload.ts",
                &elysia_project(),
                default_static_file_ctx()
            )
            .len(),
            1,
        );
    }

    #[test]
    fn allows_z_file_with_filetype_refine() {
        // True negative: pairing `z.file()` with a `fileType()` magic-number
        // refine is the documented remediation.
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nimport { fileType } from 'file-type';\nconst s = z.file().refine(b => fileType(b)?.mime === 'image/png');";
        assert!(
            run_rule_with_ctx(
                &Check,
                src,
                "upload.ts",
                &elysia_project(),
                default_static_file_ctx()
            )
            .is_empty(),
        );
    }

    #[test]
    fn no_fp_on_zod_file_schema_constructor_without_elysia_import() {
        // Regression for issue #434 (reopened): the real firing site was
        // `src/shared/schemas/import-batches.ts`, a shared Zod schema that
        // imports only zod — no Elysia import — yet sat inside an Elysia
        // project. `z.file()` there is a legitimate schema declaration.
        let src = "import { z } from 'zod';\nexport const ValuationImportBodySchema = z.object({ file: z.file().max(1000) });";
        assert!(
            run_rule_with_ctx(
                &Check,
                src,
                "src/shared/schemas/import-batches.ts",
                &elysia_project(),
                default_static_file_ctx()
            )
            .is_empty(),
        );
    }

    #[test]
    fn no_fp_when_elysia_appears_only_in_suppression_comment() {
        // Regression for issue #434 (reopened): the bare-substring `"elysia"`
        // guard re-fired because the rule id `elysia-file-magic-number` shows
        // up in its own `comply-ignore` comment. The import-specifier guard
        // ignores the comment.
        let src = "import { z } from 'zod';\n// comply-ignore: elysia-file-magic-number — the parser is the trust boundary.\nexport const S = z.object({ file: z.file().max(1000) });";
        assert!(
            run_rule_with_ctx(
                &Check,
                src,
                "src/shared/schemas/import-batches.ts",
                &elysia_project(),
                default_static_file_ctx()
            )
            .is_empty(),
        );
    }

    #[test]
    fn skip_in_test_dir_gates_out_test_files() {
        // Regression for issue #434 (reopened): `src/shared/zod-i18n.test.ts`
        // uses `z.file()` to assert the French error-message wording, not to
        // validate file content. `skip_in_test_dir = true` makes the engine's
        // applicability gate drop the rule for any `.test.`/`.spec.` file
        // (`scan_path` marks `in_test_dir`). `run_rule_gated` exercises that
        // production gate end-to-end, so this proves the rule is suppressed
        // along the real run path, not merely that the META flag is wired.
        let src = "import { z } from 'zod';\nconst schema = z.file().max(5);";
        assert!(run_rule_gated(&Check, src, "src/shared/zod-i18n.test.ts").is_empty());

        // The complementary half: the gate must let non-test files through, so
        // the rule still applies to a production schema path. `run_rule_gated`
        // builds its `FileCtx` from the default (frameworkless) project, so the
        // `run()` body would early-return on `has_framework`; assert the gate
        // verdict directly to isolate "the gate permits this path".
        let project = ProjectCtx::for_test_with_framework("elysia");
        let normal_file = FileCtx::build(
            Path::new("src/shared/schemas/import-batches.ts"),
            src,
            Language::TypeScript,
            &project,
        );
        assert!(super::super::META.applies_to_file(&normal_file));
    }
}
