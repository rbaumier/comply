//! OXC backend for xstate-no-inline-implementation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const INLINE_KEYS: &[&str] = &["actions", "entry", "exit", "guard", "cond", "src"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["xstate"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ObjectProperty(prop) = node.kind() else {
                continue;
            };

            // Check key name.
            let Some(key_name) = prop.key.name() else {
                continue;
            };
            if !INLINE_KEYS.contains(&key_name.as_ref()) {
                continue;
            }

            // Value must be an inline function.
            if !matches!(
                &prop.value,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            ) {
                continue;
            }

            // Walk ancestors: only emit if inside a createMachine / setup call.
            let mut inside_machine = false;
            for ancestor_kind in semantic.nodes().ancestor_kinds(node.id()) {
                if let AstKind::CallExpression(call) = ancestor_kind {
                    let callee_text = &ctx.source
                        [call.callee.span().start as usize..call.callee.span().end as usize];
                    if callee_text.contains("createMachine") || callee_text.contains("setup") {
                        inside_machine = true;
                        break;
                    }
                }
            }
            if !inside_machine {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Inline function used as `{key_name}` — define it as a named action/guard/service instead."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}
