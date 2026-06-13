//! no-implicit-deps oxc backend — flag bare `import` specifiers that are not
//! declared in the nearest ancestor `package.json` and are not Node.js
//! builtins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{
    is_bare_specifier, is_node_builtin, is_subpath_import, is_virtual_module, matches_alias,
    module_federation, root_package_name,
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
        // Deno subtree: a `deno.json`/`deno.jsonc` at or below the nearest
        // `package.json` governs this file's imports via its own import map
        // (which this rule does not parse). Validating its imports against the
        // npm manifest would false-positive on every mapped specifier.
        if let Some(deno_dir) = ctx.project.nearest_deno_config_dir(ctx.path) {
            let deno_governs = ctx
                .project
                .nearest_package_json_dir(ctx.path)
                .is_none_or(|pkg_dir| deno_dir.starts_with(&pkg_dir));
            if deno_governs {
                return;
            }
        }
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
        // Node.js self-reference: a package may import from itself by its own
        // published `name` (`import x from "preact"` or a subpath
        // `"preact/hooks"`), resolved by the toolchain to its own source. A
        // package never lists itself as a dependency, so this is not implicit.
        if pkg.is_self_name(root) {
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
        // Walk ancestor package.json files. A dep declared in any ancestor
        // manifest up to the repo root satisfies the import: monorepos often
        // hoist shared (dev)dependencies to a root `package.json` that has no
        // `workspaces` field, yet the package is available at runtime.
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
                    if ancestor_pkg.has_dep_or_engine(root) || ancestor_pkg.is_self_name(root) {
                        return;
                    }
                }
                pkg_dir = ancestor_dir;
            }
        }

        // Sibling manifests: a file in a directory with no `package.json` of its
        // own (a monorepo `integration/` test tree) imports packages declared in
        // sibling `packages/*/package.json` manifests, hoisted at runtime. When
        // the repo root declares no `workspaces` field the workspace walk above
        // never sees those siblings, so consult the union of every dep declared
        // anywhere under the project root before flagging.
        if ctx.project.dep_declared_in_tree(ctx.path, root) {
            return;
        }

        // Module Federation: `remotes` declared in a bundler config (rsbuild /
        // rspack / webpack / vite) turn their keys into runtime-resolved module
        // namespaces (`import X from "remote/Exposed"`). The scan is bounded by
        // the project root so it never escapes the repo.
        if module_federation::remote_names(ctx.path, ctx.project.project_root.as_deref())
            .contains(root)
        {
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

    // Regression #2045: Docusaurus component swizzling imports from the
    // `@theme-original/*` virtual namespace (resolved by Docusaurus's webpack
    // config, not an npm package) must not be flagged.
    #[test]
    fn allows_theme_original_swizzle_import_issue_2045() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"www"}"#).unwrap();
        let theme = dir.path().join("src").join("theme").join("BlogPostItem");
        fs::create_dir_all(&theme).unwrap();
        let file = theme.join("index.tsx");
        let source = "import OriginalBlogPostItem from '@theme-original/BlogPostItem';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "@theme-original/ swizzle import must not be flagged, got {diags:?}"
        );
    }

    // Regression #2042: a dep declared in a root `package.json` that has NO
    // `workspaces` field must still satisfy an import from a sub-package whose
    // nearest `package.json` neither lists the dep nor declares `workspaces`
    // (monorepos hoisting shared devDependencies to a non-workspaces root).
    #[test]
    fn allows_root_dep_without_workspaces_field_issue_2042() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"nest","devDependencies":{"chai":"^4.0.0"}}"#,
        )
        .unwrap();
        let pkg = dir.path().join("packages").join("microservices");
        let test = pkg.join("test");
        fs::create_dir_all(&test).unwrap();
        fs::write(pkg.join("package.json"), r#"{"name":"@nestjs/microservices"}"#).unwrap();
        let file = test.join("listeners.spec.ts");
        let source = "import { expect } from 'chai';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "root dep without `workspaces` field must not be flagged, got {diags:?}"
        );
    }

    // A dep declared in NO ancestor manifest must still fire even when an
    // ancestor root exists without a `workspaces` field.
    #[test]
    fn flags_dep_missing_from_all_ancestors_issue_2042() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"nest","devDependencies":{"chai":"^4.0.0"}}"#,
        )
        .unwrap();
        let pkg = dir.path().join("packages").join("microservices");
        let test = pkg.join("test");
        fs::create_dir_all(&test).unwrap();
        fs::write(pkg.join("package.json"), r#"{"name":"@nestjs/microservices"}"#).unwrap();
        let file = test.join("listeners.spec.ts");
        let source = "import x from 'not-declared-anywhere';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "dep absent from all ancestor manifests must fire, got {diags:?}"
        );
    }

    // Regression #2025: a file in a monorepo integration-test directory that has
    // NO `package.json` of its own, importing a package declared only in a
    // sibling `packages/*/package.json`, must not be flagged. The root manifest
    // declares no `workspaces` field, so the sibling is found only via the
    // tree-wide dep scan.
    #[test]
    fn allows_sibling_package_dep_from_manifestless_dir_issue_2025() {
        let dir = TempDir::new().unwrap();
        // Root manifest: has some deps but NOT @nestjs/common, and no workspaces.
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"nest","devDependencies":{"@nestjs/apollo":"^12.0.0"}}"#,
        )
        .unwrap();
        // Sibling package that DOES declare @nestjs/common.
        let common = dir.path().join("packages").join("common");
        fs::create_dir_all(&common).unwrap();
        fs::write(
            common.join("package.json"),
            r#"{"name":"@nestjs/common","peerDependencies":{"@nestjs/common":"^11.0.0"}}"#,
        )
        .unwrap();
        // Integration test tree with no package.json anywhere between it and root.
        let app = dir
            .path()
            .join("integration")
            .join("inspector")
            .join("src");
        fs::create_dir_all(&app).unwrap();
        let file = app.join("app.module.ts");
        let source = "import { Module, Scope } from '@nestjs/common';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "sibling-package dep from a manifestless integration dir must not be flagged, got {diags:?}"
        );
    }

    // Regression #1946: a file inside a Deno subtree governed by its own
    // `deno.json` import map must not be validated against the root
    // `package.json`. Both `@std/assert` (a JSR import) and `axios` (a relative
    // file mapping) are declared in `deno.json`, not the npm manifest.
    #[test]
    fn allows_deno_subtree_imports_issue_1946() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","dependencies":{}}"#,
        )
        .unwrap();
        let deno = dir.path().join("tests").join("smoke").join("deno");
        let tests = deno.join("tests");
        fs::create_dir_all(&tests).unwrap();
        fs::write(
            deno.join("deno.json"),
            r#"{"imports":{"@std/assert":"jsr:@std/assert@1","axios":"../x.js"}}"#,
        )
        .unwrap();
        let file = tests.join("x.test.ts");
        let source =
            "import { assertEquals } from '@std/assert';\nimport axios from 'axios';\n";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "imports in a deno.json-governed subtree must not be flagged, got {diags:?}"
        );
    }

    // A `deno.jsonc` (JSONC variant) at the subtree root governs it just the
    // same as `deno.json`.
    #[test]
    fn allows_deno_jsonc_subtree_imports_issue_1946() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","dependencies":{}}"#,
        )
        .unwrap();
        let deno = dir.path().join("scripts").join("deno");
        fs::create_dir_all(&deno).unwrap();
        fs::write(
            deno.join("deno.jsonc"),
            "{\n  // import map\n  \"imports\": { \"@std/fs\": \"jsr:@std/fs@1\" }\n}",
        )
        .unwrap();
        let file = deno.join("build.ts");
        let source = "import { exists } from '@std/fs';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "imports in a deno.jsonc-governed subtree must not be flagged, got {diags:?}"
        );
    }

    // A normal file under the root `package.json` with no `deno.json` in its
    // ancestry must still be flagged for an undeclared import — the Deno carve-out
    // only applies inside a Deno subtree.
    #[test]
    fn flags_undeclared_dep_outside_deno_subtree_issue_1946() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","dependencies":{}}"#,
        )
        .unwrap();
        let deno = dir.path().join("tests").join("smoke").join("deno");
        fs::create_dir_all(&deno).unwrap();
        fs::write(
            deno.join("deno.json"),
            r#"{"imports":{"axios":"../x.js"}}"#,
        )
        .unwrap();
        // Sibling source file outside the deno subtree, under the root manifest.
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("app.ts");
        let source = "import axios from 'axios';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "undeclared dep outside the deno subtree must still fire, got {diags:?}"
        );
    }

    // A package declared in NO manifest anywhere in the tree must still fire from
    // a manifestless integration directory — the tree scan only suppresses deps
    // that genuinely exist somewhere.
    #[test]
    fn flags_undeclared_dep_from_manifestless_dir_issue_2025() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"nest","devDependencies":{"@nestjs/apollo":"^12.0.0"}}"#,
        )
        .unwrap();
        let common = dir.path().join("packages").join("common");
        fs::create_dir_all(&common).unwrap();
        fs::write(
            common.join("package.json"),
            r#"{"name":"@nestjs/common"}"#,
        )
        .unwrap();
        let app = dir
            .path()
            .join("integration")
            .join("inspector")
            .join("src");
        fs::create_dir_all(&app).unwrap();
        let file = app.join("app.module.ts");
        let source = "import x from 'totally-undeclared-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "package declared in no manifest must still fire, got {diags:?}"
        );
    }

    // Regression #2011: a Module Federation remote (`remote`) declared in the
    // bundler config (rsbuild) becomes a runtime-resolved module namespace, so
    // `import Remote from "remote/remote-app"` must not be flagged even though
    // `remote` is not in package.json.
    #[test]
    fn allows_module_federation_remote_import_issue_2011() {
        let dir = TempDir::new().unwrap();
        let app = dir.path().join("host");
        let src = app.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(app.join("package.json"), r#"{"name":"host"}"#).unwrap();
        fs::write(
            app.join("rsbuild.config.ts"),
            r#"import { pluginModuleFederation } from "@module-federation/rsbuild-plugin";
export default {
  plugins: [
    pluginModuleFederation({
      name: "host",
      remotes: {
        remote: "remote@http://localhost:3001/mf-manifest.json",
      },
      exposes: {},
    }),
  ],
};
"#,
        )
        .unwrap();
        let file = src.join("App.tsx");
        let source = "import Remote from 'remote/remote-app';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "Module Federation remote import must not be flagged, got {diags:?}"
        );
    }

    // A genuinely undeclared package must still fire even when a Module
    // Federation config declares unrelated remotes — the exemption is scoped to
    // the configured remote names only.
    #[test]
    fn flags_unlisted_dep_alongside_module_federation_remote_issue_2011() {
        let dir = TempDir::new().unwrap();
        let app = dir.path().join("host");
        let src = app.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(app.join("package.json"), r#"{"name":"host"}"#).unwrap();
        fs::write(
            app.join("rsbuild.config.ts"),
            r#"export default {
  plugins: [
    pluginModuleFederation({ name: "host", remotes: { remote: "remote@http://x/mf.json" } }),
  ],
};
"#,
        )
        .unwrap();
        let file = src.join("App.tsx");
        let source = "import x from 'lodash';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "undeclared real package must still fire alongside MF remotes, got {diags:?}"
        );
    }

    // Regression #1975: Vite virtual modules — both the `virtual:` prefix
    // convention and custom namespace separators (`vitest-custom-virtual:math`)
    // are plugin-provided, never npm packages (a `:` is invalid in an npm
    // name), so they must not be flagged.
    #[test]
    fn allows_virtual_module_specifiers_issue_1975() {
        for spec in &[
            "virtual:vitest-custom-virtual-file-1",
            "vitest-custom-virtual:math",
        ] {
            let dir = TempDir::new().unwrap();
            fs::write(dir.path().join("package.json"), r#"{"name":"app","dependencies":{}}"#)
                .unwrap();
            let file = dir.path().join("virtual-files.ts");
            let source = format!("import x from '{spec}';");
            fs::write(&file, &source).unwrap();
            let diags = run_oxc_in_project(&file, &source);
            assert!(
                diags.is_empty(),
                "virtual module `{spec}` must not be flagged, got {diags:?}"
            );
        }
    }

    // Regression #1933: Astro virtual modules (`astro:content`,
    // `astro:transitions/client`) carry the `astro:` scheme, recognized as a
    // plugin-provided virtual namespace by the generic colon-scheme rule
    // (#1975) — a `:` is invalid in an npm name. They must not be flagged even
    // though `astro` is not declared as a dependency.
    #[test]
    fn allows_astro_virtual_module_protocol_issue_1933() {
        for spec in &["astro:content", "astro:transitions/client"] {
            let dir = TempDir::new().unwrap();
            fs::write(dir.path().join("package.json"), r#"{"name":"app","dependencies":{}}"#)
                .unwrap();
            let file = dir.path().join("content.config.ts");
            let source = format!("import x from '{spec}';");
            fs::write(&file, &source).unwrap();
            let diags = run_oxc_in_project(&file, &source);
            assert!(
                diags.is_empty(),
                "astro virtual module `{spec}` must not be flagged, got {diags:?}"
            );
        }
    }

    // Regression #1921: a package importing from itself by its own published
    // `name` (`import { createElement } from 'preact'`) or a subpath of it
    // (`import { teardown } from 'preact/test-utils'`) is a Node.js
    // self-reference resolved to the package's own source. A package never lists
    // itself as a dependency, so these must not be flagged.
    #[test]
    fn allows_self_name_import_issue_1921() {
        for spec in &["preact", "preact/test-utils", "preact/hooks"] {
            let dir = TempDir::new().unwrap();
            fs::write(
                dir.path().join("package.json"),
                r#"{"name":"preact","version":"10.0.0"}"#,
            )
            .unwrap();
            let test = dir.path().join("test").join("_util");
            fs::create_dir_all(&test).unwrap();
            let file = test.join("helpers.jsx");
            let source = format!("import {{ x }} from '{spec}';");
            fs::write(&file, &source).unwrap();
            let diags = run_oxc_in_project(&file, &source);
            assert!(
                diags.is_empty(),
                "self-name import `{spec}` must not be flagged, got {diags:?}"
            );
        }
    }

    // A scoped package importing from itself by its scoped `name`
    // (`import x from '@scope/pkg/sub'`) is likewise a self-reference.
    #[test]
    fn allows_scoped_self_name_import_issue_1921() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@scope/pkg","version":"1.0.0"}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.ts");
        let source = "import x from '@scope/pkg/sub';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "scoped self-name import must not be flagged, got {diags:?}"
        );
    }

    // The root manifest's `name` satisfies a self-reference from a sub-directory
    // whose nearest manifest is the root itself — the ancestor walk consults the
    // root manifest's name, not only its deps.
    #[test]
    fn allows_self_name_from_nested_dir_issue_1921() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"preact","version":"10.0.0"}"#,
        )
        .unwrap();
        let test = dir.path().join("compat").join("test").join("browser");
        fs::create_dir_all(&test).unwrap();
        let file = test.join("render.test.jsx");
        let source = "import { render } from 'preact';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "self-name import from a nested dir must not be flagged, got {diags:?}"
        );
    }

    // A genuinely undeclared third-party package must still fire even when the
    // package has its own `name` — the self-reference exemption matches only the
    // package's own name, not arbitrary imports.
    #[test]
    fn flags_unlisted_dep_alongside_self_name_issue_1921() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"preact","version":"10.0.0"}"#,
        )
        .unwrap();
        let test = dir.path().join("test");
        fs::create_dir_all(&test).unwrap();
        let file = test.join("t.ts");
        let source = "import x from 'some-undeclared-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "undeclared third-party package must still fire, got {diags:?}"
        );
    }

    // A genuinely undeclared external package must still fire — the virtual
    // exemption is scoped to the `@theme-original/` namespace only.
    #[test]
    fn flags_unlisted_dep_alongside_theme_original_issue_2045() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"www"}"#).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("page.tsx");
        let source = "import x from '@theme-not-original/Foo';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "undeclared external package must still fire, got {diags:?}"
        );
    }
}
