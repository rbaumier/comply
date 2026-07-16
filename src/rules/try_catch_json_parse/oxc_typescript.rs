//! try-catch-json-parse oxc backend — flag `JSON.parse(...)` not wrapped
//! in a `try` statement.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_json_method_call};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Walk up semantic parents to check if node is inside a try body.
/// Stop at function boundaries (outer try can't catch inner function throws
/// unless the function is awaited/called within the try).
fn is_inside_try_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cur = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(cur);
        if parent_id == cur {
            break;
        }
        let parent = semantic.nodes().get_node(parent_id);
        match parent.kind() {
            AstKind::TryStatement(try_stmt) => {
                // Check if our node is inside the try block (not the catch/finally).
                let block_span = try_stmt.block.span;
                let node_start = node.kind().span().start;
                let node_end = node.kind().span().end;
                if node_start >= block_span.start && node_end <= block_span.end {
                    return true;
                }
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return false;
            }
            _ => {}
        }
        cur = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON.parse"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Callee must be `JSON.parse`.
        if !is_json_method_call(call, "parse") {
            return;
        }

        // Skip the deep-clone idiom `JSON.parse(JSON.stringify(x))`: for any
        // serializable `x`, `JSON.stringify` yields valid JSON, so the parse is
        // not the untrusted-input risk this rule targets. (A top-level
        // `undefined`/function/symbol stringifies to `undefined` and
        // `JSON.parse(undefined)` does throw, but flagging the clone idiom over
        // that edge is net noise.) A spread or non-call first argument stays
        // flagged (`.as_expression()` yields `None` for a spread).
        if let Some(Expression::CallExpression(arg_call)) =
            call.arguments.first().and_then(|arg| arg.as_expression())
        {
            if is_json_method_call(arg_call, "stringify") {
                return;
            }
        }

        if is_inside_try_body(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`JSON.parse` can throw on invalid input — wrap it in \
                      try/catch or use a safe parser (Zod, schema validator)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod gated_tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn skips_json_parse_in_test_dir() {
        // #5758 firing site (error-handler.test.ts): parsing a controlled
        // fixture (the middleware's own Problem+JSON body) where an unexpected
        // parse throw is the intended test oracle, not a production crash. The
        // central `skip_in_test_dir` gate must suppress it.
        let src = r#"const body: unknown = JSON.parse(text);"#;
        assert!(run_rule_gated(&Check, src, "src/api/middleware/error-handler.test.ts").is_empty());
    }

    #[test]
    fn flags_unwrapped_json_parse_in_production() {
        // Negative-space guard: an unwrapped `JSON.parse` in a production module
        // is the rule's genuine target — keep flagging.
        let src = r#"export function handle(text: string) { return JSON.parse(text); }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/api/middleware/handler.ts").len(), 1);
    }

    #[test]
    fn still_allows_try_wrapped_json_parse_in_production() {
        // The remediation (wrap in try) keeps the rule silent in production.
        let src = r#"export function handle(text: string) { try { return JSON.parse(text); } catch { return null; } }"#;
        assert!(run_rule_gated(&Check, src, "src/api/middleware/handler.ts").is_empty());
    }

    #[test]
    fn skips_json_parse_of_json_stringify_round_trip() {
        // #7560: `JSON.parse(JSON.stringify(x))` is the deep-clone idiom. For
        // any serializable `x`, `JSON.stringify` yields valid JSON, so the parse
        // is not the untrusted-input risk this rule targets — flagging it would
        // be noise.
        let src = r#"export const clone = <T>(obj: T): T => JSON.parse(JSON.stringify(obj));"#;
        assert!(run_rule_gated(&Check, src, "src/utils/clone.ts").is_empty());
    }

    #[test]
    fn skips_json_parse_of_json_stringify_with_as_cast() {
        // The `as T` cast wraps the parse call, not its argument — the safe
        // round-trip argument shape is unchanged, so it stays silent.
        let src = r#"export const clone = <T extends object>(obj: T): T => JSON.parse(JSON.stringify(obj)) as T;"#;
        assert!(run_rule_gated(&Check, src, "src/utils/clone.ts").is_empty());
    }

    #[test]
    fn flags_json_parse_of_awaited_text() {
        // An awaited `res.text()` is untrusted input, not a stringify round-trip
        // — keep flagging.
        let src = r#"export async function load(res: Response) { return JSON.parse(await res.text()); }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/api/load.ts").len(), 1);
    }

    #[test]
    fn flags_json_parse_of_non_json_stringify() {
        // The skip is specific to `JSON.stringify`: a `stringify` method on any
        // other object gives no validity guarantee, so it stays flagged.
        let src = r#"export function load(notJson: { stringify: (x: unknown) => string }, x: unknown) { return JSON.parse(notJson.stringify(x)); }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/api/load.ts").len(), 1);
    }
}
