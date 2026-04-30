//! elysia-service-coupled backend — flag elysia imports inside service modules.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let path_str = ctx.path.to_string_lossy().to_lowercase();
    if !path_str.contains("service") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    // Only consider direct `elysia` imports (not @elysiajs/*).
    let from_elysia = text.contains("from 'elysia'") || text.contains("from \"elysia\"");
    if !from_elysia {
        return;
    }

    // Find the named-imports clause `{ ... }` and extract identifiers.
    let Some(open) = text.find('{') else { return };
    let Some(close) = text[open..].find('}').map(|i| i + open) else { return };
    let names: Vec<&str> = text[open + 1..close]
        .split(',')
        .map(|s| s.split(" as ").next().unwrap_or("").trim())
        .filter(|s| !s.is_empty())
        .collect();
    if names.is_empty() {
        return;
    }
    if names.iter().all(|n| *n == "status") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-service-coupled".into(),
        message: "Service modules should not import framework symbols from `elysia` (only `status` is allowed). Move HTTP concerns to the route layer.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;

    fn run_on(path: &str, source: &str) -> Vec<Diagnostic> {
        let project = ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_ts_with_project_and_path(
            source,
            &Check,
            &project,
            std::path::Path::new(path),
        )
    }

    #[test]
    fn flags_elysia_class_in_service() {
        let src = "import { Elysia } from 'elysia';\nexport const userService = {};";
        assert_eq!(run_on("src/services/user.ts", src).len(), 1);
    }

    #[test]
    fn flags_t_typebox_in_service() {
        let src = "import { t } from 'elysia';\nexport const schema = t.Object({});";
        assert_eq!(run_on("src/services/billing.ts", src).len(), 1);
    }

    #[test]
    fn allows_status_only_import() {
        let src =
            "import { status } from 'elysia';\nexport const notFound = () => status(404, 'gone');";
        assert!(run_on("src/services/user.ts", src).is_empty());
    }

    #[test]
    fn ignores_non_service_files() {
        let src = "import { Elysia } from 'elysia';\nexport const app = new Elysia();";
        assert!(run_on("src/routes/index.ts", src).is_empty());
    }
}
