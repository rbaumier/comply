//! elysia-deploy-prod-no-aot — OXC backend.
//! Flags root Elysia instances missing `aot: true` in server entry files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_root_app_file(source: &str, path: &std::path::Path) -> bool {
    if crate::oxc_helpers::source_contains(source, ".listen(") {
        return true;
    }

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

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    matches!(
        stem,
        "app" | "index" | "server" | "main" | "create-app" | "createApp" | "bootstrap" | "entry"
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !is_root_app_file(ctx.source, ctx.path) {
            return;
        }

        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ident) = &new_expr.callee else { return };
        if ident.name.as_str() != "Elysia" {
            return;
        }

        let args_start = new_expr.span.start as usize;
        let args_end = new_expr.span.end as usize;
        let args_text = ctx.source.get(args_start..args_end).unwrap_or("");
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        // Only flag when the constructor receives a config object.
        if !norm.contains('{') {
            return;
        }
        if norm.contains("aot:true") || norm.contains("aot:false") {
            return;
        }
        // Only flag server entry points that bind to a port.
        if !ctx.source_contains(".listen(") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new Elysia({ ... })` does not set `aot` — for production deployments, set `aot: true` to enable ahead-of-time compilation.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    fn run_on_at(source: &str, fake_path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path_and_framework(
            source, &Check, fake_path, "elysia",
        )
    }

    #[test]
    fn flags_config_without_aot_in_root_app() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia({ prefix: '/v1' }).listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_config_without_aot_when_no_listen() {
        let src =
            "import { Elysia } from 'elysia';\nexport const app = new Elysia({ name: 'root' });";
        assert!(run_on_at(src, "src/index.ts").is_empty());
    }

    #[test]
    fn allows_aot_true() {
        let src =
            "import { Elysia } from 'elysia';\nconst app = new Elysia({ aot: true }).listen(3000);";
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
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn ignores_plugin_in_middleware_dir() {
        let src = "import { Elysia } from 'elysia';\nexport const auth = new Elysia({ name: 'auth', prefix: '/auth' });";
        assert!(run_on_at(src, "src/middleware/auth.ts").is_empty());
    }

    use crate::rules::test_helpers::run_oxc_ts_with_project;
}
