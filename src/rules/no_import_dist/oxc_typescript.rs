//! no-import-dist OXC backend — flag imports targeting `dist/` build output.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ImportDeclaration, ImportDeclarationSpecifier};
use std::sync::Arc;

pub struct Check;

/// Returns true if `spec` points into a `dist/` directory.
fn targets_dist(spec: &str) -> bool {
    spec.contains("/dist/") || spec.starts_with("dist/")
}

/// Returns true if the import has zero runtime impact: either a top-level
/// `import type { ... }` declaration, or a declaration where every named
/// specifier carries an inline `type` qualifier (`import { type A, type B }`).
/// Such imports pull nothing from the compiled artifact at runtime, so the
/// dist/ check (aimed at runtime use of build output) does not apply.
fn is_type_only(import: &ImportDeclaration) -> bool {
    if import.import_kind.is_type() {
        return true;
    }
    let Some(specifiers) = &import.specifiers else {
        return false;
    };
    let mut saw_named = false;
    for spec in specifiers {
        match spec {
            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                saw_named = true;
                if !named.import_kind.is_type() {
                    return false;
                }
            }
            // A default or namespace specifier is always a value binding.
            _ => return false,
        }
    }
    saw_named
}

fn emit(ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>, spec: &str, offset: usize) {
    // The package's own declared entry file (package.json `main`/`exports` `.`)
    // exists to dispatch to the prebuilt `./dist/` artifact — telling it to
    // import from the entry point is circular. Non-entry files still fire.
    if ctx.project.is_package_entry_file(ctx.path) {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, offset);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Import from '{spec}' targets `dist/`. Import from package entry point, not dist/."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                let spec = import.source.value.as_str();
                if targets_dist(spec) && !is_type_only(import) {
                    emit(ctx, diagnostics, spec, import.span.start as usize);
                }
            }
            AstKind::CallExpression(call) => {
                // require('pkg/dist/foo')
                let is_require = matches!(
                    &call.callee,
                    oxc_ast::ast::Expression::Identifier(id) if id.name.as_str() == "require"
                );
                if !is_require {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let spec = match first_arg {
                    oxc_ast::ast::Argument::StringLiteral(s) => s.value.as_str(),
                    _ => return,
                };
                if targets_dist(spec) {
                    emit(ctx, diagnostics, spec, call.span.start as usize);
                }
            }
            _ => {}
        }
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Handle dynamic import() which is ImportExpression, not CallExpression
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::ImportExpression(import) = node.kind()
                && let oxc_ast::ast::Expression::StringLiteral(s) = &import.source {
                    let spec = s.value.as_str();
                    if targets_dist(spec) {
                        emit(ctx, &mut diagnostics, spec, import.span.start as usize);
                    }
                }
        }
        diagnostics
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            "/tmp/foo.ts",
            &crate::project::ProjectCtx::for_test_with_framework(""),
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_value_import_from_dist() {
        let src = r#"import { foo } from "pkg/dist/foo";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_dist_import_in_test_file() {
        // Issue #1538 — a `node:test` integration test that verifies the
        // compiled package output must import from `dist/` because the runner
        // does not support TypeScript. The central `skip_in_test_dir` gate
        // exempts the whole file.
        let src = r#"import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { isParentDirectory } from '../dist/path.js';"#;
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "packages/internal-helpers/test/path.test.ts",
        );
        assert!(diags.is_empty(), "dist import in a test file must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_dist_import_in_production_source() {
        // Negative space: a shippable source file importing from dist/ is the
        // anti-pattern the rule targets and must still fire.
        let src = r#"import { foo } from "./dist/foo";"#;
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/foo.ts");
        assert_eq!(diags.len(), 1, "dist import in production source must fire, got: {diags:?}");
    }

    #[test]
    fn allows_inline_type_import_from_dist() {
        // Issue #2074 exact example — AppType in the Next.js Pages Router is
        // only available via this internal dist/ path, as a type-only import.
        let src = r#"import { type AppType } from "next/dist/shared/lib/utils";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_top_level_type_import_from_dist() {
        let src = r#"import type { Foo } from "pkg/dist/foo";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mixed_value_and_type_import_from_dist() {
        // Not every specifier is a type — a value binding still pulls runtime.
        let src = r#"import { foo, type Bar } from "pkg/dist/foo";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_default_import_from_dist() {
        // A default binding is always a value, even alongside inline types.
        let src = r#"import Foo, { type Bar } from "pkg/dist/foo";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_package_own_entry_dispatching_to_dist() {
        // Issue #1996 — a package root entry file (declared as `main`) whose job
        // is to conditionally require the prebuilt ./dist/ artifact. A sibling
        // non-entry file importing from dist/ must still fire.
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"vue","main":"index.js"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();

        let entry = dir.path().join("index.js");
        let entry_src = r#"if (process.env.NODE_ENV === 'production') {
  module.exports = require('./dist/vue.cjs.prod.js')
} else {
  module.exports = require('./dist/vue.cjs.js')
}"#;
        let entry_diags =
            crate::rules::test_helpers::run_rule_with_ctx(&Check, entry_src, &entry, &project, file);
        assert!(
            entry_diags.is_empty(),
            "the declared entry file should be exempt, got: {entry_diags:?}"
        );

        let other = dir.path().join("src").join("foo.ts");
        let other_src = r#"import { x } from "./dist/x";"#;
        let other_diags =
            crate::rules::test_helpers::run_rule_with_ctx(&Check, other_src, &other, &project, file);
        assert_eq!(
            other_diags.len(),
            1,
            "a non-entry file importing from dist/ must still fire"
        );
    }

    #[test]
    fn ignores_package_own_bin_entry_dispatching_to_dist() {
        // Issue #4514 — a `bin/*.mjs` CLI shim declared in package.json `"bin"`
        // (object map form) whose sole job is to load the compiled dist/ output.
        // npm installs it as an executable, so it is itself an entry point.
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@antfu/ni","bin":{"na":"bin/na.mjs","ni":"bin/ni.mjs"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();

        let bin = dir.path().join("bin").join("na.mjs");
        let bin_src = "import '../dist/na.mjs'\n";
        let bin_diags =
            crate::rules::test_helpers::run_rule_with_ctx(&Check, bin_src, &bin, &project, file);
        assert!(
            bin_diags.is_empty(),
            "a declared bin entry shim should be exempt, got: {bin_diags:?}"
        );

        // Load-bearing negative: a non-bin, non-main source file importing from
        // dist/ must STILL fire — the exemption is entry-point-scoped.
        let other = dir.path().join("src").join("foo.ts");
        let other_src = r#"import { x } from "../dist/bar";"#;
        let other_diags =
            crate::rules::test_helpers::run_rule_with_ctx(&Check, other_src, &other, &project, file);
        assert_eq!(
            other_diags.len(),
            1,
            "a non-entry file importing from dist/ must still fire"
        );
    }

    #[test]
    fn ignores_package_own_string_bin_entry_dispatching_to_dist() {
        // Issue #4514 — the string form `"bin": "bin/cli.mjs"` names a single CLI
        // entry point; a dist/ import in that shim is an entry point, not an FP.
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"mycli","bin":"bin/cli.mjs"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();

        let bin = dir.path().join("bin").join("cli.mjs");
        let bin_src = "import '../dist/cli.mjs'\n";
        let bin_diags =
            crate::rules::test_helpers::run_rule_with_ctx(&Check, bin_src, &bin, &project, file);
        assert!(
            bin_diags.is_empty(),
            "a declared string-form bin entry shim should be exempt, got: {bin_diags:?}"
        );
    }
}
