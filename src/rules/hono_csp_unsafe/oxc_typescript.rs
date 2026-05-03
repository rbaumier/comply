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
        if !ctx.source.contains("hono/secure-headers") {
            return Vec::new();
        }
        if !ctx.source.contains("secureHeaders") && !ctx.source.contains("NONCE") {
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
