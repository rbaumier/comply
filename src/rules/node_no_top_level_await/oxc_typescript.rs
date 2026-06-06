//! node-no-top-level-await OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
}

fn is_script_file(path: &std::path::Path, source: &str) -> bool {
    if path.components().any(|c| c.as_os_str() == "scripts") {
        return true;
    }
    source.starts_with("#!")
}

fn is_entrypoint(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, ".listen(") || crate::oxc_helpers::source_contains(source, "process.exit")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["await"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        if is_test_file(ctx.path)
            || is_script_file(ctx.path, ctx.source)
            || is_entrypoint(ctx.source)
        {
            return;
        }

        // Walk up: if inside any function scope, this is not top-level.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_) => {
                    return; // Inside a function — not top-level.
                }
                _ => {}
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Top-level `await` is forbidden in published modules.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
