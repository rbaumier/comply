//! perf-route-level-code-split — OXC backend.
//! Flag static `import Foo from './pages/...'` (or routes/views/) patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn looks_like_route_module(spec: &str) -> bool {
    spec.contains("/pages/")
        || spec.contains("/routes/")
        || spec.contains("/views/")
        || spec.starts_with("./pages/")
        || spec.starts_with("./routes/")
        || spec.starts_with("./views/")
        || spec.starts_with("../pages/")
        || spec.starts_with("../routes/")
        || spec.starts_with("../views/")
}

fn is_test_or_e2e(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/e2e/")
        || s.contains("__tests__")
        || s.contains(".test.")
        || s.contains(".spec.")
        || s.contains(".stories.")
        || s.contains("/test/")
        || s.contains("/tests/")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/pages/", "/routes/", "/views/"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        if is_test_or_e2e(ctx.path) {
            return;
        }

        // Skip `import type ...`.
        if import.import_kind.is_type() {
            return;
        }

        let spec = import.source.value.as_str();
        if !looks_like_route_module(spec) {
            return;
        }

        // Must have a binding (default, named, or namespace import).
        let has_binding = import.specifiers.as_ref().is_some_and(|s| !s.is_empty());
        if !has_binding {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Static import of route module '{spec}' — wrap it in `React.lazy(() => import('{spec}'))` so the route bundle is split."
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_static_pages_import() {
        assert_eq!(run("import Home from './pages/Home';").len(), 1);
    }

    #[test]
    fn flags_routes_subdir_import() {
        assert_eq!(run("import Settings from '../routes/Settings';").len(), 1);
    }

    #[test]
    fn flags_views_import() {
        assert_eq!(run("import Dashboard from 'src/views/Dashboard';").len(), 1);
    }

    #[test]
    fn allows_type_only_route_import() {
        assert!(run("import type { Props } from './pages/Home';").is_empty());
    }

    #[test]
    fn allows_non_route_import() {
        assert!(run("import { clsx } from 'clsx';").is_empty());
    }

    #[test]
    fn skips_e2e_files() {
        let d = crate::rules::test_helpers::run_rule(&Check, "import LoginPage from './pages/login.page';", "project/e2e/fixtures.ts");
        assert!(d.is_empty());
    }
}
