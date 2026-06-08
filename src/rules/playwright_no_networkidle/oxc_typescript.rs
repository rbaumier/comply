//! playwright-no-networkidle OXC backend — flag `"networkidle"` string literals
//! in test files within a Playwright context.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["networkidle"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                oxc_ast::AstKind::StringLiteral(lit) if lit.value.as_str() == "networkidle" => {
                    let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "`networkidle` is timing-based and flaky \u{2014} use a web-first assertion or `waitForResponse` instead.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                oxc_ast::AstKind::TemplateLiteral(tpl) if tpl.expressions.is_empty() => {
                    // Template with no substitutions: `networkidle`
                    if tpl.quasis.len() == 1 && tpl.quasis[0].value.raw.as_str() == "networkidle" {
                        let (line, column) = byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`networkidle` is timing-based and flaky \u{2014} use a web-first assertion or `waitForResponse` instead.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
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
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

}
