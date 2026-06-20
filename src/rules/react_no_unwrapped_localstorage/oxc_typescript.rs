//! react-no-unwrapped-localstorage oxc backend.
//!
//! Flags every `localStorage.<method>` member access whose ancestor
//! chain does not include a `TryStatement`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["localStorage"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // This is a React-specific rule. A Vue / Solid / Preact / Qwik file
        // handles its own storage concerns and must not be judged by it.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "localStorage" {
            return;
        }

        // Walk ancestors — if any is a TryStatement body, skip.
        for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
            if let AstKind::TryStatement(try_stmt) = ancestor.kind() {
                // Make sure we are inside the try block body, not catch/finally.
                let body_span = try_stmt.block.span();
                let node_start = member.span.start;
                if node_start >= body_span.start && node_start < body_span.end {
                    return;
                }
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`localStorage` access outside a `try`/`catch` — throws in private-browsing mode, \
                     SSR, or on quota errors. Wrap in `try { ... } catch { ... }`."
                .into(),
            severity: Severity::Warning,
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_unwrapped_in_react_file() {
        let src = "import { useEffect } from 'react';\n\
                   useEffect(() => { localStorage.setItem('k', 'v'); });";
        assert_eq!(run(src).len(), 1, "unexpected: {:?}", run(src));
    }

    #[test]
    fn allows_wrapped_in_react_file() {
        let src = "import { useEffect } from 'react';\n\
                   try { localStorage.setItem('k', 'v'); } catch (e) {}";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }

    #[test]
    fn skips_vue_file() {
        // Regression for issue #4991: a Pinia store in a Vue-only project (no
        // React dependency) must not be judged by this React-specific rule.
        let src = "import { inject, ref } from 'vue';\n\
                   import { defineStore } from 'pinia';\n\
                   export const useAppStore = defineStore('app', () => {\n\
                       const toggleDarkMode = () => { localStorage.theme = 'dark'; };\n\
                       const applyTheme = theme => { localStorage.theme = theme; };\n\
                   });";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }
}
