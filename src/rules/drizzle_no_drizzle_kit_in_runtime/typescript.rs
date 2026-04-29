//! drizzle-no-drizzle-kit-in-runtime — flag any `import` / `require` of
//! `drizzle-kit` (or a `drizzle-kit/...` subpath) outside of files whose
//! path identifies them as configuration / migration tooling.

use crate::diagnostic::{Diagnostic, Severity};

fn is_config_or_migration_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("drizzle.config")
        || s.contains("/migrate")
        || s.contains("/migrations/")
        || s.ends_with("migrate.ts")
        || s.ends_with("migrate.js")
        || s.ends_with("migrate.mjs")
}

fn module_is_drizzle_kit(spec: &str) -> bool {
    let trimmed = spec.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    trimmed == "drizzle-kit" || trimmed.starts_with("drizzle-kit/")
}

crate::ast_check! { on ["import_statement", "call_expression"] prefilter = ["drizzle-kit"] => |node, source, ctx, diagnostics|
    if is_config_or_migration_file(ctx.path) {
        return;
    }
    if node.kind() == "import_statement" {
        // Get the source string literal.
        let Some(src_node) = node.child_by_field_name("source") else { return };
        let raw = src_node.utf8_text(source).unwrap_or("");
        if !module_is_drizzle_kit(raw) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "drizzle-no-drizzle-kit-in-runtime".into(),
            message: "`drizzle-kit` is a dev-time CLI — importing it from runtime code bloats the production bundle.".into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }
    // call_expression — `require('drizzle-kit')`.
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "require" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    if first.kind() != "string" {
        return;
    }
    let raw = first.utf8_text(source).unwrap_or("");
    if !module_is_drizzle_kit(raw) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-no-drizzle-kit-in-runtime".into(),
        message: "`require('drizzle-kit')` in runtime code — keep migration tooling out of the production bundle.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_at(src: &str, fake: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, fake)
    }

    #[test]
    fn flags_import_in_runtime() {
        let src = "import { defineConfig } from 'drizzle-kit';";
        assert_eq!(run_at(src, "src/api/handler.ts").len(), 1);
    }

    #[test]
    fn flags_require_in_runtime() {
        let src = "const k = require('drizzle-kit');";
        assert_eq!(run_at(src, "src/api/handler.ts").len(), 1);
    }

    #[test]
    fn allows_import_in_drizzle_config() {
        let src = "import { defineConfig } from 'drizzle-kit';";
        assert!(run_at(src, "drizzle.config.ts").is_empty());
    }

    #[test]
    fn allows_import_in_migrate_script() {
        let src = "import { drizzle } from 'drizzle-kit/dist';";
        assert!(run_at(src, "scripts/migrate.ts").is_empty());
    }

    #[test]
    fn allows_drizzle_orm_import() {
        let src = "import { drizzle } from 'drizzle-orm';";
        assert!(run_at(src, "src/api/handler.ts").is_empty());
    }
}
