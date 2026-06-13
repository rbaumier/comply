//! node-no-top-level-await OXC backend.
//!
//! Flags top-level `await` in published library modules. Exempt are test files,
//! `__mocks__/` manual-mock files (Jest/Vitest test infrastructure that uses
//! top-level `await` for `vi.importActual()`), scripts, and entrypoints.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
}

fn is_script_file(path: &std::path::Path, source: &str) -> bool {
    if path.components().any(|c| c.as_os_str() == "scripts") {
        return true;
    }
    source.starts_with("#!")
}

fn is_entrypoint(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, ".listen(") || crate::oxc_helpers::source_contains(source, "process.exit")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["await"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        if is_test_file(ctx.path)
            || crate::rules::path_utils::is_auto_mock_dir_path(ctx.path)
            || is_script_file(ctx.path, ctx.source)
            || is_entrypoint(ctx.source)
        {
            return;
        }

        // Walk up: if inside any function scope, this is not top-level.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_) => {
                    return; // Inside a function — not top-level.
                }
                _ => {}
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Top-level `await` is forbidden in published modules.".into(),
            severity: Severity::Error,
            span: None,
        });
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

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_top_level_await() {
        let d = run_at("const data = await fetch('/api');", "src/load.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Top-level"));
    }

    #[test]
    fn allows_await_in_async_function() {
        let src = "async function load() { const data = await fetch('/api'); }";
        assert!(run_at(src, "src/load.ts").is_empty());
    }

    // Regression for #1757: manual-mock files in `__mocks__/` are Vitest/Jest
    // test infrastructure that legitimately use top-level `await` (e.g.
    // `vi.importActual()`); they never ship in the published package.
    #[test]
    fn allows_top_level_await_in_auto_mock_dir() {
        let src = r#"
import { act } from '@testing-library/react';
import { afterEach, vi } from 'vitest';
import * as zustand from 'zustand';

const { create: actualCreate, createStore: actualCreateStore } =
  await vi.importActual<typeof zustand>('zustand');
"#;
        assert!(run_at(src, "apps/react-vite/__mocks__/zustand.ts").is_empty());
    }
}
