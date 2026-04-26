//! fsd-no-global-store-imports — Lower FSD layers (`entities`, `shared`,
//! `widgets`) must not import the global store directly. Doing so inverts
//! the dependency direction of Feature-Sliced Design: lower-level building
//! blocks become coupled to top-level state, preventing reuse and testing.
//!
//! Detection is path-based:
//! - Current file lives under `entities/`, `shared/`, or `widgets/`.
//! - Import specifier ends with `/store` or contains `/store/`.

use std::path::{Component, Path};

use crate::diagnostic::{Diagnostic, Severity};

const LOWER_LAYERS: &[&str] = &["entities", "shared", "widgets"];

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(source_node) = node.child_by_field_name("source") else { return };
    let import_path = source_node
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');

    if import_path.is_empty() {
        return;
    }

    if !file_is_in_lower_layer(ctx.path) {
        return;
    }

    if !import_references_store(import_path) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &source_node,
        super::META.id,
        "Lower FSD layers must not import the global store. Use dependency injection instead."
            .to_string(),
        Severity::Warning,
    ));
}

/// Return true when any path component matches a lower FSD layer.
fn file_is_in_lower_layer(path: &Path) -> bool {
    path.components().any(|c| match c {
        Component::Normal(s) => s.to_str().is_some_and(|s| LOWER_LAYERS.contains(&s)),
        _ => false,
    })
}

/// Match import specifiers that reference a `store` module: either the
/// specifier ends with `/store` (or is bare `store`) or contains a
/// `/store/` segment somewhere along the path.
fn import_references_store(spec: &str) -> bool {
    let trimmed = spec.trim_end_matches('/');
    if trimmed == "store" || trimmed.ends_with("/store") {
        return true;
    }
    trimmed.contains("/store/")
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run_at(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_entities_importing_store() {
        let diags = run_at(
            "src/entities/user/model.ts",
            "import { store } from '@/store'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_shared_importing_store_with_subpath() {
        let diags = run_at(
            "src/shared/lib/hooks.ts",
            "import { rootReducer } from '@/app/store/reducer'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_widgets_importing_store_relative() {
        let diags = run_at(
            "src/widgets/header/ui.tsx",
            "import { store } from '../../store'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_features_importing_store() {
        let diags = run_at(
            "src/features/login/model.ts",
            "import { store } from '@/store'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_pages_importing_store() {
        let diags = run_at(
            "src/pages/home/index.tsx",
            "import { store } from '@/app/store'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_entities_importing_non_store() {
        let diags = run_at(
            "src/entities/user/model.ts",
            "import { api } from '@/shared/api'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_entities_importing_storage_lookalike() {
        let diags = run_at(
            "src/entities/user/model.ts",
            "import { storage } from '@/shared/storage'",
        );
        assert!(diags.is_empty());
    }
}
