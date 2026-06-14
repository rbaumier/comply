//! react-no-forward-ref oxc backend — flag `forwardRef(...)` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ImportDeclarationSpecifier, Program};
use std::sync::Arc;

pub struct Check;

/// React's `forwardRef` is exported from `react`; `react-dom` re-exports the
/// React namespace, so both are in-scope. Other packages — notably
/// `@angular/core`, whose `forwardRef(() => Token)` resolves circular DI — are
/// unrelated APIs and must not be flagged.
fn is_react_source(source: &str) -> bool {
    source == "react" || source == "react-dom"
}

/// The local binding `callee` of a `forwardRef(...)` / `<ns>.forwardRef(...)`
/// call. For a bare call this is the named binding `forwardRef`; for a member
/// call it is the namespace/default object (e.g. `React` in `React.forwardRef`).
/// Returns `None` for shapes this rule does not recognise.
fn forward_ref_binding<'a>(callee: &Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::Identifier(id) if id.name.as_str() == "forwardRef" => Some("forwardRef"),
        Expression::StaticMemberExpression(member)
            if member.property.name.as_str() == "forwardRef" =>
        {
            match &member.object {
                Expression::Identifier(obj) => Some(obj.name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// True when `binding` is introduced by an `import` from React. A named call
/// (`binding == "forwardRef"`) must resolve to a named/default specifier whose
/// `local` name matches; a member call resolves to the namespace/default object
/// binding. Keying on the binding's import provenance — not the literal name —
/// lets a file import both React's and Angular's `forwardRef` and only flag the
/// React one.
fn binding_imported_from_react(program: &Program<'_>, binding: &str) -> bool {
    program.body.iter().any(|stmt| {
        let oxc_ast::ast::Statement::ImportDeclaration(import) = stmt else {
            return false;
        };
        if !is_react_source(import.source.value.as_str()) {
            return false;
        }
        let Some(specifiers) = &import.specifiers else {
            return false;
        };
        specifiers.iter().any(|spec| {
            let local = match spec {
                ImportDeclarationSpecifier::ImportSpecifier(s) => s.local.name.as_str(),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => s.local.name.as_str(),
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => s.local.name.as_str(),
            };
            local == binding
        })
    })
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        let Some(binding) = forward_ref_binding(&call.callee) else {
            return;
        };
        // Only React's `forwardRef` is deprecated; `@angular/core`'s same-named
        // DI helper is a different API. Gate on the binding's import source.
        if !binding_imported_from_react(semantic.nodes().program(), binding) {
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

    const FORWARD_REF_SRC: &str = r#"
        import { forwardRef } from "react";
        const Btn = forwardRef((props, ref) => <button ref={ref} />);
    "#;

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

    // Regression for #1612: Angular's `forwardRef` from `@angular/core` resolves
    // circular DI references — an unrelated API that must not be flagged.
    #[test]
    fn ignores_angular_forward_ref() {
        let src = r#"
            import { Component, forwardRef } from "@angular/core";
            @Component({ imports: [forwardRef(() => DocViewer)] })
            export class CodeSnippet {}
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A bare `forwardRef(...)` with no import cannot be proven to be React's, so
    // it must not be flagged.
    #[test]
    fn ignores_forward_ref_without_import() {
        let src = r#"const Btn = forwardRef((props, ref) => <button ref={ref} />);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative space: a file importing BOTH React's and Angular's `forwardRef`
    // under different names must still flag the React one.
    #[test]
    fn flags_react_forward_ref_alongside_angular_import() {
        let src = r#"
            import { forwardRef } from "react";
            import { forwardRef as ngForwardRef } from "@angular/core";
            const Btn = forwardRef((props, ref) => <button ref={ref} />);
            const token = ngForwardRef(() => DocViewer);
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
