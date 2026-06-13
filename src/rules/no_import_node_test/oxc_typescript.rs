//! no-import-node-test oxc backend — flag `import ... from 'node:test'` only
//! when the package also uses vitest/jest (mixing test runners).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::PackageJson;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

/// True if `spec` resolves to vitest or jest as imported from a source file.
fn is_test_runner_specifier(spec: &str) -> bool {
    spec == "vitest"
        || spec.starts_with("vitest/")
        || spec == "jest"
        || spec == "@jest/globals"
}

/// True if the nearest package.json declares vitest or jest as a dependency.
fn package_uses_test_runner(pkg: &PackageJson) -> bool {
    pkg.has_dep_or_engine("vitest") || pkg.has_dep_or_engine("jest")
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["node:test"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut node_test_starts = Vec::new();
        let mut same_file_has_runner = false;
        for stmt in &semantic.nodes().program().body {
            let Statement::ImportDeclaration(import) = stmt else {
                continue;
            };
            let spec = import.source.value.as_str();
            if spec == "node:test" {
                node_test_starts.push(import.span.start);
            } else if is_test_runner_specifier(spec) {
                same_file_has_runner = true;
            }
        }

        if node_test_starts.is_empty() {
            return Vec::new();
        }

        let mixes_runners = same_file_has_runner
            || ctx
                .project
                .nearest_package_json(ctx.path)
                .is_some_and(|pkg| package_uses_test_runner(&pkg));
        if !mixes_runners {
            return Vec::new();
        }

        node_test_starts
            .into_iter()
            .map(|start| {
                let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "Importing from `node:test` mixes test runners; use vitest/jest APIs instead."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for issue #1401: node:test used as the sole test runner
    //! (no vitest/jest in the package) must not be flagged as "mixing runners".

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

    fn run_with_pkg(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
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
        let project = ProjectCtx::load(&[&source_file], &Config::default());
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
        Check.run_on_semantic(&semantic, &ctx)
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
