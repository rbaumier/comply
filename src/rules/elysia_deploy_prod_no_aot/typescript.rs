//! elysia-deploy-prod-no-aot backend — flag the **root** Elysia instance
//! missing `aot:true`.
//!
//! `aot` is a root-app option only — setting it on a plugin/middleware
//! instance triggers a runtime warning. The rule only fires when the file
//! looks like a root app: it calls `.listen(...)` on the instance, or is
//! named `app.ts` / `index.ts` / `server.ts` / `create-app.ts` / `main.ts`.
//! Files in a `middleware/`, `plugins/`, or `routes/` segment are skipped.

use crate::diagnostic::{Diagnostic, Severity};

fn is_root_app_file(source: &str, path: &std::path::Path) -> bool {
    // Strong signal: this file boots the server.
    if source.contains(".listen(") {
        return true;
    }

    // Plugin/middleware directory? Not a root app.
    let in_plugin_dir = path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("middleware")
                | Some("middlewares")
                | Some("plugins")
                | Some("plugin")
                | Some("routes")
                | Some("modules")
        )
    });
    if in_plugin_dir {
        return false;
    }

    // Filename heuristic for entry points.
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    matches!(
        stem,
        "app" | "index" | "server" | "main" | "create-app" | "createApp" | "bootstrap" | "entry"
    )
}

crate::ast_check! { on ["new_expression"] prefilter = [".listen("] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !is_root_app_file(ctx.source, ctx.path) {
        return;
    }

    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.utf8_text(source).unwrap_or("") != "Elysia" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    // Only flag when the constructor receives a config object — bare `new Elysia()` is fine.
    if !norm.contains('{') {
        return;
    }
    if norm.contains("aot:true") || norm.contains("aot:false") {
        return;
    }
    // Only flag server entry points that bind to a port. Sub-apps and
    // factory modules (create-app.ts) don't need aot.
    if !ctx.source.contains(".listen(") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-deploy-prod-no-aot".into(),
        message: "`new Elysia({ ... })` does not set `aot` — for production deployments, set `aot: true` to enable ahead-of-time compilation.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_ts_with_project_and_path;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    fn run_on_at(source: &str, fake_path: &str) -> Vec<Diagnostic> {
        let project = ProjectCtx::for_test_with_framework("elysia");
        run_ts_with_project_and_path(source, &Check, &project, Path::new(fake_path))
    }

    #[test]
    fn flags_config_without_aot_in_root_app() {
        // `.listen()` makes this the root app.
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia({ prefix: '/v1' }).listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_config_without_aot_when_no_listen() {
        let src = "import { Elysia } from 'elysia';\nexport const app = new Elysia({ name: 'root' });";
        assert!(run_on_at(src, "src/index.ts").is_empty());
    }

    #[test]
    fn allows_aot_true() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia({ aot: true }).listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_constructor() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const app = new Elysia({ prefix: '/v1' }).listen(3000);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }

    #[test]
    fn ignores_plugin_in_middleware_dir() {
        // Regression: plugins must NOT set `aot`. Setting it on a plugin
        // triggers a runtime warning. Only the root app gets flagged.
        let src = "import { Elysia } from 'elysia';\nexport const auth = new Elysia({ name: 'auth', prefix: '/auth' });";
        assert!(run_on_at(src, "src/middleware/auth.ts").is_empty());
    }

    #[test]
    fn ignores_plugin_in_plugins_dir() {
        let src = "import { Elysia } from 'elysia';\nexport const logger = new Elysia({ name: 'logger' });";
        assert!(run_on_at(src, "src/plugins/logger.ts").is_empty());
    }

    #[test]
    fn ignores_plugin_in_arbitrary_file_without_listen() {
        // No `.listen()`, not in an entry filename — treat as plugin.
        let src = "import { Elysia } from 'elysia';\nexport const usersRouter = new Elysia({ prefix: '/users' });";
        assert!(run_on_at(src, "src/users/router.ts").is_empty());
    }
}
