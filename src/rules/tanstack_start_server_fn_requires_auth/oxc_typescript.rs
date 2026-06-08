//! tanstack-start-server-fn-requires-auth OXC backend — flag `createServerFn`
//! with mutations but no auth check.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const AUTH_CALLEES: &[&str] = &[
    "getSession",
    "auth",
    "verifySession",
    "requireAuth",
    "currentUser",
];

const MUTATION_METHODS: &[&str] = &["insert", "update", "delete"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut server_fn_spans: Vec<u32> = Vec::new();
        let mut has_mutation = false;
        let mut has_auth = false;

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };

            match &call.callee {
                // Bare identifier: `createServerFn(...)`, or auth helpers.
                Expression::Identifier(ident) => {
                    let name = ident.name.as_str();
                    if name == "createServerFn" {
                        server_fn_spans.push(call.span.start);
                    }
                    if AUTH_CALLEES.contains(&name) {
                        has_auth = true;
                    }
                }
                // Member expression: `.insert(...)`, `.delete(...)`, etc.,
                // or `ctx.auth(...)`.
                Expression::StaticMemberExpression(member) => {
                    let method = member.property.name.as_str();
                    if MUTATION_METHODS.contains(&method) {
                        has_mutation = true;
                    }
                    if AUTH_CALLEES.contains(&method) {
                        has_auth = true;
                    }
                }
                _ => {}
            }
        }

        if server_fn_spans.is_empty() || !has_mutation || has_auth {
            return Vec::new();
        }

        server_fn_spans
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`createServerFn` with mutations must verify authentication before proceeding.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(s, &Check, "api.functions.ts")
    }


    #[test]
    fn flags_mutation_without_auth() {
        assert_eq!(
            run("const del = createServerFn().handler(async () => { await db.delete(posts) })")
                .len(),
            1
        );
    }


    #[test]
    fn allows_with_get_session() {
        assert!(run(
            "const del = createServerFn().handler(async () => { const s = await getSession(); await db.delete(posts) })"
        )
        .is_empty());
    }


    #[test]
    fn allows_read_only() {
        assert!(
            run("const get = createServerFn().handler(async () => db.select().from(posts))")
                .is_empty()
        );
    }
}
