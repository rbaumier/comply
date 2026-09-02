//! no-array-delete oxc backend — flag `delete arr[i]` on array targets.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_array_delete_target};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::MemberExpression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["delete"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            return;
        };
        if unary.operator != oxc_ast::ast::UnaryOperator::Delete {
            return;
        }
        // Test files delete `process.env` keys and fixture entries in teardown —
        // bounded to the test scope with no non-mutating equivalent.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // The argument must be a computed member expression (bracket access),
        // read through `get_member_expr` so that every spelling `no-delete`
        // hands over reaches this test — see the `no-delete` rule.
        let Some(MemberExpression::ComputedMemberExpression(member)) =
            unary.argument.get_member_expr()
        else {
            return;
        };

        // Only fire with positive evidence the target is an array; deleting a
        // key from a plain object / record / dictionary is a valid operation
        // that creates no sparse hole.
        if !is_array_delete_target(member, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`delete arr[i]` creates a sparse hole — use `arr.splice(i, 1)` instead."
                .into(),
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
mod oxc_tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_delete_array_element() {
        assert_eq!(run("delete arr[0];").len(), 1);
    }

    #[test]
    fn flags_delete_array_literal_binding() {
        let src = "const arr = [1, 2, 3]; delete arr[i];";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_delete_array_typed_binding() {
        let src = "const arr: number[] = []; delete arr[i];";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_delete_through_an_optional_chain() {
        // `delete arr?.[i]` leaves the same sparse hole. `no-delete` hands the
        // array case here on the same unwrapped member, so a spelling this rule
        // did not read would go unreported by both.
        let src = "const arr = [1, 2, 3]; delete arr?.[i];";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_delete_new_array_binding() {
        let src = "const arr = new Array(3); delete arr[i];";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_record_key_issue_1889() {
        // `plugins` is a record typed binding; the key is a `keyof`-typed param.
        let src = "type Plugins = Record<string, unknown>; const plugins: Plugins = {}; \
                   const clearPlugin = <K extends keyof Plugins>(pluginKey: K): void => { delete plugins[pluginKey]; };";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_descriptors_issue_1889() {
        // `descriptors` is a PropertyDescriptorMap from getOwnPropertyDescriptors.
        let src = "const descriptors = Object.getOwnPropertyDescriptors(base); delete descriptors[DRAFT_STATE as any];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_member_target_issue_1889() {
        // `state.copy_` is a member access typed as the proxied object T.
        let src = "if (state.copy_) { delete state.copy_[prop]; }";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_object_typed_binding() {
        let src = "const obj: Record<string, number> = {}; delete obj[key];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_object_literal_binding() {
        let src = "const obj = {}; delete obj[key];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_process_env_issue_479() {
        let src = "delete process.env[key];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_in_test_file_issue_582() {
        // Test teardown deletes fixture entries; bounded to test scope.
        assert!(run_in_test_file("delete fixtures[id];").is_empty());
    }

    // The receiver decides, not the index — issue #8440

    #[test]
    fn skips_numeric_key_deletion_on_a_fresh_object_issue_8440() {
        // Regression for rbaumier/comply#8440: a numeric literal is evidence
        // about the KEY, not about the receiver. `copy` is an object, and
        // `copy.splice(0, 1)` names a method it does not have — this line
        // belongs to `no-delete`, which is free to decide it is an immutable
        // `omit` and stay silent.
        let src = "const copy = { ...o }; delete copy[0];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_numeric_key_deletion_on_a_number_keyed_record_param_issue_8440() {
        // Same defect through a parameter: `Record<number, string>` takes a
        // numeric index and is not an array.
        let src = "function f(cache: Record<number, string>) { delete cache[0]; }";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn flags_dynamic_deletion_on_an_array_typed_parameter_issue_8440() {
        // The other direction: a parameter's declaration node is a
        // `FormalParameter`, which the resolution used to reject outright, so a
        // `string[]` parameter carried no array evidence and the sparse-hole
        // advice was replaced by `no-delete`'s rest-destructuring advice.
        let src = "function f(arr: string[], i: number) { delete arr[i]; }";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn each_delete_draws_exactly_one_diagnostic_issue_8440() {
        // The hand-off is only observable through the engine: `no-delete`
        // returns early on what this predicate calls an array, so a wrong
        // verdict replaces the right message instead of adding a second one.
        // Every line must still be reported — by one rule, the one the receiver
        // names.
        let source = r#"
export function numericKeyOnParam(cache: Record<number, string>): void {
  delete cache[0];
}

export function arrayParam(arr: string[], i: number): void {
  delete arr[i];
}
"#;
        let diagnostics = crate::engine::lint_in_memory(
            std::path::Path::new("a.ts"),
            crate::files::Language::TypeScript,
            source,
            crate::config::default_static_config(),
            None,
        );
        // Only the two delete rules are under test: `cache[0]` also draws
        // `boundary-condition`, which reads the index for a different reason.
        let on_line = |line: usize| {
            let mut ids: Vec<&str> = diagnostics
                .iter()
                .filter(|d| d.line == line && d.rule_id.contains("delete"))
                .map(|d| d.rule_id.as_ref())
                .collect();
            ids.sort_unstable();
            ids
        };
        assert_eq!(on_line(3), ["no-delete"], "record key: {diagnostics:?}");
        assert_eq!(on_line(7), ["no-array-delete"], "array param: {diagnostics:?}");
    }
}
