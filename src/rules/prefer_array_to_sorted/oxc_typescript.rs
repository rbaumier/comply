//! prefer-array-to-sorted oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["sort"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `.sort(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "sort" {
            return;
        }

        let is_copy_pattern = match &member.object {
            // `[...arr].sort()` — a defensive copy only when the operand is
            // already an array. Spreading a `Map`/`Set`/iterator converts it to
            // an array instead, and the receiver has no `toSorted` of its own,
            // so the suggested rewrite would not compile.
            Expression::ArrayExpression(arr) => match arr.elements.as_slice() {
                [ArrayExpressionElement::SpreadElement(spread)] => {
                    expression_is_array(&spread.argument, semantic)
                }
                _ => false,
            },
            // arr.slice().sort()
            Expression::CallExpression(inner_call) => {
                if let Expression::StaticMemberExpression(inner_member) = &inner_call.callee {
                    inner_member.property.name.as_str() == "slice"
                } else {
                    false
                }
            }
            _ => false,
        };

        if !is_copy_pattern {
            return;
        }

        // `Array.prototype.toSorted` is ES2023: under an older declared library
        // the suggested rewrite does not type-check.
        if ctx.project.targets_es_below(ctx.path, 2023) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `arr.toSorted()` instead of copying then sorting (ES2023).".into(),
            severity: Severity::Error,
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

    /// Run the rule on `source` inside a TempDir holding the given tsconfig
    /// files (`(name, body)` pairs), so the ES-edition gate resolves against a
    /// real config graph. The source file sits next to them as `t.ts`.
    fn run_with_configs(source: &str, configs: &[(&str, &str)]) -> Vec<Diagnostic> {
        use crate::files::Language;
        use crate::project::ProjectCtx;
        use crate::rules::file_ctx::FileCtx;

        let dir = tempfile::TempDir::new().unwrap();
        for (name, body) in configs {
            std::fs::write(dir.path().join(name), body).unwrap();
        }
        let file_path = dir.path().join("t.ts");
        std::fs::write(&file_path, source).unwrap();

        let project = ProjectCtx::empty();
        let file = FileCtx::build(&file_path, source, Language::TypeScript, &project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &file_path, &project, &file)
    }

    const SPREAD_SORT: &str = "const arr: number[] = [3, 1];\nconst s = [...arr].sort((a, b) => a - b);";

    #[test]
    fn does_not_flag_under_a_library_older_than_es2023() {
        let d = run_with_configs(
            SPREAD_SORT,
            &[(
                "tsconfig.json",
                r#"{"compilerOptions":{"lib":["ES2022","DOM","DOM.Iterable"]}}"#,
            )],
        );
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn flags_under_an_es2023_library() {
        let d = run_with_configs(
            SPREAD_SORT,
            &[("tsconfig.json", r#"{"compilerOptions":{"lib":["ES2023","DOM"]}}"#)],
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn does_not_flag_under_an_older_target_without_lib() {
        let d = run_with_configs(
            SPREAD_SORT,
            &[("tsconfig.json", r#"{"compilerOptions":{"target":"ES2022"}}"#)],
        );
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn does_not_flag_when_a_referenced_project_declares_an_older_library() {
        // A create-vue solution-style root: the edition lives in the referenced
        // `tsconfig.app.json` the sources actually compile under.
        let d = run_with_configs(
            SPREAD_SORT,
            &[
                (
                    "tsconfig.json",
                    r#"{"files":[],"references":[{"path":"./tsconfig.app.json"}]}"#,
                ),
                (
                    "tsconfig.app.json",
                    r#"{"compilerOptions":{"lib":["ES2022","DOM"]},"include":["src"]}"#,
                ),
            ],
        );
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn flags_spread_of_annotated_array() {
        let d = run_on("const arr: number[] = [3, 1];\nconst s = [...arr].sort((a, b) => a - b);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_slice_then_sort() {
        let d = run_on("const arr = [3, 1];\nconst s = arr.slice().sort((a, b) => a - b);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn does_not_flag_spread_of_map() {
        let d = run_on(
            "const totalByCategory = new Map<string, number>();\nconst s = [...totalByCategory].sort((a, b) => b[1] - a[1]);",
        );
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn does_not_flag_spread_of_set() {
        let d = run_on("const seen = new Set<string>();\nconst s = [...seen].sort();");
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn does_not_flag_spread_of_unproven_receiver() {
        let d = run_on("export function f(input) {\n  return [...input].sort();\n}");
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn does_not_flag_plain_sort() {
        let d = run_on("const arr: number[] = [3, 1];\nconst s = arr.sort();");
        assert_eq!(d.len(), 0);
    }
}
