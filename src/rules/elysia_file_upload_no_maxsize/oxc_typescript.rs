//! OXC backend for elysia-file-upload-no-maxsize.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // callee must be `t.File` or `t.Files`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();
        if prop != "File" && prop != "Files" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "t" {
            return;
        }

        // Check args text for `maxSize:`
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("maxSize:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`t.File`/`t.Files` without `maxSize` — uncapped uploads can DoS the server.".into(),
            severity: Severity::Error,
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
    fn flags_file_without_maxsize() {
        let src = "import { Elysia, t } from 'elysia';\nconst s = t.File();";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_file_with_maxsize() {
        let src = "import { Elysia, t } from 'elysia';\nconst s = t.File({ maxSize: '5m' });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.File();";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
