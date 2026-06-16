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
        // tsd type-test files pass `Function` as an input to the utility under
        // test (e.g. `ConditionalSimplify<SomeFunction, Function>`), so the
        // banned type is the test subject, not application code.
        if ctx.file.is_type_test_file() {
            return;
        }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            std::path::Path::new(path),
            source,
            crate::files::Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
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

    #[test]
    fn exempts_tsd_type_test_file_issue3324() {
        // type-fest test-d/conditional-simplify.ts: `Function` is the input to
        // the utility under test, not application code.
        let src = "type SimplifiedFunctionPass = ConditionalSimplify<SomeFunction, Function>;";
        assert!(run_at(src, "test-d/conditional-simplify.ts").is_empty());
    }

    #[test]
    fn still_flags_function_type_in_production_issue3324() {
        assert_eq!(run_at("function call(cb: Function) { cb(); }", "src/widget.ts").len(), 1);
    }
}
