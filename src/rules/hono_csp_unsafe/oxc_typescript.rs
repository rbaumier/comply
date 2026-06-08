//! hono-csp-unsafe OxcCheck backend — flag unsafe CSP directives in secureHeaders.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono/secure-headers"])
    }

    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains("hono/secure-headers") {
            return Vec::new();
        }
        if !ctx.source_contains("secureHeaders") && !ctx.source_contains("NONCE") {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::StringLiteral(lit) = node.kind() else {
                continue;
            };
            let text = lit.value.as_str();

            if text.contains("unsafe-inline") {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "hono-csp-unsafe".into(),
                    message: "`'unsafe-inline'` in CSP defeats its purpose — use nonces instead."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }

            if text.contains("unsafe-eval") {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "hono-csp-unsafe".into(),
                    message: "`'unsafe-eval'` in CSP enables code injection.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }

            // Check for `defaultSrc: ['*']`
            if text == "*" {
                for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
                    if let AstKind::ObjectProperty(prop) = ancestor.kind() {
                        let key_text = &ctx.source
                            [prop.key.span().start as usize..prop.key.span().end as usize];
                        if key_text == "defaultSrc" {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: "hono-csp-unsafe".into(),
                                message:
                                    "`defaultSrc: ['*']` allows loading resources from any origin."
                                        .into(),
                                severity: Severity::Error,
                                span: None,
                            });
                        }
                        break;
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_unsafe_inline() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['unsafe-inline'] } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_unsafe_eval() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['unsafe-eval'] } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_default_src_wildcard() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { defaultSrc: ['*'] } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_safe_csp() {
        let src = "import { secureHeaders, NONCE } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['self', NONCE] } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_hono_files() {
        let src = "const policy = { scriptSrc: ['unsafe-inline'] };";
        assert!(run_on(src).is_empty());
    }
}
