//! react-no-forward-ref oxc backend — flag `forwardRef(...)` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True if `expr` resolves to `forwardRef` — accepts both
/// `forwardRef(...)` (named import) and `React.forwardRef(...)` /
/// `*.forwardRef(...)` (namespace import).
fn callee_is_forward_ref(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == "forwardRef",
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == "forwardRef"
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["forwardRef"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // A project whose React range still admits React 18 must keep
        // `forwardRef` — React 18 has no ref-as-prop API.
        if ctx.project.react_supports_v18(ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !callee_is_forward_ref(&call.callee) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`forwardRef(...)` is deprecated in React 19 — accept `ref` \
                      as a regular prop on the component."
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
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    /// Run the rule against a file that lives in a temp project with the given
    /// `package.json`, so the React-range gate sees a real manifest on disk.
    fn run_in_project(pkg_json: &str, src: &str) -> Vec<Diagnostic> {
        crate::oxc_helpers::reset_file_caches();
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let path = dir.path().join("Btn.tsx");
        fs::write(&path, src).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, src, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test(&path, src);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if Check.interested_kinds().contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    const FORWARD_REF_SRC: &str =
        r#"const Btn = forwardRef((props, ref) => <button ref={ref} />);"#;

    // Issue #2000: a library whose peerDependencies admit React 18 must keep
    // `forwardRef`; the rule must stay silent there.
    #[test]
    fn ignores_forward_ref_when_peer_dep_supports_react18() {
        let pkg = r#"{"name":"lib","peerDependencies":{"react":">=18.0.0"}}"#;
        assert!(run_in_project(pkg, FORWARD_REF_SRC).is_empty());
    }

    #[test]
    fn ignores_forward_ref_when_dep_range_spans_react18_and_19() {
        let pkg = r#"{"name":"lib","dependencies":{"react":"^18 || ^19"}}"#;
        assert!(run_in_project(pkg, FORWARD_REF_SRC).is_empty());
    }

    #[test]
    fn flags_forward_ref_when_project_requires_react19() {
        let pkg = r#"{"name":"app","dependencies":{"react":"^19.0.0"}}"#;
        assert_eq!(run_in_project(pkg, FORWARD_REF_SRC).len(), 1);
    }

    #[test]
    fn flags_forward_ref_when_no_react_declared() {
        let pkg = r#"{"name":"app"}"#;
        assert_eq!(run_in_project(pkg, FORWARD_REF_SRC).len(), 1);
    }

    #[test]
    fn flags_named_forward_ref_call() {
        let src = r#"
            import { forwardRef } from "react";
            const Btn = forwardRef((props, ref) => <button ref={ref} />);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_namespaced_forward_ref_call() {
        let src = r#"
            import * as React from "react";
            const Btn = React.forwardRef((props, ref) => <button ref={ref} />);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_unrelated_calls() {
        let src = r#"const x = doStuff();"#;
        assert!(run(src).is_empty());
    }
}
