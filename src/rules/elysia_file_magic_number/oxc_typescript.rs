//! OxcCheck backend for elysia-file-magic-number — flag `z.file()` without a magic-number check.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") || !ctx.source_contains("zod") {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "z.file" {
            return;
        }
        if ctx.source_contains("fileType(") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.file()` only checks the MIME header — verify the magic number with `fileType()`.".into(),
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
    fn flags_bare_z_file() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\napp.post('/upload', h, { body: z.object({ file: z.file() }) });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_z_file_with_options() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nconst s = z.file({ type: 'image/png' });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_z_file_with_filetype_refine() {
        let src = "import { Elysia } from 'elysia';\nimport { z } from 'zod';\nimport { fileType } from 'file-type';\nconst s = z.file().refine(b => fileType(b)?.mime === 'image/png');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "import { z } from 'zod';\nconst s = z.file();";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
