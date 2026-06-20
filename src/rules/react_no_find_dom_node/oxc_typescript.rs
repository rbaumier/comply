//! react-no-find-dom-node oxc backend.
//!
//! Files for a non-React JSX framework (Vue, Solid, Preact, Qwik, Stencil) are
//! exempt: they may define their own `findDOMNode` helper (Ant Design Vue's
//! walks the VNode tree), unrelated to React's deprecated `ReactDOM.findDOMNode`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findDOMNode"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // `findDOMNode` is a React-only API. A Vue / Solid / Preact / Qwik /
        // Stencil JSX file may define its own helper of the same name (Ant
        // Design Vue's `findDOMNode` walks the VNode tree), so it must not be
        // judged against React's deprecated `ReactDOM.findDOMNode`.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let (matched, span_start) = match &call.callee {
            Expression::StaticMemberExpression(member) => {
                let is_find = member.property.name.as_str() == "findDOMNode";
                (is_find, member.span.start)
            }
            Expression::Identifier(ident) => {
                let is_find = ident.name.as_str() == "findDOMNode";
                (is_find, ident.span.start)
            }
            _ => (false, 0),
        };

        if !matched {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`findDOMNode` is deprecated in React 19 — use refs instead.".into(),
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
    fn flags_react_dom_find_dom_node() {
        let src = "import ReactDOM from 'react-dom'; const n = ReactDOM.findDOMNode(this);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_find_dom_node_from_react_dom() {
        let src = "import { findDOMNode } from 'react-dom'; const n = findDOMNode(ref.current);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_user_defined_find_dom_node_in_vue_file() {
        // Regression for issue #4902: Ant Design Vue defines its own `findDOMNode`
        // utility that walks the VNode tree — unrelated to React's deprecated API.
        let src = "import { defineComponent } from 'vue';\n\
                   import { findDOMNode } from '../_util/props-util';\n\
                   const el = findDOMNode(instance.value);";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }
}
