//! OxcCheck backend for security-require-pkce-oauth.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn looks_like_authorize_url(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (lower.contains("/authorize") || lower.contains("/oauth/authorize") || lower.contains("/auth?"))
        && (lower.contains("client_id") || lower.contains("response_type"))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                oxc_ast::AstKind::StringLiteral(s) => {
                    let text = s.value.as_str();
                    if !looks_like_authorize_url(text) {
                        continue;
                    }
                    if text.contains("code_challenge") {
                        continue;
                    }
                    let (line, column) = byte_offset_to_line_col(ctx.source, s.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "OAuth authorize URL is missing `code_challenge` — PKCE is required for public clients.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                oxc_ast::AstKind::TemplateLiteral(tpl) => {
                    let text = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
                    if !looks_like_authorize_url(text) {
                        continue;
                    }
                    if text.contains("code_challenge") {
                        continue;
                    }
                    let (line, column) = byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "OAuth authorize URL is missing `code_challenge` — PKCE is required for public clients.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                _ => {}
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_authorize_url_without_pkce() {
        let src = "const url = 'https://idp.example.com/oauth/authorize?client_id=abc&response_type=code';";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_authorize_url_with_pkce() {
        let src = "const url = 'https://idp.example.com/oauth/authorize?client_id=abc&response_type=code&code_challenge=xyz&code_challenge_method=S256';";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_unrelated_strings() {
        assert!(run("const s = 'hello world';").is_empty());
    }
}
