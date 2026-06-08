//! elysia-streaming-headers-after-yield OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // Elysia streams via generator handlers. An arrow function can never be
        // a generator, and a `yield` inside a non-generator function belongs to
        // a nested generator (visited on its own) — so only generator functions
        // can be the streaming handler under scrutiny.
        let AstKind::Function(f) = node.kind() else {
            return;
        };
        if !f.generator {
            return;
        }

        // A generator passed to `Result.gen()` / `Effect.gen()` is a monadic
        // generator: its `yield*` are binds, not stream chunks, and the handler
        // returns a buffered value — headers commit at `return`, not mid-yield.
        if is_monadic_gen_callback(node, semantic) {
            return;
        }

        let span = f.span;
        let start = span.start as usize;
        let end = span.end as usize;
        let body_text = &ctx.source[start..end.min(ctx.source.len())];

        let Some(yield_idx) = body_text.find("yield") else {
            return;
        };
        let Some(headers_idx) = body_text.find("set.headers") else {
            return;
        };
        if headers_idx <= yield_idx {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`set.headers` is assigned after a `yield` — headers are already flushed once the stream starts. Move header writes before the first yield."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when `node` (a generator function) is the callback argument of a
/// `Result.gen(...)` / `Effect.gen(...)` (or any `*.gen(...)`) call — a monadic
/// generator, not an Elysia streaming handler.
fn is_monadic_gen_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(c) => Some(c),
        _ => match nodes.kind(nodes.parent_id(parent_id)) {
            AstKind::CallExpression(c) => Some(c),
            _ => None,
        },
    };
    let Some(call) = call else { return false };
    matches!(
        &call.callee,
        Expression::StaticMemberExpression(m) if m.property.name.as_str() == "gen"
    )
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
mod tests {
    use super::*;
    
    // Regression for #285: a `Result.gen(async function* () { … })` nested in an
    // arrow handler is a monadic generator — its `yield*` are binds, and the
    // handler returns a buffered value. No streaming, nothing to flag.
    #[test]
    fn allows_result_gen_generator_in_arrow_handler() {
        let src = r#"
            app.get("/csv", async ({ session, set }) =>
                unwrapOrThrow(Result.gen(async function* () {
                    yield* authorize(session, { kind: "x" });
                    const rows = yield* Result.await(query());
                    const csv = stringify(rows);
                    set.headers["content-type"] = "text/csv";
                    return Result.ok(csv);
                }))
            );
        "#;
        let d = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx());
        assert!(d.is_empty(), "monadic generator must not flag: {d:?}");
    }

    // A genuine streaming generator handler that writes headers after the first
    // yield is still flagged — that is the bug the rule exists to catch.
    #[test]
    fn flags_headers_after_yield_in_generator_handler() {
        let src = r#"
            app.get("/sse", async function* ({ set }) {
                yield "data: start\n\n";
                set.headers["content-type"] = "text/event-stream";
            });
        "#;
        let d = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(d.len(), 1, "headers after yield in a stream must flag: {d:?}");
    }
}
