//! fsd-no-cross-slice-dependency — Feature-Sliced Design forbids a slice
//! at layer `L` from importing another slice at the same layer `L`. For
//! instance, `features/foo/` must not import from `features/bar/`; it
//! should go through a shared layer or the sibling's public API (which
//! is itself a separate layer concern).
//!
//! Detection is path-based:
//! - Extract `(layer, slice)` from the current file path.
//! - Extract `(layer, slice)` from the import specifier (resolving
//!   relative paths against the current file's directory).
//! - Flag when both layers match and slices differ.

use std::path::{Component, Path, PathBuf};

use crate::diagnostic::{Diagnostic, Severity};

const FSD_LAYERS: &[&str] = &[
    "app",
    "processes",
    "pages",
    "widgets",
    "features",
    "entities",
    "shared",
];

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

    let Some((cur_layer, cur_slice)) = layer_and_slice_from_path(ctx.path) else { return };

    let resolved = resolve_import(ctx.path, import_path);
    let Some((imp_layer, imp_slice)) = layer_and_slice_from_path(&resolved) else { return };

    if cur_layer == imp_layer && cur_slice != imp_slice {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &source_node,
            super::META.id,
            format!(
                "Cross-slice import at `{cur_layer}` layer: `{cur_slice}` imports from `{imp_slice}`. Use shared layer or public API."
            ),
            Severity::Warning,
        ));
    }
}

/// Walk path components and return the `(layer, slice)` pair for the
/// first FSD layer segment followed by a slice segment.
fn layer_and_slice_from_path(path: &Path) -> Option<(String, String)> {
    let comps: Vec<&str> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();

    for i in 0..comps.len().saturating_sub(1) {
        if FSD_LAYERS.contains(&comps[i]) {
            let slice = comps[i + 1];
            // Strip any trailing extension from the slice segment (rare,
            // but e.g. a bare `features/foo.ts` file).
            let slice = Path::new(slice)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(slice);
            return Some((comps[i].to_string(), slice.to_string()));
        }
    }
    None
}

/// Resolve an import specifier against the current file's directory.
/// Relative specifiers are joined and normalized; bare / alias specifiers
/// are returned as-is so a containing layer segment can still be found.
fn resolve_import(current_file: &Path, spec: &str) -> PathBuf {
    if spec.starts_with('.') {
        let base = current_file.parent().unwrap_or(Path::new(""));
        normalize(&base.join(spec))
    } else {
        // Strip leading alias markers like `@/` or `~/` so the first real
        // component can match an FSD layer name.
        let stripped = spec
            .strip_prefix("@/")
            .or_else(|| spec.strip_prefix("~/"))
            .unwrap_or(spec);
        PathBuf::from(stripped)
    }
}

/// Collapse `.` and `..` without touching the filesystem.
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
    fn flags_cross_slice_at_features_layer_relative() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from '../bar/api'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_cross_slice_with_alias() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from '@/features/bar/api'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_cross_slice_at_entities_layer() {
        let diags = run_at(
            "src/entities/user/model.ts",
            "import { x } from '../post/model'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_same_slice_import() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from './api'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_import_from_different_layer() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from '@/shared/ui/button'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_import_from_lower_layer_relative() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from '../../entities/user'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_external_package_import() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { useState } from 'react'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_file_outside_any_layer() {
        let diags = run_at(
            "src/utils/helpers.ts",
            "import { x } from '@/features/foo'",
        );
        assert!(diags.is_empty());
    }
}
