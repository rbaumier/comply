//! ts-no-implicit-any-catch OXC backend — flag `catch (e)` without a type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // The suggested fix — `catch (e: unknown)` — is TypeScript-only syntax.
        // In a plain-JS file (`.js`/`.jsx`/`.mjs`/`.cjs`) a typed catch binding
        // is a syntax error, so the rule applies to TypeScript-language files
        // only. `.jsx` lands in the shared `Tsx` language bucket, so this guard
        // (not just backend registration) is what excludes it.
        if !crate::rules::path_utils::is_typescript_language_file(ctx.path) {
            return;
        }
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };
        let Some(handler) = &try_stmt.handler else { return };
        let Some(param) = &handler.param else {
            // `catch { ... }` — no binding, nothing to annotate.
            return;
        };
        // If the catch parameter has a type annotation, it's fine.
        if param.type_annotation.is_some() {
            return;
        }
        // Under `useUnknownInCatchVariables` — its own compiler option, or
        // implied by `strict` since TypeScript 4.4 — an un-annotated catch
        // binding is typed `unknown`, not `any`. The rule's premise (an implicit
        // `any` that silently disables type checking) then does not hold, so the
        // binding needs no explicit annotation.
        if ctx.project.uses_unknown_in_catch_variables(ctx.path) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, param.pattern.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "catch binding has no type annotation — it defaults to `any`. \
                      Use `catch (e: unknown)` and narrow the value explicitly."
                .into(),
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

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    /// Run the rule against `source` inside a TempDir that also holds a
    /// `tsconfig.json` with the given `compiler_options` body, so the guard
    /// `uses_unknown_in_catch_variables` resolves against a real tsconfig.
    fn run_with_tsconfig(source: &str, compiler_options: &str) -> Vec<Diagnostic> {
        use crate::files::Language;
        use crate::project::ProjectCtx;
        use crate::rules::file_ctx::FileCtx;

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            format!(r#"{{"compilerOptions":{compiler_options}}}"#),
        )
        .unwrap();
        let file_path = dir.path().join("t.ts");
        std::fs::write(&file_path, source).unwrap();

        let project = ProjectCtx::empty();
        let file = FileCtx::build(&file_path, source, Language::TypeScript, &project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &file_path, &project, &file)
    }

    const UNTYPED_CATCH: &str = r#"
try {
    doWork();
} catch (e) {
    handle(e);
}
"#;

    #[test]
    fn flags_untyped_catch_in_typescript() {
        let diags = run_on_path(UNTYPED_CATCH, "t.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("type annotation"));
    }

    #[test]
    fn allows_typed_catch_in_typescript() {
        let source = r#"
try {
    doWork();
} catch (e: unknown) {
    handle(e);
}
"#;
        assert!(run_on_path(source, "t.ts").is_empty());
    }

    #[test]
    fn allows_bindingless_catch() {
        let source = r#"
try {
    doWork();
} catch {
    handle();
}
"#;
        assert!(run_on_path(source, "t.ts").is_empty());
    }

    // Issue #5621: `catch (e: unknown)` is TypeScript-only syntax, so the rule
    // must not fire in plain-JavaScript files (`.js`/`.jsx`/`.mjs`/`.cjs`) where
    // the suggested fix is a syntax error.
    #[test]
    fn skips_untyped_catch_in_plain_javascript_files() {
        for path in ["app.js", "app.jsx", "app.mjs", "app.cjs"] {
            assert!(
                run_on_path(UNTYPED_CATCH, path).is_empty(),
                "{path} (plain JS) must not be flagged",
            );
        }
    }

    #[test]
    fn still_flags_untyped_catch_in_typescript_files() {
        for path in ["app.ts", "app.tsx", "app.mts", "app.cts"] {
            assert_eq!(
                run_on_path(UNTYPED_CATCH, path).len(),
                1,
                "{path} (TypeScript) must still be flagged",
            );
        }
    }

    // Issue #7447: under a tsconfig that enables `strict` (which implies
    // `useUnknownInCatchVariables` since TS 4.4), the binding is `unknown`, not
    // `any`, so the rule must not fire.
    #[test]
    fn allows_untyped_catch_under_strict_tsconfig() {
        assert!(run_with_tsconfig(UNTYPED_CATCH, r#"{"strict":true}"#).is_empty());
    }

    // An explicit `useUnknownInCatchVariables: true` under a non-strict config
    // still types the binding `unknown`, so the rule must not fire.
    #[test]
    fn allows_untyped_catch_under_explicit_use_unknown() {
        assert!(run_with_tsconfig(UNTYPED_CATCH, r#"{"useUnknownInCatchVariables":true}"#).is_empty());
    }

    // Without `strict` and without the option, the binding defaults to `any` —
    // the true positive must still fire.
    #[test]
    fn flags_untyped_catch_under_non_strict_tsconfig() {
        assert_eq!(run_with_tsconfig(UNTYPED_CATCH, r#"{"strict":false}"#).len(), 1);
    }

    // An explicit `useUnknownInCatchVariables: false` opts out of the `unknown`
    // default even under `strict`, so the binding is `any` and must be flagged.
    #[test]
    fn flags_untyped_catch_when_use_unknown_explicitly_disabled_under_strict() {
        assert_eq!(
            run_with_tsconfig(
                UNTYPED_CATCH,
                r#"{"strict":true,"useUnknownInCatchVariables":false}"#
            )
            .len(),
            1
        );
    }
}
