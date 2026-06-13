use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ExportDefaultDeclarationKind;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportDefaultDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["export default"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Vitest/Playwright setup/teardown hook files are resolved by path and
        // invoked via their default export, so an anonymous default export is
        // the framework convention, not a smell.
        if ctx.file.path_segments.is_framework_hook_file {
            return;
        }
        let AstKind::ExportDefaultDeclaration(export) = node.kind() else {
            return;
        };
        let (is_anon, label) = match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                let has_name = func.id.as_ref().is_some_and(|id| !id.name.is_empty());
                (!has_name, "function")
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                let has_name = class.id.as_ref().is_some_and(|id| !id.name.is_empty());
                (!has_name, "class")
            }
            _ => return,
        };
        if !is_anon {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Anonymous default export {label} — give it a name for \
                 better stack traces and refactoring support."
            ),
            severity: super::META.severity,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, source, path)
    }

    #[test]
    fn flags_anonymous_function() {
        let d = run_on("export default function() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("function"));
    }

    #[test]
    fn flags_anonymous_class() {
        let d = run_on("export default class {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn allows_named_function() {
        assert!(run_on("export default function myFn() {}").is_empty());
    }

    #[test]
    fn allows_named_class() {
        assert!(run_on("export default class MyClass {}").is_empty());
    }

    #[test]
    fn allows_identifier_export() {
        assert!(run_on("export default myVariable;").is_empty());
    }

    #[test]
    fn allows_vitest_global_setup_file_issue1154() {
        // Vitest globalSetup file: anonymous default export by convention.
        let src = "export default async function ({ provide }) {}";
        assert!(
            run_on_path(src, "sdk/servicebus/service-bus/test/utils/setup.ts").is_empty()
        );
    }

    #[test]
    fn allows_playwright_global_teardown_file_issue1154() {
        // Playwright global-setup / global-teardown files use anonymous default
        // exports by design (resolved by file path, not function name).
        let setup = "export default async function globalSetup() {}";
        assert!(run_on_path(setup, "samples/v1/ts/global-setup.ts").is_empty());
        let teardown = "export default async function() {}";
        assert!(run_on_path(teardown, "samples/v1/ts/global-teardown.ts").is_empty());
    }

    #[test]
    fn allows_fixture_file_anonymous_default_issue1154() {
        // Dev-tool test fixture demonstrating the anti-pattern on purpose.
        let src = "export default async function (value, ms) {}";
        assert!(
            run_on_path(
                src,
                "common/tools/dev-tool/test/samples/files/inputs/cjs-forms/hasDefaultExport.ts"
            )
            .is_empty()
        );
        // The `__fixtures__/` convention is also exempt.
        assert!(run_on_path(src, "src/__fixtures__/anon.ts").is_empty());
    }

    #[test]
    fn still_flags_anonymous_default_in_normal_source_issue1154() {
        // A regular source module is not a hook file or a fixture — still flagged.
        let d = run_on_path("export default function() {}", "src/widgets/index.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("function"));
    }

    #[test]
    fn still_flags_setup_like_named_module_issue1154() {
        // `setupRouter.ts` is a regular module (stem is not exactly `setup`).
        let d = run_on_path("export default function() {}", "src/setupRouter.ts");
        assert_eq!(d.len(), 1);
    }
}
