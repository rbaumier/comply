//! OxcCheck backend — flag mixing Zod with Elysia's `t`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["zod"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        let source_value = import.source.value.as_str();
        if source_value != "zod" {
            return;
        }

        let uses_t = ctx.source_contains("t.Object(")
            || ctx.source_contains("t.String(")
            || ctx.source_contains("t.Number(")
            || ctx.source_contains("t.Array(")
            || ctx.source_contains("t.Boolean(");
        if !uses_t {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "File uses both Zod and Elysia's `t` validators — pick one. Mixing breaks Elysia's static type inference.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_mixed_zod_and_t() {
        let src = "import { Elysia, t } from 'elysia';\nimport { z } from 'zod';\nconst s = t.Object({ a: t.String() });\nconst z2 = z.object({});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_only_t() {
        let src = "import { Elysia, t } from 'elysia';\nconst s = t.Object({ a: t.String() });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_only_zod() {
        let src =
            "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nconst s = z.object({});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "import { z } from 'zod';\nconst x = t.Object({});";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
