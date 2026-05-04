//! tanstack-start-server-fn-requires-validation OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const VALIDATION_METHODS: &[&str] = &["input", "safeParse", "parse"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut server_fn_spans = Vec::new();
        let mut has_validation = false;

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };

            // Check for createServerFn(...)
            if let oxc_ast::ast::Expression::Identifier(id) = &call.callee
                && id.name.as_str() == "createServerFn" {
                    server_fn_spans.push(call.span);
                    continue;
                }

            // Check for .input() / .safeParse() / .parse()
            if let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee
                && VALIDATION_METHODS.contains(&member.property.name.as_str()) {
                    has_validation = true;
                }
        }

        if server_fn_spans.is_empty() || has_validation {
            return Vec::new();
        }

        server_fn_spans
            .into_iter()
            .map(|span| {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, span.start as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`createServerFn` without `.input()` validation accepts unvalidated data at the RPC boundary.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}
