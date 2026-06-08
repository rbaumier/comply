//! avoid-importing-barrel-files OXC backend — flag relative imports that
//! resolve to a barrel file.
//!
//! Skipped when the importing file lives under a `routes/` directory: that's
//! the TanStack Router file-system convention where `index.tsx` is the leaf
//! route module for a segment, not a re-export hub.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::Path;
use std::sync::Arc;

const INDEX_SUFFIXES: &[&str] = &[
    "/index",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
    "/index.mjs",
    "/index.cjs",
];

fn is_barrel_path(module: &str) -> bool {
    if !module.starts_with('.') {
        return false;
    }
    if module == "." || module == ".." {
        return true;
    }
    if module.ends_with('/') {
        return true;
    }
    INDEX_SUFFIXES.iter().any(|s| module.ends_with(s))
}

fn is_tanstack_route_file(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == "routes")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let module = import.source.value.as_str();
        if !is_barrel_path(module) {
            return;
        }
        if is_tanstack_route_file(ctx.path) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from barrel file `{module}` — import directly from the source module instead."
            ),
            severity: Severity::Warning,
            span: Some((
                import.span.start as usize,
                (import.span.end - import.span.start) as usize,
            )),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_explicit_index_import() {
        let d = run_on("import { foo } from './utils/index';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("barrel file"));
    }

    #[test]
    fn flags_explicit_index_with_extension() {
        let d = run_on("import { foo } from './utils/index.ts';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_directory_with_trailing_slash() {
        let d = run_on("import { foo } from './utils/';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_current_dir_import() {
        let d = run_on("import { foo } from '.';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_parent_dir_import() {
        let d = run_on("import { foo } from '..';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_file_import() {
        assert!(run_on("import { foo } from './utils/string';").is_empty());
    }

    #[test]
    fn allows_package_import() {
        assert!(run_on("import { useState } from 'react';").is_empty());
    }

    #[test]
    fn allows_file_named_index_like() {
        assert!(run_on("import { foo } from './indexer';").is_empty());
    }

    #[test]
    fn allows_index_import_from_tanstack_route_file() {
        // Regression for #160: TanStack route files (under `routes/`) commonly
        // import `./<segment>/index` as a leaf route module, not a barrel.
        let d = crate::rules::test_helpers::run_rule(&Check, "import { Route } from './_authed/index';", "src/routes/__root.tsx");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }
}
