//! fsd-no-relative-imports — Feature-Sliced Design forbids relative imports
//! that traverse across slices or layers. A relative specifier that walks
//! up (`../`) far enough to leave the current slice or land in a different
//! layer is flagged.
//!
//! Detection is path-based:
//! - Only relative specifiers (`./` or `../`) are considered.
//! - Extract `(layer, slice)` from the current file path.
//! - Resolve the import against the current file's directory.
//! - Extract `(layer, slice)` from the resolved path.
//! - Flag when the layers differ, or the layers match but the slices differ.

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

    // Only relative specifiers that walk up (`../`) can cross a slice/layer.
    if !import_path.starts_with("../") {
        return;
    }

    let Some((cur_layer, cur_slice)) = layer_and_slice_from_path(ctx.path) else { return };

    let resolved = resolve_import(ctx.path, import_path);
    let Some((imp_layer, imp_slice)) = layer_and_slice_from_path(&resolved) else {
        // Relative import escapes any known layer — flag as crossing out.
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &source_node,
            super::META.id,
            format!(
                "Relative import from `{cur_layer}/{cur_slice}` escapes the FSD layer structure. Use absolute imports or shared layer for cross-slice dependencies."
            ),
            Severity::Warning,
        ));
        return;
    };

    if cur_layer != imp_layer {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &source_node,
            super::META.id,
            format!(
                "Relative import crosses FSD layers: `{cur_layer}/{cur_slice}` -> `{imp_layer}/{imp_slice}`. Use absolute imports or shared layer for cross-slice dependencies."
            ),
            Severity::Warning,
        ));
    } else if cur_slice != imp_slice {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &source_node,
            super::META.id,
            format!(
                "Relative import crosses FSD slices at `{cur_layer}` layer: `{cur_slice}` -> `{imp_slice}`. Use absolute imports or shared layer for cross-slice dependencies."
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
            let slice = Path::new(slice)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(slice);
            return Some((comps[i].to_string(), slice.to_string()));
        }
    }
    None
}

/// Resolve a relative import specifier against the current file's directory.
fn resolve_import(current_file: &Path, spec: &str) -> PathBuf {
    let base = current_file.parent().unwrap_or(Path::new(""));
    normalize(&base.join(spec))
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
    fn flags_relative_import_crossing_slice_at_same_layer() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from '../bar/api'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_relative_import_crossing_layer() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from '../../entities/user'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_deep_relative_escape_to_another_layer() {
        let diags = run_at(
            "src/features/foo/ui/component.ts",
            "import { x } from '../../../widgets/header'",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_same_slice_sibling_import() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from './api'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_within_slice_parent_import() {
        let diags = run_at(
            "src/features/foo/ui/component.ts",
            "import { x } from '../api'",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_absolute_alias_import() {
        let diags = run_at(
            "src/features/foo/ui.ts",
            "import { x } from '@/shared/ui/button'",
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
            "import { x } from '../other/thing'",
        );
        assert!(diags.is_empty());
    }
}
