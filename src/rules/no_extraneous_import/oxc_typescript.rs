//! no-extraneous-import OXC backend.
//!
//! Flags imports of devDependency packages from non-test production files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::Path;
use std::sync::Arc;

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

        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
            return;
        };
        if is_test_file(ctx.path) {
            return;
        }
        if crate::rules::path_utils::is_config_file(ctx.path) {
            return;
        }
        if is_build_script(ctx.path) {
            return;
        }
        if is_sample_file(ctx.path) {
            return;
        }

        let specifier = import.source.value.as_str();
        if !is_bare_specifier(specifier) {
            return;
        }

        let root = package_root(specifier);
        let in_runtime = pkg.dependencies.contains_key(root)
            || pkg.peer_dependencies.contains_key(root)
            || pkg.optional_dependencies.contains_key(root);
        if in_runtime {
            return;
        }

        if pkg.dev_dependencies.contains_key(root) {
            let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-extraneous-import".into(),
                message: format!(
                    "`{root}` is a devDependency; production code should import from dependencies, peerDependencies, or optionalDependencies."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn package_root(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        match specifier.find('/') {
            Some(first_slash) => match specifier[first_slash + 1..].find('/') {
                Some(second_slash) => &specifier[..first_slash + 1 + second_slash],
                None => specifier,
            },
            None => specifier,
        }
    } else {
        match specifier.find('/') {
            Some(slash) => &specifier[..slash],
            None => specifier,
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("__tests__")
        || path_str.contains(".test.")
        || path_str.contains(".spec.")
        || path_str.contains(".stories.")
        || path_str.contains(".setup.")
        || path_str.contains("/test/")
        || path_str.contains("/tests/")
        || path_str.contains("/e2e/")
}

/// Build/codegen scripts under a `scripts/` directory run at dev/CI time and
/// are not part of the shipped bundle, so importing a devDependency from them
/// is the correct classification — promoting the tool to `dependencies` would
/// wrongly bloat the production dependency closure.
fn is_build_script(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("/scripts/") || s.starts_with("scripts/")
}

/// Demonstration code under `samples/`, `samples-dev/`, `examples/`, or
/// `example-apps/` is compiled and run at dev time to show library usage; it is
/// never bundled into the shipped package. Such files intentionally import peer
/// libraries (e.g. auth providers) declared as devDependencies, so importing a
/// devDependency from them is the correct classification.
fn is_sample_file(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    ["samples", "samples-dev", "examples", "example-apps"]
        .iter()
        .any(|dir| s.contains(&format!("/{dir}/")) || s.starts_with(&format!("{dir}/")))
}

fn is_bare_specifier(spec: &str) -> bool {
    !spec.is_empty()
        && !spec.starts_with('.')
        && !spec.starts_with('/')
        && !spec.starts_with("node:")
}

#[cfg(test)]
mod tests {
    //! Regression tests for issue #101: false positives on devDependencies
    //! (vitest, @testing-library/*) imported from `*.test.{ts,tsx}` and
    //! `vitest.config.*` files.

    use super::Check;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::{CheckCtx, OxcCheck};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn run_with_pkg_at_path(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let lang = Language::from_path(&file_path).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: lang,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();

        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            Language::JavaScript => SourceType::cjs(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, source, &project);

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
    fn allows_vitest_in_dot_test_tsx_file() {
        // Issue #101: `src/app/features/auth/components/login-form.test.tsx`
        // importing vitest + @testing-library/* must not flag.
        let pkg = r#"{
            "dependencies": {"react": "^19"},
            "devDependencies": {
                "vitest": "^1",
                "@testing-library/react": "^14",
                "@testing-library/user-event": "^14"
            }
        }"#;
        let src = r#"
import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
"#;
        let d = run_with_pkg_at_path(
            pkg,
            "src/app/features/auth/components/login-form.test.tsx",
            src,
        );
        assert!(d.is_empty(), "test file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_vitest_in_dot_test_ts_file() {
        // Issue #101: `src/app/lib/form-server-errors.test.ts`
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe, expect, it } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/app/lib/form-server-errors.test.ts", src);
        assert!(d.is_empty(), "test file should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_vitest_in_vitest_config_file() {
        // Issue #101: vitest.config.{ts,mts} importing from "vitest/config"
        // must not flag — `*.config.*` is treated as tooling.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { defineConfig } from "vitest/config";
export default defineConfig({});"#;
        let d = run_with_pkg_at_path(pkg, "vitest.config.ts", src);
        assert!(d.is_empty(), "vitest.config.ts should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_build_script() {
        // Issue #286: a codegen script under `scripts/` runs at dev/CI time and
        // is not part of the shipped bundle — importing a devDependency is correct.
        let pkg = r#"{"devDependencies":{"@tanstack/router-generator":"^1"}}"#;
        let src = r#"import { Generator, getConfig } from "@tanstack/router-generator";"#;
        let d = run_with_pkg_at_path(pkg, "scripts/generate-routes.ts", src);
        assert!(d.is_empty(), "build script should not flag devDeps: {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_samples_dev_file() {
        // Issue #1073: Azure SDK `samples-dev/` files are compiled and run as
        // documentation examples; `@azure/identity` is intentionally a
        // devDependency. Demonstration code must not flag.
        let pkg = r#"{
            "dependencies": {"@azure/core-client": "^1"},
            "devDependencies": {"@azure/identity": "^4"}
        }"#;
        let src = r#"import { DefaultAzureCredential } from "@azure/identity";"#;
        let d = run_with_pkg_at_path(pkg, "samples-dev/managementGroupsGetSample.ts", src);
        assert!(d.is_empty(), "samples-dev file should not flag devDeps: {d:?}");
    }

    #[test]
    fn still_flags_dev_dep_outside_sample_dirs() {
        // Guard against over-relaxing: a path that merely contains "samples" as a
        // substring of another segment (not its own directory) must still flag.
        let pkg = r#"{"devDependencies":{"@azure/identity":"^4"}}"#;
        let src = r#"import { DefaultAzureCredential } from "@azure/identity";"#;
        let d = run_with_pkg_at_path(pkg, "src/mysamples/index.ts", src);
        assert_eq!(d.len(), 1, "non-sample dir should still flag: {d:?}");
        assert!(d[0].message.contains("@azure/identity"));
    }

    #[test]
    fn still_flags_dev_dep_in_production_code() {
        // Guard against over-relaxing: production code outside test/config
        // paths must still flag devDependency imports.
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = r#"import { describe } from "vitest";"#;
        let d = run_with_pkg_at_path(pkg, "src/app/features/auth/login.ts", src);
        assert_eq!(d.len(), 1, "production code should still flag: {d:?}");
        assert!(d[0].message.contains("vitest"));
    }
}
