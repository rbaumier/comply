//! no-implicit-deps oxc backend — flag bare `import` specifiers that are not
//! declared in the nearest ancestor `package.json` and are not Node.js
//! builtins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{
    is_bare_specifier, is_node_builtin, is_subpath_import, is_virtual_module, matches_alias,
    root_package_name,
};

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

        // Stay silent if there's no `package.json` anywhere above this file.
        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
            return;
        };
        let alias_prefixes = ctx
            .project
            .nearest_tsconfig(ctx.path)
            .map(|t| t.alias_prefixes())
            .unwrap_or_default();

        let spec = import.source.value.as_str();

        if !is_bare_specifier(spec) {
            return;
        }
        if is_node_builtin(spec) {
            return;
        }
        if is_subpath_import(spec) {
            return;
        }
        if is_virtual_module(spec) {
            return;
        }
        if matches_alias(spec, &alias_prefixes) {
            return;
        }
        // tsconfig `baseUrl` lets non-relative specifiers resolve to local
        // source (e.g. `src/types/Foo` → `<baseUrl>/src/types/Foo.ts`). Those
        // are project files, not npm packages.
        if ctx
            .project
            .resolves_via_tsconfig_base_url(ctx.path, spec)
        {
            return;
        }
        let root = root_package_name(spec);
        if pkg.has_dep_or_engine(root) {
            return;
        }
        // Workspace package names: skip if this is a cross-workspace import.
        if ctx
            .project
            .workspace_package_names()
            .iter()
            .any(|n| n == root)
        {
            return;
        }
        // Walk ancestor package.json files. If any declares `workspaces` (a
        // monorepo root) and lists the dep, this is a valid workspace import.
        if let Some(mut pkg_dir) = ctx.project.nearest_package_json_dir(ctx.path) {
            for _ in 0..8 {
                let Some(parent) = pkg_dir.parent() else { break };
                let Some(ancestor_dir) =
                    ctx.project.nearest_package_json_dir(&parent.join("_"))
                else {
                    break;
                };
                if ancestor_dir == pkg_dir {
                    break;
                }
                if let Some(ancestor_pkg) =
                    ctx.project.nearest_package_json(&ancestor_dir.join("_"))
                {
                    if !ancestor_pkg.workspaces.is_empty()
                        && ancestor_pkg.has_dep_or_engine(root)
                    {
                        return;
                    }
                }
                pkg_dir = ancestor_dir;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Bare import `{spec}` is not listed in package.json (checked root `{root}`)."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn run_oxc_in_project(path: &std::path::Path, source: &str) -> Vec<Diagnostic> {
        crate::oxc_helpers::reset_file_caches();
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test(path, source);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let ty = node.kind().ty();
            if Check.interested_kinds().contains(&ty) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    // Regression for issue #828: workspace package imports dep listed only in root.
    #[test]
    fn allows_root_dep_in_workspace_package() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"],"devDependencies":{"vite":"^5.0.0"}}"#,
        )
        .unwrap();
        let app = dir.path().join("packages").join("app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("package.json"), r#"{"name":"app"}"#).unwrap();
        let file = app.join("vitest.config.mts");
        let source = "import { defineConfig } from 'vite';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "workspace dep in root package.json must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn flags_unlisted_dep_in_workspace_package() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"],"devDependencies":{"vite":"^5.0.0"}}"#,
        )
        .unwrap();
        let app = dir.path().join("packages").join("app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("package.json"), r#"{"name":"app"}"#).unwrap();
        let file = app.join("t.ts");
        let source = "import x from 'not-listed-at-all';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(diags.len(), 1, "unlisted dep must be flagged, got {diags:?}");
    }

    #[test]
    fn flags_unlisted_dep_in_root_package() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","dependencies":{"react":"^19"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.ts");
        let source = "import x from 'not-listed';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(diags.len(), 1, "unlisted dep in root must be flagged, got {diags:?}");
    }

    #[test]
    fn flags_unlisted_dep_in_single_package() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","dependencies":{}}"#,
        )
        .unwrap();
        let file = dir.path().join("index.ts");
        let source = "import x from 'missing-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(diags.len(), 1, "unlisted dep in flat project must be flagged, got {diags:?}");
    }

    // Regression #1385: Node.js subpath imports (`#`-prefixed aliases from the
    // package.json `imports` field) are not npm packages and must not be flagged.
    #[test]
    fn allows_node_subpath_import_issue_1385() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r##"{"name":"@storybook/core","imports":{"#manager-stores":{"default":"./src/manager/manager-stores.ts"},"#utils":{"default":"./template/stories/utils.ts"}}}"##,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("TestingWidget.tsx");
        let source = "import { managerStore } from '#manager-stores';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "Node.js subpath import must not be flagged, got {diags:?}"
        );
    }

    // Regression #1422: tsconfig `baseUrl` lets non-relative specifiers resolve
    // to local source (`import ... from 'src/types/InfraConfig'` →
    // `<root>/src/types/InfraConfig.ts`). These are project files, not packages.
    #[test]
    fn allows_base_url_local_import_issue_1422() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"backend"}"#).unwrap();
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"baseUrl":"./"}}"#,
        )
        .unwrap();
        let types = dir.path().join("src").join("types");
        fs::create_dir_all(&types).unwrap();
        fs::write(types.join("InfraConfig.ts"), "export enum InfraConfigEnum {}\n").unwrap();
        let dto = dir.path().join("src").join("infra-config").join("dto");
        fs::create_dir_all(&dto).unwrap();
        let file = dto.join("onboarding.dto.ts");
        let source = "import { InfraConfigEnum } from 'src/types/InfraConfig';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "baseUrl-resolved local import must not be flagged, got {diags:?}"
        );
    }

    // A genuine undeclared package import must still fire even when `baseUrl` is
    // configured — the baseUrl candidate does not exist on disk.
    #[test]
    fn flags_unlisted_dep_with_base_url_issue_1422() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"backend"}"#).unwrap();
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"baseUrl":"./"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("app.ts");
        let source = "import { Client } from 'not-installed-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "undeclared package must still fire under baseUrl, got {diags:?}"
        );
    }

    // Regression #1060: a nested tsconfig.json with path aliases AND a trailing
    // comma (JSONC) must still suppress an aliased bare import — the parser must
    // tolerate the trailing comma so the alias survives.
    #[test]
    fn allows_import_via_tsconfig_paths_with_trailing_comma_issue_1060() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"nest"}"#).unwrap();
        let micro = dir.path().join("integration").join("microservices");
        fs::create_dir_all(micro.join("e2e")).unwrap();
        fs::write(
            micro.join("tsconfig.json"),
            "{\"compilerOptions\":{\"paths\":{\"@nestjs/common\":[\"../../packages/common\"],\"@nestjs/common/*\":[\"../../packages/common/*\"]}},\"exclude\":[\"node_modules\",]}",
        )
        .unwrap();
        let file = micro.join("e2e").join("broadcast.spec.ts");
        let source = "import { Module } from '@nestjs/common';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "tsconfig path alias (with trailing comma) must suppress, got {diags:?}"
        );
    }
}
