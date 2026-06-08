use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
];

const DEPRECATED: &[(&str, &str)] = &[
    ("cacheTime", "renamed to `gcTime` in v5"),
    ("useErrorBoundary", "renamed to `throwOnError` in v5"),
    ("onSuccess", "removed from useQuery in v5 — use useEffect"),
    ("onError", "removed from useQuery in v5 — use useEffect"),
    ("onSettled", "removed from useQuery in v5 — use useEffect"),
];

fn callee_tail_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
        _ => None,
    }
}

fn inside_query_hook(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            if let Some(name) = callee_tail_name(&call.callee) {
                if QUERY_HOOKS.contains(&name) {
                    return true;
                }
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["cacheTime", "useErrorBoundary", "onSuccess", "onError", "onSettled"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        let Some((_, reason)) = DEPRECATED.iter().find(|(k, _)| *k == key_name) else {
            return;
        };
        if !inside_query_hook(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{key_name}` is deprecated — {reason}."),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_cache_time() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], cacheTime: 5000 });").len(),
            1
        );
    }

    #[test]
    fn flags_on_success_on_use_query() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], onSuccess: () => {} });").len(),
            1
        );
    }

    #[test]
    fn allows_gc_time() {
        assert!(run("useQuery({ queryKey: ['x'], gcTime: 5000 });").is_empty());
    }

    #[test]
    fn does_not_flag_on_success_in_mutation() {
        assert!(run("useMutation({ onSuccess: () => {} });").is_empty());
    }
}
