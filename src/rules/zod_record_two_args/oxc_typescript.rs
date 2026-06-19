//! OxcCheck backend for zod-record-two-args.

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
        Some(&["record"])
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
        if member.property.name.as_str() != "record" {
            return;
        }
        let Expression::Identifier(obj_id) = &member.object else { return };
        if obj_id.name.as_str() != "z" {
            return;
        }

        if call.arguments.len() != 1 {
            return;
        }

        // Zod v3 keeps the single-argument `z.record(valueSchema)` API; only v4
        // removed it. When `z` resolves to a v3 import (`zod/v3` / `zod3`), the
        // single-arg form is correct, so this v4-migration rule must not fire.
        if crate::oxc_helpers::resolves_to_import_from(obj_id, semantic, &["zod/v3", "zod3"]) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.record(valueSchema)` with a single argument is removed in Zod v4 — \
                      pass the key schema explicitly: `z.record(z.string(), valueSchema)`."
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_single_arg_record() {
        assert_eq!(run_on("const r = z.record(z.boolean());").len(), 1);
    }

    #[test]
    fn flags_single_arg_record_with_v4_import() {
        let src = r#"
            import * as z from "zod";
            const r = z.record(z.boolean());
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_two_arg_record() {
        assert!(run_on("const r = z.record(z.string(), z.boolean());").is_empty());
    }

    // https://github.com/rbaumier/comply/issues/4436 — Zod v3 keeps the
    // single-argument `z.record(valueSchema)` API, so a `z` resolving to a v3
    // import must not be flagged by this v4-migration rule.
    #[test]
    fn does_not_flag_single_arg_record_with_zod_v3_import() {
        let src = r#"
            import * as z from "zod/v3";
            const booleanRecord = z.record(z.boolean());
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_single_arg_record_with_zod3_import() {
        let src = r#"
            import * as z from "zod3";
            const r = z.record(z.boolean());
        "#;
        assert!(run_on(src).is_empty());
    }
}
