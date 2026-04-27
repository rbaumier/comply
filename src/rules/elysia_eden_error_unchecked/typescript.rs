//! elysia-eden-error-unchecked backend — flag `{ data }` destructuring without `error` in eden treaty files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["object_pattern"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    let norm: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm != "{data}" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-eden-error-unchecked".into(),
        message: "Eden treaty returns `{ data, error }` — destructure both and check `error` before using `data`.".into(),
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
    fn flags_data_only_destructure() {
        let src = "import { treaty } from '@elysiajs/eden';\nconst api = treaty('http://x');\nconst { data } = await api.users.get();";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_data_and_error_destructure() {
        let src = "import { treaty } from '@elysiajs/eden';\nconst api = treaty('http://x');\nconst { data, error } = await api.users.get();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_eden_files() {
        let src = "const { data } = await fetch('/x').then(r => r.json());";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
