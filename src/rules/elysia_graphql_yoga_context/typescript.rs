//! elysia-graphql-yoga-context backend — flag `yoga({ context })` without `useContext`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["useContext"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "yoga" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    if !norm.contains("context:") {
        return;
    }
    if ctx.source.contains("useContext") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-graphql-yoga-context".into(),
        message: "`yoga({ context })` without a `useContext` placeholder — resolvers will not see the context.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_context_without_placeholder() {
        let src = "import { yoga } from '@elysiajs/graphql-yoga';\napp.use(yoga({ context: () => ({ user: 1 }) }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_when_use_context_present() {
        let src = "import { yoga } from '@elysiajs/graphql-yoga';\nimport { useContext } from './ctx';\napp.use(yoga({ context: () => ({ user: 1 }) }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_when_no_context_field() {
        let src = "import { yoga } from '@elysiajs/graphql-yoga';\napp.use(yoga({ typeDefs }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_yoga_files() {
        let src = "yoga({ context: () => ({}) });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
