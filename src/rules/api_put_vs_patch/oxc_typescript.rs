use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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

        // Callee must be `<x>.put`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "put" {
            return;
        }

        // Check if any argument's type annotation subtree contains `Partial<...>`
        let source = ctx.source;
        let call_text = &source[call.span.start as usize..call.span.end as usize];
        if !call_text.contains("Partial") {
            return;
        }

        // Verify `Partial` appears in a type position by checking the AST
        // We look for Partial in type annotations within the arguments
        if !args_contain_partial_type(call, source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "PUT handler accepts `Partial<...>` — use `.patch(...)` for partial updates so clients keep idempotency guarantees for PUT.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn args_contain_partial_type(
    call: &oxc_ast::ast::CallExpression,
    source: &str,
) -> bool {
    // Walk arguments looking for type annotations containing Partial
    for arg in &call.arguments {
        let span = oxc_span::GetSpan::span(arg);
        let text = &source[span.start as usize..span.end as usize];
        // Look for arrow functions or function expressions with typed parameters
        if contains_partial_in_type_annotation(text) {
            return true;
        }
    }
    // Also check type_arguments on the call itself
    if let Some(type_args) = &call.type_arguments {
        let span = type_args.span;
        let text = &source[span.start as usize..span.end as usize];
        if text.contains("Partial<") || text.contains("Partial <") {
            return true;
        }
    }
    false
}

/// Check if text contains `Partial<` in what looks like a type annotation
/// (after `:` or inside `<...>`). This is a simplified heuristic.
fn contains_partial_in_type_annotation(text: &str) -> bool {
    // Look for `: ...Partial<` pattern (type annotation)
    // or `<...Partial<...>` (type argument)
    for (i, _) in text.match_indices("Partial<") {
        // Check if this is inside a string literal by counting quotes before it
        let before = &text[..i];
        let single_quotes = before.matches('\'').count();
        let double_quotes = before.matches('"').count();
        let backticks = before.matches('`').count();
        // If any quote count is odd, we're inside a string
        if !single_quotes.is_multiple_of(2) || !double_quotes.is_multiple_of(2) || !backticks.is_multiple_of(2) {
            continue;
        }
        // Check there's a `:` or `<` before it on the same nesting level
        // (simplified: just ensure it's not in a string)
        return true;
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
    fn flags_put_with_partial_in_handler() {
        let d = run(
            "app.put('/users/:id', (req: Request<{id: string}, {}, Partial<User>>, res) => {});",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_put_with_partial_in_body_type() {
        let d = run("router.put('/x', (body: Partial<Thing>) => body);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_put_with_full_type() {
        assert!(
            run("app.put('/users/:id', (req: Request<{id: string}, {}, User>, res) => {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_patch_with_partial() {
        assert!(
            run("app.patch('/users/:id', (req: Request<{}, {}, Partial<User>>, res) => {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_non_route_put_method() {
        assert!(run("const m = new Map(); m.put('k', 'v');").is_empty());
    }

    #[test]
    fn allows_partial_in_value_position_only() {
        assert!(run("app.put('/x', (req, res) => { console.log('Partial<User>'); });").is_empty());
    }
}
