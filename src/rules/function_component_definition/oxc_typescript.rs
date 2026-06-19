//! function-component-definition OXC backend — flag React arrow-function components.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_test_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("/tests/")
        || s.contains("\\tests\\")
}

fn starts_with_uppercase(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// Check if any node under `start` contains JSX by iterating all nodes
/// whose byte range falls within the start node's span.
fn contains_jsx(start: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let start_span = match start.kind() {
        AstKind::VariableDeclarator(d) => d.span,
        _ => return false,
    };
    for node in semantic.nodes().iter() {
        if let AstKind::JSXOpeningElement(el) = node.kind()
            && el.span.start >= start_span.start && el.span.end <= start_span.end {
                return true;
            }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return;
        }

        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };

        let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &decl.id else {
            return;
        };
        let name = binding.name.as_str();
        if !starts_with_uppercase(name) {
            return;
        }

        let Some(Expression::ArrowFunctionExpression(_arrow)) = &decl.init else {
            return;
        };

        // Check if the arrow function body contains JSX.
        if !contains_jsx(node, semantic) {
            return;
        }

        // Non-React JSX frameworks (Solid, Vue, Preact, Qwik, Stencil) use JSX
        // but not React; arrow-function components are idiomatic there, so the
        // React-only "use a function declaration" guidance does not apply. The
        // framework is detected via a framework import, an in-file
        // `@jsxImportSource` pragma, or the nearest `tsconfig.json`'s
        // `compilerOptions.jsxImportSource` (project-wide JSX factory).
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        let span = match &decl.init {
            Some(expr) => oxc_span::GetSpan::span(expr),
            None => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "React component `{name}` should be a `function` declaration, not an arrow function."
            ),
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    /// Run the check against `source` placed at `importer_rel`, with a
    /// `tsconfig.json` written at `tsconfig_rel`, both under a fresh temp dir.
    /// Exercises the on-disk tsconfig `jsxImportSource` lookup the rule performs.
    fn run_with_tsconfig(
        importer_rel: &str,
        source: &str,
        tsconfig_rel: &str,
        tsconfig: &str,
    ) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        let ts_path = dir.path().join(tsconfig_rel);
        fs::create_dir_all(ts_path.parent().unwrap()).unwrap();
        fs::write(&ts_path, tsconfig).unwrap();
        let importer = dir.path().join(importer_rel);
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, source).unwrap();
        let canon = fs::canonicalize(&importer).unwrap();
        let source_file = SourceFile {
            path: canon.clone(),
            language: Language::from_path(&canon).unwrap(),
        };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_react_arrow_component() {
        let src = "export const Display = (props) => <div>{props.x}</div>;";
        assert_eq!(run(src).len(), 1);
    }

    // Regression test for #1924: a Solid.js arrow-function component that
    // imports from `solid-js/web` must not be flagged — Solid is not React.
    #[test]
    fn allows_solid_arrow_component() {
        let src = r#"import { createStore, useSelector } from '@tanstack/solid-store'
import { render } from 'solid-js/web'

export const Display = (props) => {
  const count = useSelector(store, (state) => state[props.animals])
  return <div>{props.animals}: {count()}</div>
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_solid_arrow_component_bare_import() {
        let src = r#"import { createSignal } from 'solid-js'

export const Counter = () => {
  const [count] = createSignal(0)
  return <div>{count()}</div>
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_solidstart_arrow_component_via_tsconfig_jsx_import_source() {
        // A SolidJS `.tsx` arrow component with NO per-file `solid-js` import —
        // the JSX factory comes solely from the package tsconfig's
        // `compilerOptions.jsxImportSource: "solid-js"`. Must not be flagged.
        // (Closes #3235)
        let diags = run_with_tsconfig(
            "packages/start/src/server/assets/PatchVirtualDevStyles.tsx",
            "const PatchVirtualDevStyles = (props: { nonce?: string }) => {\n\
             \x20 return <script nonce={props.nonce} />;\n\
             };",
            "packages/start/tsconfig.json",
            r#"{"compilerOptions":{"jsx":"preserve","jsxImportSource":"solid-js"}}"#,
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn flags_react_arrow_component_via_tsconfig_react_jsx_import_source() {
        // A real React project whose tsconfig sets `jsxImportSource: "react"`
        // (or omits it) — a `.tsx` arrow component must still be flagged.
        let diags = run_with_tsconfig(
            "src/App.tsx",
            "const App = () => <div />;",
            "tsconfig.json",
            r#"{"compilerOptions":{"jsx":"react-jsx","jsxImportSource":"react"}}"#,
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
    }
}
