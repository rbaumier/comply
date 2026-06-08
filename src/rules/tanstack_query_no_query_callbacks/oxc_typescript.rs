//! tanstack-query-no-query-callbacks oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
];

const REMOVED: &[&str] = &["onSuccess", "onError", "onSettled"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_span = prop.key.span();
        let key_text = &ctx.source[key_span.start as usize..key_span.end as usize];
        // Strip quotes for computed string keys
        let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
        if !REMOVED.contains(&key_name) {
            return;
        }

        if !inside_query_hook(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{key_name}:` on `useQuery` was removed in TanStack Query v5 — move side-effects to `useEffect`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn inside_query_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut first = true;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if first {
            first = false;
            continue;
        }
        if let AstKind::CallExpression(call) = ancestor.kind() {
            if let oxc_ast::ast::Expression::Identifier(ident) = &call.callee {
                if QUERY_HOOKS.contains(&ident.name.as_str()) {
                    return true;
                }
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
    fn flags_on_success() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, onSuccess: () => {} })").len(),
            1
        );
    }


    #[test]
    fn allows_no_callbacks() {
        assert!(run("useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }


    #[test]
    fn ignores_use_mutation() {
        assert!(run("useMutation({ onSuccess: () => {} })").is_empty());
    }
}
