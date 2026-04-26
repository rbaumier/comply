//! node-no-top-level-await backend — disallow top-level `await`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    // Walk up: if we're inside any function scope, this is not top-level.
    let mut current = node.parent();
    while let Some(ancestor) = current {
        let ak = ancestor.kind();
        if ak == "function_declaration"
            || ak == "function"
            || ak == "arrow_function"
            || ak == "method_definition"
            || ak == "generator_function"
            || ak == "generator_function_declaration"
        {
            return; // Not top-level — inside a function.
        }
        current = ancestor.parent();
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-top-level-await".into(),
        message: "Top-level `await` is forbidden in published modules.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_top_level_await() {
        let d = run_on("const data = await fetch('/api');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Top-level"));
    }

    #[test]
    fn allows_await_in_async_function() {
        assert!(run_on("async function load() { const data = await fetch('/api'); }").is_empty());
    }

    #[test]
    fn allows_await_in_arrow() {
        assert!(run_on("const load = async () => { await fetch('/api'); };").is_empty());
    }
}
