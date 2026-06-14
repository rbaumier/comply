//! react-jsx-no-bind OxcCheck backend. Files in a non-React JSX framework
//! package — the nearest `package.json` declares `vue` or `solid-js` and not
//! `react` — are exempt, as are files importing from `solid-js`: Vue's own
//! reactivity and Solid's fine-grained reactivity mean a fresh inline function
//! per render is not a re-render hazard and `useCallback` does not apply.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Module-level JSX is evaluated exactly once: there is no render cycle, so an
/// inline function cannot create per-render reference churn and `useCallback`
/// is not even usable there.
fn is_inside_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    semantic
        .nodes()
        .ancestors(node.id())
        .skip(1)
        .any(|a| matches!(a.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.source_contains("solid-js") {
            return;
        }
        // A file belonging to a Vue or Solid package (nearest `package.json`
        // declares `vue`/`solid-js` and not `react`) writes JSX/TSX for a
        // framework with its own reactivity, where a fresh inline function per
        // render is not a re-render hazard. React-named rules stay on when the
        // package declares `react` (or both) or has no resolvable framework dep.
        if crate::oxc_helpers::in_non_react_framework_package(ctx.project, ctx.path) {
            return;
        }
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        if !is_inside_function(node, semantic) {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };

            // Get the attribute name
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let attr_name = name_ident.name.as_str();

            // `ref` is not a diffed prop: React assigns it outside the
            // render/prop path and never re-renders on ref identity change, so
            // an inline ref callback (the standard array-of-refs pattern) is
            // not a churn concern.
            if attr_name == "ref" {
                continue;
            }

            // Value must be an expression container
            let Some(JSXAttributeValue::ExpressionContainer(ec)) = &attr.value else {
                continue;
            };

            let expr = match &ec.expression {
                JSXExpression::EmptyExpression(_) => continue,
                other => other,
            };

            let (kind_label, span) = match expr {
                JSXExpression::ArrowFunctionExpression(arrow) => {
                    ("arrow function", arrow.span)
                }
                JSXExpression::FunctionExpression(func) => {
                    ("function expression", func.span)
                }
                JSXExpression::CallExpression(call) => {
                    // Detect `foo.bind(...)`
                    let Expression::StaticMemberExpression(member) = &call.callee else {
                        continue;
                    };
                    if member.property.name.as_str() != "bind" {
                        continue;
                    }
                    ("`.bind()` call", call.span())
                }
                _ => continue,
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render \u{2014} hoist to `useCallback` or a stable handler."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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
    fn flags_arrow_in_jsx_prop_react() {
        let src = "function App() { return <button onClick={() => f()} />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arrow_in_jsx_prop_solid() {
        let src = "import { createSignal } from \"solid-js\";\nfunction App() { return <button onClick={() => f()} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bind_in_jsx_prop_solid() {
        let src = "import { createSignal } from \"solid-js\";\nfunction App() { return <button onClick={this.f.bind(this)} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_module_level_jsx_issue_1053() {
        // Regression for issue #1053: module-level JSX is evaluated once,
        // no render cycle, so inline functions are not a re-render hazard.
        let src = "const state = { description: <Trans bold={(text) => <strong>{text}</strong>} br={() => <br />} /> };";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }

    #[test]
    fn flags_jsx_inside_component_issue_1053() {
        let src = "function App() { return <button onClick={() => f()} />; }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_arrow_on_ref_attr_issue_1965() {
        // Regression for issue #1965: per-index ref callbacks in a `.map(...)`
        // are the standard array-of-refs pattern; `useCallback` cannot capture
        // a distinct index per element, and `ref` does not trigger re-renders.
        let src = "function App() { return views.map((view, index) => <HeaderControl ref={(node) => { r.current[index] = node; }} />); }";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }

    #[test]
    fn allows_bind_on_ref_attr_issue_1965() {
        let src = "function App() { return <div ref={this.setRef.bind(this)} />; }";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }

    #[test]
    fn flags_arrow_on_non_ref_attr_issue_1965() {
        let src = "function App() { return <button onClick={() => doThing()} />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bind_on_non_ref_attr_issue_1965() {
        let src = "function App() { return <button onClick={this.f.bind(this)} />; }";
        assert_eq!(run(src).len(), 1);
    }

    /// Stage a `.tsx` file at `rel_path` under a package whose `package.json` is
    /// `pkg_json`, then lint it so `nearest_package_json` resolves the manifest.
    fn run_with_pkg_at_path(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;
        use oxc_semantic::SemanticBuilder;
        use oxc_span::SourceType;

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::Tsx,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = std::fs::canonicalize(&file_path).unwrap();

        crate::oxc_helpers::reset_file_caches();
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let file_ctx = crate::rules::file_ctx::FileCtx::build(&canon, source, Language::Tsx, &project);
        let ctx = CheckCtx::for_test_full(&canon, source, &project, &file_ctx);

        let mut diagnostics = Vec::new();
        let kinds = Check.interested_kinds();
        for node in semantic.nodes().iter() {
            if kinds.contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn allows_arrow_in_vue_package_tsx_issue_2180() {
        // Issue #2180: a `.tsx` file whose nearest package.json declares `vue`
        // (and not `react`) is Vue JSX. Vue has its own reactivity, so an inline
        // arrow in a JSX prop is not a re-render hazard and must not flag.
        let pkg = r#"{"dependencies":{"vue":"^3"}}"#;
        let src = "import { defineComponent } from 'vue';\nfunction App() { return <input onInput={(e) => setFilter(e.target.value)} />; }";
        let d = run_with_pkg_at_path(pkg, "examples/vue/expanding/src/App.tsx", src);
        assert!(d.is_empty(), "vue package tsx should not flag: {d:?}");
    }

    #[test]
    fn allows_arrow_in_solid_package_tsx_issue_2180() {
        // Issue #2180: same exemption for a `solid-js` package (no `solid-js`
        // import text in the source — the package dependency is the signal).
        let pkg = r#"{"dependencies":{"solid-js":"^1"}}"#;
        let src = "function App() { return <input onInput={(e) => setFilter(e.target.value)} />; }";
        let d = run_with_pkg_at_path(pkg, "examples/solid/src/App.tsx", src);
        assert!(d.is_empty(), "solid package tsx should not flag: {d:?}");
    }

    #[test]
    fn still_flags_arrow_in_react_package_tsx_issue_2180() {
        // Negative-space guard: a `react` package keeps firing — the React
        // re-render concern applies.
        let pkg = r#"{"dependencies":{"react":"^19"}}"#;
        let src = "function App() { return <input onInput={(e) => setFilter(e.target.value)} />; }";
        let d = run_with_pkg_at_path(pkg, "examples/react/src/App.tsx", src);
        assert_eq!(d.len(), 1, "react package tsx should still flag: {d:?}");
    }

    #[test]
    fn still_flags_arrow_with_no_framework_dep_issue_2180() {
        // Negative-space guard: a package with no resolvable framework dep keeps
        // firing — these React-named rules default on.
        let pkg = r#"{"dependencies":{}}"#;
        let src = "function App() { return <input onInput={(e) => setFilter(e.target.value)} />; }";
        let d = run_with_pkg_at_path(pkg, "src/App.tsx", src);
        assert_eq!(d.len(), 1, "no-framework package should still flag: {d:?}");
    }

    #[test]
    fn still_flags_arrow_when_both_react_and_vue_issue_2180() {
        // Ambiguity guard: a package declaring both `react` and `vue` keeps
        // firing — default to the rule's React intent.
        let pkg = r#"{"dependencies":{"react":"^19","vue":"^3"}}"#;
        let src = "function App() { return <input onInput={(e) => setFilter(e.target.value)} />; }";
        let d = run_with_pkg_at_path(pkg, "src/App.tsx", src);
        assert_eq!(d.len(), 1, "react+vue package should still flag: {d:?}");
    }
}
