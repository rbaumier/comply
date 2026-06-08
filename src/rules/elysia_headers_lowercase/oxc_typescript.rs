//! elysia-headers-lowercase oxc backend — flag uppercase header keys in headers schema.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const COMMON_UPPERCASE_KEYS: &[&str] = &[
    "Authorization:",
    "Content-Type:",
    "Accept:",
    "User-Agent:",
    "Cookie:",
    "X-",
];

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

        let Some(idx) = norm.find("headers:t.Object({") else { return };
        let after = &norm[idx..];

        // Bound the headers section to the next top-level key.
        let cut = ["body:", "params:", "query:", "response:", "cookie:", "detail:", "tags:"]
            .iter()
            .filter_map(|k| after[1..].find(k).map(|i| i + 1))
            .min()
            .unwrap_or(after.len());
        let section = &after[..cut];

        let has_uppercase = COMMON_UPPERCASE_KEYS.iter().any(|k| section.contains(k));
        if !has_uppercase {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`headers:` schema uses uppercase keys — Elysia lowercases header names, so the schema will never match.".into(),
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
    fn flags_uppercase_authorization() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { headers: t.Object({ Authorization: t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_uppercase_x_header() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { headers: t.Object({ 'X-Api-Key': t.String() }) });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_lowercase_headers() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/x', () => 'ok', { headers: t.Object({ authorization: t.String() }) });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/x', () => 'ok', { headers: t.Object({ Authorization: 1 }) });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
