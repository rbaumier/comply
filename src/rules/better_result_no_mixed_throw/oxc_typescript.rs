use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Result.try", "Result.tryPromise"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Find function-like nodes whose return type contains "Result<"
        for node in nodes.iter() {
            let (ret_span, body_span) = match node.kind() {
                AstKind::Function(f) => {
                    let Some(ref ret) = f.return_type else { continue };
                    let Some(ref body) = f.body else { continue };
                    (ret.span, body.span)
                }
                AstKind::ArrowFunctionExpression(f) => {
                    let Some(ref ret) = f.return_type else { continue };
                    (ret.span, f.body.span)
                }
                _ => continue,
            };

            let ret_text = &ctx.source[ret_span.start as usize..ret_span.end as usize];
            if !ret_text.contains("Result<") && !ret_text.contains("Result <") {
                continue;
            }

            // Walk all descendants of this function body looking for ThrowStatement
            let func_node_id = node.id();
            for descendant in nodes.iter() {
                if !matches!(descendant.kind(), AstKind::ThrowStatement(_)) {
                    continue;
                }

                let throw_span = match descendant.kind() {
                    AstKind::ThrowStatement(t) => t.span,
                    _ => continue,
                };
                // Check if this throw is inside the function body span
                if throw_span.start < body_span.start || throw_span.end > body_span.end {
                    continue;
                }

                // Walk ancestors to check:
                // 1. Not inside a nested function
                // 2. Not inside Result.try / Result.tryPromise
                let mut ancestor_id = descendant.id();
                let mut inside_nested_fn = false;
                let mut inside_result_try = false;

                loop {
                    let parent_id = nodes.parent_id(ancestor_id);
                    if parent_id == ancestor_id {
                        // Reached root
                        break;
                    }
                    if parent_id == func_node_id {
                        break;
                    }
                    let parent = nodes.get_node(parent_id);
                    match parent.kind() {
                        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                            inside_nested_fn = true;
                            break;
                        }
                        AstKind::CallExpression(call) => {
                            let callee_span = call.callee.span();
                            let callee_text = &ctx.source[callee_span.start as usize..callee_span.end as usize];
                            if callee_text == "Result.try" || callee_text == "Result.tryPromise" {
                                inside_result_try = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                    ancestor_id = parent_id;
                }

                if inside_nested_fn || inside_result_try {
                    continue;
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, throw_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Function returns Result<...> but contains `throw` — return Result.err(...) instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
