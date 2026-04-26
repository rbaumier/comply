//! import-namespace backend — verify namespace imports' member accesses resolve to real exports.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let index = ctx.project.import_index();
    if index.is_empty() {
        return;
    }

    let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

    // 1. Collect namespace imports: local_name → resolved source path.
    let mut ns_map: HashMap<String, PathBuf> = HashMap::new();
    for imp in index.get_imports(&canon) {
        if imp.kind == ImportKind::Namespace
            && let Some(src) = &imp.source_path
        {
            ns_map.insert(imp.local_name.clone(), src.clone());
        }
    }

    if ns_map.is_empty() {
        return;
    }

    // 2. For each source module, collect exported names. Skip if the module
    //    has `export * from '…'` — we can't enumerate transitive exports.
    let mut exports_by_source: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for src in ns_map.values() {
        if exports_by_source.contains_key(src) {
            continue;
        }
        let exports = index.get_exports(src);
        let has_star = exports.iter().any(|e| e.kind == ExportKind::StarReExport);
        if has_star {
            continue;
        }
        let names: HashSet<String> = exports.iter().map(|e| e.name.clone()).collect();
        exports_by_source.insert(src.clone(), names);
    }

    // 3. Walk this program's descendants manually (we only have the root node,
    //    not the full `Tree`, so we can't reuse `walker::walk_tree`).
    let mut cursor = node.walk();
    let mut progressed = cursor.goto_first_child();
    while progressed {
        let child = cursor.node();
        if !(child.is_error() || child.is_missing()) && child.kind() == "member_expression" {
            inspect_member(child, source, &ns_map, &exports_by_source, ctx, diagnostics);
        }

        if !(child.is_error() || child.is_missing()) && cursor.goto_first_child() {
            continue;
        }
        // Move to next sibling, climbing up when exhausted — stop when we
        // would leave the `program` subtree.
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                progressed = false;
                break;
            }
            if cursor.node().id() == node.id() {
                progressed = false;
                break;
            }
        }
    }
}

fn inspect_member(
    member: tree_sitter::Node,
    source: &[u8],
    ns_map: &HashMap<String, PathBuf>,
    exports_by_source: &HashMap<PathBuf, HashSet<String>>,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(obj) = member.child_by_field_name("object") else {
        return;
    };
    if obj.kind() != "identifier" {
        return;
    }
    let Ok(obj_name) = std::str::from_utf8(&source[obj.byte_range()]) else {
        return;
    };

    let Some(src_path) = ns_map.get(obj_name) else {
        return;
    };
    let Some(export_names) = exports_by_source.get(src_path) else {
        return;
    };

    let Some(prop) = member.child_by_field_name("property") else {
        return;
    };
    if prop.kind() != "property_identifier" {
        return;
    }
    let Ok(prop_name) = std::str::from_utf8(&source[prop.byte_range()]) else {
        return;
    };

    if !export_names.contains(prop_name) {
        let pos = prop.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "import-namespace".into(),
            message: format!("`{prop_name}` is not exported by the source module."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_ts_with_project_and_path;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> (TempDir, ProjectCtx, Vec<PathBuf>) {
        let dir = TempDir::new().unwrap();
        let mut source_files = Vec::new();
        let mut paths = Vec::new();

        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p.clone(),
                language: lang,
            });
            paths.push(fs::canonicalize(&p).unwrap());
        }

        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        (dir, project, paths)
    }

    #[test]
    fn flags_nonexistent_member() {
        let (_dir, project, paths) = setup_project(&[
            (
                "utils.ts",
                "export const add = (a: number, b: number) => a + b;\n\
                 export const subtract = (a: number, b: number) => a - b;",
            ),
            (
                "app.ts",
                "import * as utils from './utils';\nutils.multiply(1, 2);",
            ),
        ]);
        let source = "import * as utils from './utils';\nutils.multiply(1, 2);";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[1]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("multiply"));
    }

    #[test]
    fn allows_existing_member() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const add = (a: number, b: number) => a + b;"),
            (
                "app.ts",
                "import * as utils from './utils';\nutils.add(1, 2);",
            ),
        ]);
        let source = "import * as utils from './utils';\nutils.add(1, 2);";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[1]);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_star_reexport() {
        let (_dir, project, paths) = setup_project(&[
            ("base.ts", "export const x = 1;"),
            (
                "utils.ts",
                "export * from './base';\nexport const add = 1;",
            ),
            (
                "app.ts",
                "import * as utils from './utils';\nutils.anything();",
            ),
        ]);
        let source = "import * as utils from './utils';\nutils.anything();";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[2]);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_bare_specifier() {
        let (_dir, project, paths) = setup_project(&[(
            "app.ts",
            "import * as React from 'react';\nReact.useState();",
        )]);
        let source = "import * as React from 'react';\nReact.useState();";
        let diags = run_ts_with_project_and_path(source, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
