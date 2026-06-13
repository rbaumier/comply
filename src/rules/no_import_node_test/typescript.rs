//! no-import-node-test backend — flag `import ... from 'node:test'` only when
//! the package also uses vitest/jest (mixing test runners).

use crate::diagnostic::{Diagnostic, Severity};

/// Strip surrounding quotes from a tree-sitter string literal.
fn unquote(spec: &str) -> &str {
    spec.trim_matches(|c| c == '\'' || c == '"' || c == '`')
}

/// True if `spec` resolves to vitest or jest as imported from a source file.
fn is_test_runner_specifier(spec: &str) -> bool {
    spec == "vitest"
        || spec.starts_with("vitest/")
        || spec == "jest"
        || spec == "@jest/globals"
}

crate::ast_check! { on ["program"] prefilter = ["node:test"] => |node, source, ctx, diagnostics|
    let mut node_test_imports = Vec::new();
    let mut same_file_has_runner = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "import_statement" {
            continue;
        }
        let Some(src) = child.child_by_field_name("source") else { continue };
        let spec = unquote(src.utf8_text(source).unwrap_or(""));
        if spec == "node:test" {
            node_test_imports.push(child);
        } else if is_test_runner_specifier(spec) {
            same_file_has_runner = true;
        }
    }

    if node_test_imports.is_empty() {
        return;
    }

    let mixes_runners = same_file_has_runner
        || ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.has_dep_or_engine("vitest") || pkg.has_dep_or_engine("jest"));
    if !mixes_runners {
        return;
    }

    for import in node_test_imports {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &import,
            "no-import-node-test",
            "Importing from `node:test` mixes test runners; use vitest/jest APIs instead.".into(),
            Severity::Warning,
        ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Run with a package.json and the source written to `rel_path` under it.
    fn run_with_pkg(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            canon.to_str().unwrap(),
            &project,
            &crate::rules::file_ctx::FileCtx::default(),
        )
    }

    #[test]
    fn allows_node_test_as_sole_runner() {
        // Issue #1401: astro packages/internal-helpers uses node:test exclusively.
        let pkg = r#"{"devDependencies":{"astro":"^4"}}"#;
        let src = "import assert from 'node:assert/strict';\n\
                   import { describe, it } from 'node:test';";
        let d = run_with_pkg(pkg, "test/path.test.ts", src);
        assert!(d.is_empty(), "sole node:test runner must not flag: {d:?}");
    }

    #[test]
    fn flags_node_test_when_package_uses_vitest() {
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = "import { describe, it } from 'node:test';";
        let d = run_with_pkg(pkg, "test/path.test.ts", src);
        assert_eq!(d.len(), 1, "node:test in a vitest package mixes runners");
    }

    #[test]
    fn flags_node_test_when_package_uses_jest() {
        let pkg = r#"{"devDependencies":{"jest":"^29"}}"#;
        let src = "import test from 'node:test';";
        let d = run_with_pkg(pkg, "src/foo.test.ts", src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_node_test_mixed_with_vitest_in_same_file() {
        // No vitest dep declared, but the file itself imports both runners.
        let pkg = r#"{"devDependencies":{"astro":"^4"}}"#;
        let src = "import { describe, it } from 'node:test';\n\
                   import { expect } from 'vitest';";
        let d = run_with_pkg(pkg, "test/path.test.ts", src);
        assert_eq!(d.len(), 1, "same-file mixing must flag");
    }

    #[test]
    fn allows_vitest_only_import() {
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = "import { describe, it } from 'vitest';";
        let d = run_with_pkg(pkg, "test/path.test.ts", src);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_other_node_builtin() {
        let pkg = r#"{"devDependencies":{"vitest":"^1"}}"#;
        let src = "import { readFile } from 'node:fs';";
        let d = run_with_pkg(pkg, "src/foo.ts", src);
        assert!(d.is_empty());
    }
}
