//! elysia-scope-missing OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const HOOK_METHODS: &[&str] = &[
    "onBeforeHandle",
    "onAfterHandle",
    "onError",
    "onRequest",
    "onTransform",
];

fn is_root_app_file(source: &str, path: &std::path::Path) -> bool {
    if source.contains(".listen(") {
        return true;
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    matches!(
        stem,
        "app" | "index" | "server" | "main" | "create-app" | "createApp" | "bootstrap" | "entry"
    )
}

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
        if !ctx.source.contains("export") {
            return;
        }
        if is_root_app_file(ctx.source, ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop_text = member.property.name.as_str();
        if !HOOK_METHODS.contains(&prop_text) {
            return;
        }

        // If the file uses any scope marker, skip.
        let s = ctx.source;
        let has_scope = s.contains("as:'global'")
            || s.contains("as: 'global'")
            || s.contains("as:\"global\"")
            || s.contains("as: \"global\"")
            || s.contains("as:'scoped'")
            || s.contains("as: 'scoped'")
            || s.contains("as:\"scoped\"")
            || s.contains("as: \"scoped\"")
            || s.contains(".as('scoped')")
            || s.contains(".as(\"scoped\")")
            || s.contains(".as('global')")
            || s.contains(".as(\"global\")");
        if has_scope {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` in an exported plugin without a scope — hooks default to `local` and won't propagate to the parent app.",
                prop_text
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
