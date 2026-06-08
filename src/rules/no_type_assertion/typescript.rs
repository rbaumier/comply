use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["as_expression"] => |node, source, ctx, diagnostics|
    let node_text = node.utf8_text(source).unwrap_or("");

    // Allow `as const` — it's a type refinement, not a cast
    if node_text.trim_end().ends_with("as const") {
        return;
    }

    // Allow `x as unknown as T` — inline double cast (canonical escape hatch)
    if node_text.contains(" as unknown as ") {
        return;
    }

    // Allow the `as unknown` half of a double cast — both inline form
    // (`x as unknown as T`, parent is as_expression) and parenthesized form
    // (`(x as unknown) as T`, parent is parenthesized_expression whose parent
    // is as_expression).
    if node_text.trim_end().ends_with("as unknown") {
        if let Some(parent) = node.parent() {
            if parent.kind() == "as_expression" {
                return;
            }
            if parent.kind() == "parenthesized_expression" {
                if parent.parent().is_some_and(|gp| gp.kind() == "as_expression") {
                    return;
                }
            }
        }
    }

    // Allow `(x as unknown) as T` — the outer cast in the parenthesized variant.
    if node.named_child(0).is_some_and(|inner| inner_is_unknown_cast(inner, source)) {
        return;
    }

    // Allow TanStack Router/Query hook and cache accessor calls — their return
    // types bridge deeply-generic boundaries that TypeScript cannot narrow
    // without an explicit `as` cast.
    if node.named_child(0).is_some_and(|src| is_tanstack_call(src, source)) {
        return;
    }

    // Allow single `as T` when the line or the preceding line carries
    // `// comply-ignore-reason: utility-type-constraint` — an acknowledged
    // workaround for third-party deferred conditional types (e.g. Drizzle).
    let pos = node.start_position();
    if line_or_prev_has_ignore_reason(source, pos.row) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-type-assertion".into(),
        message: "Type assertion `as T` bypasses the type checker — use `satisfies`, type guards, or generics.".into(),
        severity: Severity::Error,
        span: None,
    });
}

/// Returns `true` if source line `row` (0-based) or the preceding line contains
/// `// comply-ignore-reason: utility-type-constraint`.
fn line_or_prev_has_ignore_reason(source: &[u8], row: usize) -> bool {
    const MARKER: &[u8] = b"// comply-ignore-reason: utility-type-constraint";
    let lines: Vec<&[u8]> = source.split(|&b| b == b'\n').collect();
    let check = |line: &[u8]| line.windows(MARKER.len()).any(|w| w == MARKER);
    if lines.get(row).is_some_and(|l| check(l)) {
        return true;
    }
    if row > 0 && lines.get(row - 1).is_some_and(|l| check(l)) {
        return true;
    }
    false
}

/// Returns `true` when `node` is `(expr as unknown)` — the inner cast of a
/// parenthesized double-cast like `(x as unknown) as T`.
fn inner_is_unknown_cast(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "parenthesized_expression" {
        return false;
    }
    let Some(child) = node.named_child(0) else {
        return false;
    };
    if child.kind() != "as_expression" {
        return false;
    }
    let Some(target) = child.named_child(1) else {
        return false;
    };
    target.utf8_text(source).is_ok_and(|t| t.trim() == "unknown")
}

/// TanStack Router hooks and TanStack Query cache accessors whose return types
/// cannot be narrowed by TypeScript without an explicit `as` cast.
const TANSTACK_CALLS: &[&str] = &[
    "useParams",
    "useSearch",
    "useRouteContext",
    "useLoaderData",
    "useLoaderDeps",
    "useMatch",
    "useMatches",
    "useMatchedRoute",
    "useRouterState",
    "useNavigate",
    "getMutationCache",
    "getQueryCache",
];

/// Returns `true` when `node` is a call expression to a known TanStack
/// Router/Query hook or cache accessor (bare or member form, e.g.
/// `useParams()` or `routeApi.useParams()`).
fn is_tanstack_call(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(fn_name) = crate::rules::call_expression::call_function_name(node, source) else {
        return false;
    };
    let basename = fn_name.split('.').next_back().unwrap_or(fn_name);
    TANSTACK_CALLS.contains(&basename)
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_as_expression() {
        let diags = run_on("const x = foo as string;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("as T"));
    }

    #[test]
    fn flags_as_any() {
        let diags = run_on("const x = foo as any;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_as_unknown() {
        let diags = run_on("const x = foo as unknown;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_as_unknown_as_t_double_assertion() {
        let diags = run_on("const x = foo as unknown as Bar;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_const() {
        let diags = run_on("const x = { a: 1 } as const;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_const_multiline() {
        let code = "const x = { a: 1 } as const;\nconst y = 2;";
        let diags = run_on(code);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_const_array() {
        let diags = run_on("const arr = [1, 2, 3] as const;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_satisfies() {
        let diags = run_on("const x = { a: 1 } satisfies Config;");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_generic_call() {
        let diags = run_on("const x = getValue<string>();");
        assert!(diags.is_empty());
    }

    // Regression #368 — parenthesized double-cast variant of the #114 escape hatch
    #[test]
    fn allows_parenthesized_as_unknown_as_t() {
        // (x as unknown) as T — parenthesized form must be allowed
        let diags = run_on("const x = (someResult as unknown) as ExpectedType;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_tanstack_use_params_cast() {
        // TanStack Router: useParams() cannot express its return type without `as`
        let diags = run_on("const p = useParams() as { userId: string };");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_tanstack_use_search_cast() {
        let diags = run_on("const s = useSearch() as SearchParams;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_tanstack_member_call_cast() {
        // routeApi.useParams() — member-expression call form
        let diags = run_on("const p = routeApi.useParams() as { userId: string };");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_tanstack_query_get_mutation_cache() {
        // TanStack Query: getMutationCache() needs explicit cast for generics
        let diags = run_on(
            "const cache = queryClient.getMutationCache() as MutationCache<unknown, Error>;",
        );
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_non_tanstack_plain_cast() {
        // Non-TanStack calls must still be flagged
        let diags = run_on("const x = someOtherFn() as AdminUser;");
        assert_eq!(diags.len(), 1);
    }

    // Regression #388 — single `as T` with comply-ignore-reason: utility-type-constraint
    #[test]
    fn allows_utility_type_constraint_inline_comment() {
        let src = "const x = junctionTable as AnyPgTable; // comply-ignore-reason: utility-type-constraint";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_utility_type_constraint_preceding_comment() {
        let src = "// comply-ignore-reason: utility-type-constraint\nconst x = junctionTable as AnyPgTable;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_without_utility_type_constraint_comment() {
        let diags = run_on("const x = junctionTable as AnyPgTable;");
        assert_eq!(diags.len(), 1);
    }
}
