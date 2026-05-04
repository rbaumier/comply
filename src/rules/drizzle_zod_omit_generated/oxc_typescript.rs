//! drizzle-zod-omit-generated oxc backend — flag `createInsertSchema(...)`
//! calls without a `.omit({ id: true, ... })` chain step.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        use oxc_ast::ast::Expression;

        let Expression::Identifier(func) = &call.callee else {
            return;
        };
        if func.name != "createInsertSchema" {
            return;
        }

        // Check the full chain by looking at the source text from the call
        // up through any chained method calls.
        let chain_text = get_chain_text(call.span, ctx.source);

        if !chain_text.contains(".omit(") {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`createInsertSchema(table)` must chain `.omit({ id: true, createdAt: true, ... })` so API consumers don't submit DB-generated columns.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // Has `.omit(...)` — check if `id` is included.
        if !chain_text_has_omit_with_id(&chain_text) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`createInsertSchema(table).omit(...)` must drop the generated `id` column at minimum.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Get the source text of the entire method chain starting from `createInsertSchema(...)`.
/// We look ahead from the call's end for `.omit(...)` patterns.
fn get_chain_text(call_span: oxc_span::Span, source: &str) -> &str {
    let start = call_span.start as usize;
    // Scan forward from call end to find the full chain expression.
    // Simple heuristic: find the line or statement containing this call.
    let rest = &source[start..];
    // Find the end of the chain — look for a semicolon, newline after
    // close paren at nesting level 0, or end of source.
    let mut depth = 0i32;
    let mut end = rest.len();
    for (i, c) in rest.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth <= 0 {
                    // Check if next non-whitespace is `.` (chain continues).
                    let after = &rest[i + 1..];
                    let next = after.trim_start();
                    if !next.starts_with('.') {
                        end = i + 1;
                        break;
                    }
                }
            }
            ';' | '\n' if depth <= 0 => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    &rest[..end]
}

/// Check if the chain text has `.omit({ ... id ... })`.
fn chain_text_has_omit_with_id(text: &str) -> bool {
    if let Some(pos) = text.find(".omit(") {
        let after_omit = &text[pos + 6..];
        // Find the object literal inside omit(...).
        if let Some(brace_start) = after_omit.find('{') {
            let inner = &after_omit[brace_start..];
            // Simple check: does the object literal contain `id`.
            if let Some(brace_end) = inner.find('}') {
                let obj_content = &inner[1..brace_end];
                // Check for `id` as a key (not substring of another key).
                return obj_content.split(',').any(|part| {
                    let key = part.split(':').next().unwrap_or("").trim();
                    key == "id"
                });
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_create_insert_schema_without_omit() {
        let src = "export const schema = createInsertSchema(users)";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_create_insert_schema_with_omit() {
        let src =
            "export const schema = createInsertSchema(users).omit({ id: true, createdAt: true })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_other_calls() {
        let src = "export const schema = createSelectSchema(users)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_omit_that_does_not_drop_id() {
        let src = "export const schema = createInsertSchema(users).omit({ name: true })";
        assert_eq!(run(src).len(), 1);
    }
}
