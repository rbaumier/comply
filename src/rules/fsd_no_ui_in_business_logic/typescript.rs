//! fsd-no-ui-in-business-logic — Feature-Sliced Design forbids business
//! logic segments (`model/`, `api/`, `lib/`) from importing UI code.
//!
//! Detection is path-based:
//! - Current file must live inside a `/model/`, `/api/`, or `/lib/` segment.
//! - Import specifier must reference a `ui/` segment (either resolved via
//!   relative path, alias-stripped bare specifier, or the first real
//!   component).

use std::path::{Component, Path, PathBuf};

use crate::diagnostic::{Diagnostic, Severity};

const BUSINESS_SEGMENTS: &[&str] = &["model", "api", "lib"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" {
        return;
    }

    let Some(source_node) = node.child_by_field_name("source") else { return };
    let import_path = source_node
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');

    if import_path.is_empty() {
        return;
    }

    if !current_file_in_business_segment(ctx.path) {
        return;
    }

    if !import_references_ui(ctx.path, import_path) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &source_node,
        super::META.id,
        "Don't import UI components from business logic layers (model/api/lib).".to_string(),
        Severity::Warning,
    ));
}

fn current_file_in_business_segment(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(c, Component::Normal(s) if s.to_str().is_some_and(|name| BUSINESS_SEGMENTS.contains(&name)))
    })
}

fn import_references_ui(current_file: &Path, spec: &str) -> bool {
    // Bare specifier starting with `ui/` — direct alias like `ui/button`.
    if spec.starts_with("ui/") {
        return true;
    }

    let resolved = resolve_import(current_file, spec);
    resolved.components().any(|c| {
        matches!(c, Component::Normal(s) if s.to_str() == Some("ui"))
    })
}

fn resolve_import(current_file: &Path, spec: &str) -> PathBuf {
    if spec.starts_with('.') {
        let base = current_file.parent().unwrap_or(Path::new(""));
        normalize(&base.join(spec))
    } else {
        let stripped = spec
            .strip_prefix("@/")
            .or_else(|| spec.strip_prefix("~/"))
            .unwrap_or(spec);
        PathBuf::from(stripped)
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(out.last(), Some(Component::Normal(_))) {
                    out.push(comp);
                } else {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }
    out.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run_at(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_model_importing_ui_relative() {
        let diags = run_at(
            "src/entities/user/model/store.ts",
            "import { Button } from '../ui/button'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_api_importing_ui_alias() {
        let diags = run_at(
            "src/features/auth/api/login.ts",
            "import { Spinner } from '@/shared/ui/spinner'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_lib_importing_ui_bare() {
        let diags = run_at(
            "src/shared/lib/format.ts",
            "import { Icon } from 'ui/icon'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_ui_importing_model() {
        let diags = run_at(
            "src/entities/user/ui/card.tsx",
            "import { userStore } from '../model/store'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_model_importing_api() {
        let diags = run_at(
            "src/entities/user/model/store.ts",
            "import { fetchUser } from '../api/fetch'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_model_importing_external_package() {
        let diags = run_at(
            "src/entities/user/model/store.ts",
            "import { create } from 'zustand'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_file_outside_business_segment() {
        let diags = run_at(
            "src/pages/home/index.ts",
            "import { Button } from '@/shared/ui/button'",
        );
        assert!(diags.is_empty());
    }
}
