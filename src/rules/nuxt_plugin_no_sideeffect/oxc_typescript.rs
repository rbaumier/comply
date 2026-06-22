//! nuxt-plugin-no-sideeffect oxc backend — flag top-level side effects
//! in Nuxt plugin files outside `defineNuxtPlugin(...)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

fn is_plugin_file(ctx_path: &std::path::Path) -> bool {
    ctx_path
        .to_str()
        .map(|p| {
            p.contains("/plugins/")
                || p.contains("\\plugins\\")
                || p.starts_with("plugins/")
                || p.starts_with("plugins\\")
        })
        .unwrap_or(false)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_plugin_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for stmt in &semantic.nodes().program().body {
            let expr_stmt = match stmt {
                Statement::ExpressionStatement(s) => s,
                _ => continue,
            };
            let text = &ctx.source[expr_stmt.span.start as usize..expr_stmt.span.end as usize];
            let trimmed = text.trim();
            if trimmed.starts_with("defineNuxtPlugin(")
                || trimmed.starts_with("export default defineNuxtPlugin(")
            {
                continue;
            }
            if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Top-level side effect in a Nuxt plugin — move it inside `defineNuxtPlugin((nuxtApp) => { ... })`.".into(),
                severity: Severity::Error,
                span: Some((expr_stmt.span.start as usize, (expr_stmt.span.end - expr_stmt.span.start) as usize)),
            });
        }
        diagnostics
    }
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{run_rule, run_rule_gated};

    // Issue #5155 — a `*.spec.ts` file inside `plugins/` is a Vitest spec that
    // exercises a Nuxt plugin (e.g. via `@nuxt/test-utils` `mockNuxtImport`), not
    // a plugin itself. Its top-level `mockNuxtImport`/`it` calls are test setup,
    // not plugin side effects. The central `skip_in_test_dir` gate exempts it.
    const PLUGIN_SPEC: &str = r#"
import { it, expect } from 'vitest'
import { mockNuxtImport } from '@nuxt/test-utils/runtime'

mockNuxtImport('useHead', () => {
  return () => 'mocked-head'
})

it('plugin spec runs under nuxt env', () => {
  expect(globalThis.window).toBeDefined()
})
"#;

    #[test]
    fn gated_no_fp_in_plugin_spec_file() {
        assert!(
            run_rule_gated(
                &Check,
                PLUGIN_SPEC,
                "test/fixtures/simple/plugin-spec/plugins/customFetch.nuxt.spec.ts"
            )
            .is_empty(),
            "skip_in_test_dir must suppress top-level calls in a plugins/ spec file"
        );
    }

    // A real plugin file with a top-level side effect must still be flagged — the
    // exemption is test-spec-specific, not a blanket disable for `plugins/`.
    #[test]
    fn still_fires_on_real_plugin_side_effect() {
        let src = "console.log('boot')\nexport default defineNuxtPlugin(() => {})\n";
        assert_eq!(
            run_rule(&Check, src, "plugins/analytics.ts").len(),
            1,
            "a top-level side effect in a real Nuxt plugin is still flagged"
        );
    }
}
