//! OXC backend for elysia-jwt-name-multiple.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jwt"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        // Collect every `jwt(...)` call span + args source text.
        let mut calls: Vec<(u32, String)> = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            if callee.name.as_str() != "jwt" {
                continue;
            }
            let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
            calls.push((call.span.start, args_text.to_string()));
        }

        if calls.len() < 2 {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        let all_have_name = calls.iter().all(|(_, t)| {
            let norm: String = t.chars().filter(|c| !c.is_whitespace()).collect();
            norm.contains("name:'") || norm.contains("name:\"") || norm.contains("name:`")
        });

        if !all_have_name {
            let pos = calls.last().map(|(p, _)| *p).unwrap_or(0);
            let (line, column) = byte_offset_to_line_col(ctx.source, pos as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Multiple `jwt(...)` registrations but at least one has no `name` — they will overwrite each other. Give each a distinct `name`.".into(),
                severity: Severity::Error,
                span: None,
            });
            return diagnostics;
        }

        // Extract literal name values and check uniqueness.
        let mut names: Vec<String> = Vec::new();
        for (_, t) in &calls {
            let norm: String = t.chars().filter(|c| !c.is_whitespace()).collect();
            if let Some(start) = norm.find("name:") {
                let rest = &norm[start + 5..];
                let bytes = rest.as_bytes();
                if bytes.is_empty() {
                    continue;
                }
                let quote = bytes[0] as char;
                if quote != '\'' && quote != '"' && quote != '`' {
                    continue;
                }
                if let Some(end) = rest[1..].find(quote) {
                    names.push(rest[1..1 + end].to_string());
                }
            }
        }

        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != names.len() {
            let pos = calls.last().map(|(p, _)| *p).unwrap_or(0);
            let (line, column) = byte_offset_to_line_col(ctx.source, pos as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Multiple `jwt(...)` registrations share the same `name` — they will overwrite each other. Give each a distinct `name`.".into(),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_two_jwt_one_unnamed() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'access', secret: 'a' })).use(jwt({ secret: 'b' }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_duplicate_names() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'auth', secret: 'a' })).use(jwt({ name: 'auth', secret: 'b' }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_distinct_names() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'access', secret: 'a' })).use(jwt({ name: 'refresh', secret: 'b' }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_single_jwt() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ secret: 'a' }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_jwt_files() {
        let src = "app.use(jwt({})).use(jwt({}));";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
