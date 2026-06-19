//! OxcCheck backend for zod-no-unknown-schema.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.unknown"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "unknown" {
            return;
        }
        let Expression::Identifier(obj_id) = &member.object else { return };
        if obj_id.name.as_str() != "z" {
            return;
        }

        // `z.unknown().transform(...)`: the transform body narrows the output
        // type, so the schema is doing real work — skip it.
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::StaticMemberExpression(member) = parent.kind() {
            if member.property.name.as_str() == "transform" {
                let grand = semantic.nodes().parent_node(parent.id());
                if matches!(grand.kind(), AstKind::CallExpression(_)) {
                    return;
                }
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.unknown()` accepts any input — the schema provides no \
                      validation. Replace it with a concrete schema describing \
                      the expected shape."
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
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_rule_gated;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_z_unknown() {
        assert_eq!(run_on("const s = z.unknown();").len(), 1);
    }

    #[test]
    fn flags_z_unknown_inside_object() {
        assert_eq!(
            run_on("const s = z.object({ data: z.unknown() });").len(),
            1
        );
    }

    #[test]
    fn allows_concrete_schema() {
        assert!(run_on("const s = z.string();").is_empty());
    }

    /// Regression for #113: `z.unknown().transform(...)` narrows the
    /// output via the transform body, so the schema does real work.
    #[test]
    fn allows_z_unknown_as_transform_head() {
        let src = r#"
            const sortSchema = z.unknown().transform((value): "name:asc" | "name:desc" => {
                const parsed = sortLiteral.safeParse(value);
                return parsed.success ? parsed.data : "name:asc";
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_z_unknown_when_transform_is_not_immediate() {
        // `.optional()` between `z.unknown()` and `.transform(...)` —
        // the head is still `z.unknown()` but only the immediate `.transform`
        // case is whitelisted (anything else should keep flagging).
        let src = "const s = z.unknown().optional();";
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #4452: zod's own test suite exercises `z.unknown()`'s accept-any /
    // `unknown`-inference behavior (it is the schema under test, not a lazy
    // placeholder). The central `skip_in_test_dir` gate exempts `/tests/` dirs
    // and `.test.`/`.spec.` infixes.
    #[test]
    fn silent_in_test_dir_issue4452() {
        assert!(
            run_rule_gated(
                &Check,
                "const t1 = z.unknown();",
                "packages/zod/src/v4/classic/tests/anyunknown.test.ts",
            )
            .is_empty()
        );
        assert!(
            run_rule_gated(&Check, "const t1 = z.unknown();", "src/schema.test.ts").is_empty()
        );
    }

    // Suppression is test-scoped: the same `z.unknown()` in production source is
    // still flagged, preserving the rule's purpose.
    #[test]
    fn flags_z_unknown_in_production_issue4452() {
        assert_eq!(
            run_rule_gated(&Check, "const t1 = z.unknown();", "src/schema.ts").len(),
            1
        );
    }
}
