//! AST backend — flag static `import Foo from './pages/...'` (or `routes/`,
//! `views/`) patterns. These should be wrapped in `React.lazy(() => import(...))`
//! so the router-level bundle only contains the shell, not every page.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the module path matches a common route-component pattern:
/// it contains a `/pages/`, `/routes/`, or `/views/` segment.
fn looks_like_route_module(spec: &str) -> bool {
    let inner = spec.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    inner.contains("/pages/")
        || inner.contains("/routes/")
        || inner.contains("/views/")
        || inner.starts_with("./pages/")
        || inner.starts_with("./routes/")
        || inner.starts_with("./views/")
        || inner.starts_with("../pages/")
        || inner.starts_with("../routes/")
        || inner.starts_with("../views/")
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    // Skip type-only imports — they erase at build time and don't ship.
    let node_text = node.utf8_text(source).unwrap_or("");
    if node_text.trim_start().starts_with("import type") {
        return;
    }

    let Some(src_node) = node.child_by_field_name("source") else { return };
    let Ok(text) = src_node.utf8_text(source) else { return };
    if !looks_like_route_module(text) { return; }

    // Only flag default/named component imports — bare `import './x'` (side-effect
    // imports) don't pull a component into scope.
    let mut cursor = node.walk();
    let has_binding = node.children(&mut cursor).any(|c| {
        matches!(
            c.kind(),
            "import_clause" | "identifier" | "named_imports" | "namespace_import"
        )
    });
    if !has_binding { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Static import of route module {text} — wrap it in `React.lazy(() => import({text}))` so the route bundle is split."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
}
