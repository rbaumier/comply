//! OxcCheck backend for elysia-model-export-types.
//!
//! When a file exports a `t.Object(...)` const, expect a corresponding
//! `typeof X.static` type alias. Full-semantic dispatch (no per-node walk)
//! because this is a whole-file text heuristic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        let norm: String = ctx.source.chars().filter(|c| !c.is_whitespace()).collect();

        let exports_typebox_const = norm.contains("exportconst") && norm.contains("=t.Object(");
        if !exports_typebox_const {
            return Vec::new();
        }

        if norm.contains(".static") {
            return Vec::new();
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, 0);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Module exports a `t.Object(...)` schema but no `typeof X.static` type — consumers cannot annotate variables with the model type.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_schema_without_static_type() {
        let src = "import { t } from 'elysia';\nexport const User = t.Object({ id: t.Number() });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_schema_with_static_type() {
        let src = "import { t } from 'elysia';\nexport const User = t.Object({ id: t.Number() });\nexport type User = typeof User.static;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_file_with_no_typebox_export() {
        let src = "import { Elysia } from 'elysia';\nexport const app = new Elysia();";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "export const User = t.Object({ id: t.Number() });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
