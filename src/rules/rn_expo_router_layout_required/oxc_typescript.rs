use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn file_is_layout(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem == "_layout")
}

fn dir_has_layout(dir: &std::path::Path) -> bool {
    let Ok(read) = std::fs::read_dir(dir) else {
        return true;
    };
    for entry in read.flatten() {
        let p = entry.path();
        if file_is_layout(&p) {
            return true;
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["expo-router"])
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
        if import.source.value.as_str() != "expo-router" {
            return;
        }
        // Expo Router routes live under an `app/` directory; `_layout` only has
        // meaning there. Files outside the `app/` tree — library packages,
        // server-side code, test utilities — import `expo-router` for its types
        // and have no router layout to provide.
        if !ctx.file.path_segments.in_app_router {
            return;
        }
        if file_is_layout(ctx.path) {
            return;
        }
        let Some(dir) = ctx.path.parent() else {
            return;
        };
        if dir_has_layout(dir) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Directory imports `expo-router` but is missing a `_layout` file.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::files::Language;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;
    use crate::rules::test_helpers::run_oxc_check;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Write `source` to `path` (creating parent dirs), build the production
    /// `FileCtx` from that on-disk path so path-classification matches the real
    /// engine, then run the rule.
    fn run_at(path: &Path, source: &str) -> Vec<Diagnostic> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
        let project = ProjectCtx::empty();
        let file = FileCtx::build(path, source, Language::Tsx, &project);
        run_oxc_check(&Check, source, path, &project, &file)
    }

    #[test]
    fn library_package_importing_expo_router_types_is_not_flagged() {
        // Issue #1646: a library package source file imports `expo-router` for
        // its types but lives outside any `app/` tree — no route layout applies.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("packages/router-server/src/routes-manifest.ts");
        let src = "import type { ExpoRouterServerManifestV1 } from 'expo-router/build/routes-manifest';";
        assert!(run_at(&path, src).is_empty());
    }

    #[test]
    fn e2e_test_utility_importing_expo_router_types_is_not_flagged() {
        // Issue #1646: a Jest/e2e test imports `expo-router` types to test CLI
        // export behavior; it is not an app route directory.
        let dir = TempDir::new().unwrap();
        let path = dir
            .path()
            .join("packages/cli/e2e/__tests__/export/static-redirects-api.test.ts");
        let src = "import type { RedirectConfig } from 'expo-router';";
        assert!(run_at(&path, src).is_empty());
    }

    #[test]
    fn app_route_missing_layout_still_fires() {
        // Negative space: a real Expo Router route under `app/` with no
        // `_layout` sibling must still be flagged.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("app/profile/index.tsx");
        let src = "import { Link } from 'expo-router';";
        assert_eq!(run_at(&path, src).len(), 1);
    }

    #[test]
    fn app_route_with_layout_sibling_is_not_flagged() {
        // A route under `app/` that has a `_layout` sibling is fine.
        let dir = TempDir::new().unwrap();
        let route_dir = dir.path().join("app/profile");
        fs::create_dir_all(&route_dir).unwrap();
        fs::write(route_dir.join("_layout.tsx"), "export default function L() {}").unwrap();
        let path = route_dir.join("index.tsx");
        let src = "import { Stack } from 'expo-router';";
        assert!(run_at(&path, src).is_empty());
    }
}
