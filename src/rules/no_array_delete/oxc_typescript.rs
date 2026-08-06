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
}
