//! no-implicit-deps oxc backend — flag bare `import` specifiers that are not
//! declared in the nearest ancestor `package.json` and are not Node.js
//! builtins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{
    is_bare_specifier, is_node_builtin, is_path_alias_prefix, is_subpath_import,
    is_sveltekit_adapter_virtual_module, is_sveltekit_app_alias, is_virtual_module,
    jest_module_roots, matches_alias, module_federation, root_package_name, types_package_name,
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

        // Declaration-level type-only imports (`import type { X } from "pkg"`)
        // are erased at compile time and perform no runtime module resolution,
        // so they can never be a missing *runtime* dependency — the rule's
        // entire concern. Their types are frequently provided transitively
        // through a declared package's bundled declarations (e.g. `eslint`
        // ships `estree` types), which `package.json` never lists. Exempt them.
        // A mixed import (`import { type X, y }`) keeps a value binding (`y`)
        // and stays `import_kind == Value`, so it remains checked.
        if import.import_kind.is_type() {
            return;
        }

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
        // `~/…` and `@/…` are path aliases (Vite/webpack `resolve.alias`,
        // tsconfig `paths`, or framework defaults), never npm packages: a name
        // cannot start with `~`, and `@/` is a scoped name with an empty scope.
        // Exempt them structurally so an alias used without a parsed tsconfig
        // `paths` entry is not reported as a missing dependency.
        if is_path_alias_prefix(spec) {
            return;
        }
        if is_virtual_module(spec) {
            return;
        }
        // SvelteKit virtual specifiers that the framework's own Vite/Rollup
        // plugins resolve at build time, never installed from npm:
        //   - adapter virtual modules: bare uppercase names (`HANDLER`, `ENV`,
        //     `SERVER`, `SHIMS`, `MANIFEST`), and
        //   - reserved app aliases: `$lib`/`$lib/…` (→ `src/lib`), `$app/…`,
        //     `$env/…`, `$service-worker`.
        // Gate on SvelteKit being detected for this file's package so the same
        // specifiers still fire as implicit dependencies in a non-SvelteKit
        // project.
        if is_sveltekit_adapter_virtual_module(spec) || is_sveltekit_app_alias(spec) {
            let is_sveltekit = ctx
                .project
                .frameworks_for_path(ctx.path)
                .iter()
                .any(|f| f.name == "svelte");
            if is_sveltekit {
                return;
            }
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
        // Jest `modulePaths`/`moduleDirectories` add in-repo resolution roots, so
        // a bare specifier whose first segments resolve to local source under one
        // of those roots (e.g. `app/core/foo` → `<rootDir>/public/app/core/foo`)
        // is a project file, not an npm package.
        if jest_module_roots::resolves_via_jest_module_roots(
            ctx.path,
            spec,
            ctx.project.project_root.as_deref(),
        ) {
            return;
        }
        let root = root_package_name(spec);
        // DefinitelyTyped convention: a project that lists only `@types/X` (and
        // never `X`) in its (dev|peer)dependencies can still `import … from "X"`
        // — TypeScript resolves the bare specifier to the `@types/X`
        // declarations (e.g. `@types/json-schema` satisfies an import from
        // `json-schema`). The aliased name is consulted alongside `root` at
        // every dependency-resolution layer below.
        let types_root = types_package_name(root);
        if pkg.has_dep_or_engine(root) || pkg.has_dep_or_engine(&types_root) {
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
                    if ancestor_pkg.has_dep_or_engine(root)
                        || ancestor_pkg.has_dep_or_engine(&types_root)
                        || ancestor_pkg.is_self_name(root)
                    {
                        return;
                    }
                }
                pkg_dir = ancestor_dir;
            }
        }

        // npm-workspaces siblings: in a workspaces monorepo npm hoists every
        // member's deps to the shared root `node_modules`, so a member may import
        // a specifier declared only in a sibling member (declared in neither the
        // importing package nor any ancestor). Resolve the members from the root
        // `workspaces` globs and consult the union of their declared deps.
        if ctx.project.dep_declared_in_workspace_siblings(ctx.path, root)
            || ctx
                .project
                .dep_declared_in_workspace_siblings(ctx.path, &types_root)
        {
            return;
        }

        // Sibling manifests: a file in a directory with no `package.json` of its
        // own (a monorepo `integration/` test tree) imports packages declared in
        // sibling `packages/*/package.json` manifests, hoisted at runtime. When
        // the repo root declares no `workspaces` field the workspace walk above
        // never sees those siblings, so consult the union of every dep declared
        // anywhere under the project root before flagging.
        if ctx.project.dep_declared_in_tree(ctx.path, root)
            || ctx.project.dep_declared_in_tree(ctx.path, &types_root)
        {
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

    // Regression #1823 (mswjs/msw): the importer's nearest manifest is a
    // marker-only `/test/package.json` ({"type":"module"}). The dep lives in the
    // root `devDependencies`. The marker is not a package boundary, so dep
    // lookup resolves the substantive root and the import is satisfied.
    #[test]
    fn allows_root_dep_when_nearest_is_marker_only_issue_1823() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"msw","main":"./lib/index.js","devDependencies":{"vitest":"^1.0.0"}}"#,
        )
        .unwrap();
        let sub = dir.path().join("test").join("memory");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.path().join("test").join("package.json"), r#"{"type":"module"}"#).unwrap();
        let file = sub.join("vitest.config.ts");
        let source = "import { defineConfig } from 'vitest/config';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "root dep reached past a marker-only manifest must not be flagged, got {diags:?}"
        );
    }

    // Negative space for #1823: a marker-only nearest manifest must not mask a
    // genuinely-undeclared dependency — the substantive root is consulted and
    // the missing package still fires.
    #[test]
    fn flags_undeclared_dep_when_nearest_is_marker_only_issue_1823() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"msw","main":"./lib/index.js","devDependencies":{"vitest":"^1.0.0"}}"#,
        )
        .unwrap();
        let sub = dir.path().join("test").join("memory");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.path().join("test").join("package.json"), r#"{"type":"module"}"#).unwrap();
        let file = sub.join("t.ts");
        let source = "import x from 'genuinely-undeclared';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "undeclared dep must still fire past a marker manifest, got {diags:?}"
        );
    }

    // Regression #1365: `~/` path alias (Vite/webpack `resolve.alias`) used
    // without a parsed tsconfig `paths` entry. `~` can never start an npm
    // package name, so the import is a local alias, not a missing dependency.
    #[test]
    fn allows_tilde_slash_path_alias_issue_1365() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"app","dependencies":{}}"#).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.tsx");
        let source = "import { Carbon } from '~/components/Carbon';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(diags.is_empty(), "`~/` path alias must not be flagged, got {diags:?}");
    }

    // Regression #1376: `@/` path alias (WXT/Vite framework default) used
    // without a parsed tsconfig `paths` entry. `@/` is a scoped name with an
    // empty scope, so it can never name a package.
    #[test]
    fn allows_at_slash_path_alias_issue_1376() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"app","dependencies":{}}"#).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.tsx");
        let source = "import { y } from '@/utils/bar';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(diags.is_empty(), "`@/` path alias must not be flagged, got {diags:?}");
    }

    // Negative space for #1376: the `@/` exemption must not over-reach to a
    // genuine scoped package (non-empty scope), which stays flagged if unlisted.
    #[test]
    fn still_flags_real_scoped_package_not_at_slash_alias() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"app","dependencies":{}}"#).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.ts");
        let source = "import { x } from '@scope/pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(diags.len(), 1, "real scoped package must still be flagged, got {diags:?}");
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

    // Regression #1378: `unplugin-icons` exposes icon components under the
    // `~icons/<collection>/<name>` virtual namespace (resolved by the plugin at
    // build time, never an npm package), so these imports must not be flagged.
    #[test]
    fn allows_unplugin_icons_virtual_import_issue_1378() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("lens-actions.ts");
        let source =
            "import IconAdd from '~icons/carbon/add';\nimport Logos from '~icons/logos/vue';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "~icons/ virtual imports must not be flagged, got {diags:?}"
        );
    }

    // Regression #1379: `@qwik-city-plan` is the build-time virtual module
    // injected by the `@builder.io/qwik-city` Vite plugin (exposing the routing
    // plan); it is never published to npm, so it must not be flagged.
    #[test]
    fn allows_qwik_city_plan_virtual_import_issue_1379() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("entry.preview.tsx");
        let source = "import qwikCityPlan from '@qwik-city-plan';\nimport { routes } from '@qwik-city-plan';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "@qwik-city-plan virtual import must not be flagged, got {diags:?}"
        );
    }

    // Negative-space guard: a genuinely-unlisted bare import that merely looks
    // similar must still fire — the `~icons/` exemption is prefix-scoped.
    #[test]
    fn flags_unlisted_bare_import_alongside_icons_issue_1378() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.ts");
        let source = "import x from 'some-unlisted-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "genuinely-unlisted bare import must still fire, got {diags:?}"
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

    // Regression #2068: a declaration-level type-only import
    // (`import type { Node } from "estree"`) is erased at compile time and
    // performs no runtime module resolution, so it can never be a missing
    // runtime dependency — even when `estree` is absent from package.json (its
    // types are provided transitively via a declared package's bundled
    // declarations). It must not be flagged.
    #[test]
    fn allows_type_only_import_issue_2068() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"babel-eslint-plugin-development","peerDependencies":{"eslint":"^9"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src").join("utils");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("get-reference-origin.ts");
        let source = "import type { Node, Expression, Identifier } from 'estree';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "type-only import of an unlisted package must not be flagged, got {diags:?}"
        );
    }

    // A genuine RUNTIME (value) import of the same unlisted package must still
    // fire — the type-only exemption is scoped to declaration-level
    // `import type` only, not to value imports.
    #[test]
    fn flags_value_import_of_unlisted_dep_issue_2068() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"babel-eslint-plugin-development","peerDependencies":{"eslint":"^9"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src").join("utils");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("get-reference-origin.ts");
        let source = "import { foo } from 'estree';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "value import of an unlisted package must still fire, got {diags:?}"
        );
    }

    // Regression #1671: in an npm-workspaces monorepo (root `package.json` has a
    // `workspaces` glob) a package may import a specifier declared only in a
    // SIBLING workspace package, not the root and not the importing package. npm
    // hoists every member's deps to the shared root `node_modules`, so the import
    // resolves at runtime. `@jest/globals` is declared only in
    // `packages/integration-testsuite/package.json` yet imported from
    // `packages/server/`.
    #[test]
    fn allows_sibling_workspace_package_dep_issue_1671() {
        let dir = TempDir::new().unwrap();
        // Root manifest: declares `workspaces` but NOT @jest/globals.
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"apollo-server-monorepo","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        // Sibling workspace package that DOES declare @jest/globals.
        let testsuite = dir.path().join("packages").join("integration-testsuite");
        fs::create_dir_all(&testsuite).unwrap();
        fs::write(
            testsuite.join("package.json"),
            r#"{"name":"@apollo/server-integration-testsuite","devDependencies":{"@jest/globals":"^29.0.0"}}"#,
        )
        .unwrap();
        // Importing workspace package: its own manifest does NOT declare it.
        let server = dir.path().join("packages").join("server");
        let tests = server.join("src").join("__tests__");
        fs::create_dir_all(&tests).unwrap();
        fs::write(server.join("package.json"), r#"{"name":"@apollo/server"}"#).unwrap();
        let file = tests.join("errors.test.ts");
        let source = "import { describe, it } from '@jest/globals';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "dep declared in a sibling workspace package must not be flagged, got {diags:?}"
        );
    }

    // Negative-space guard for #1671: a specifier declared in NO workspace
    // package (and nowhere in the tree) must still fire from a workspace package.
    #[test]
    fn flags_dep_in_no_workspace_package_issue_1671() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"apollo-server-monorepo","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let testsuite = dir.path().join("packages").join("integration-testsuite");
        fs::create_dir_all(&testsuite).unwrap();
        fs::write(
            testsuite.join("package.json"),
            r#"{"name":"@apollo/server-integration-testsuite","devDependencies":{"@jest/globals":"^29.0.0"}}"#,
        )
        .unwrap();
        let server = dir.path().join("packages").join("server");
        let tests = server.join("src").join("__tests__");
        fs::create_dir_all(&tests).unwrap();
        fs::write(server.join("package.json"), r#"{"name":"@apollo/server"}"#).unwrap();
        let file = tests.join("errors.test.ts");
        let source = "import x from 'totally-undeclared-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "a specifier declared in no workspace package must still fire, got {diags:?}"
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

    fn run_oxc_with_project(
        file: &std::path::Path,
        source: &str,
        project: &crate::project::ProjectCtx,
    ) -> Vec<Diagnostic> {
        crate::oxc_helpers::reset_file_caches();
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(file, source, project);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let ty = node.kind().ty();
            if Check.interested_kinds().contains(&ty) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    // Regression #1797: pnpm monorepos (wagmi) declare members in
    // `pnpm-workspace.yaml`, leaving the root `package.json#workspaces` empty.
    // A test file importing a sibling workspace member (`@wagmi/test`) and a
    // root devDependency (`vitest`) must not be flagged, while a specifier
    // declared in no member and no root manifest still fires.
    #[test]
    fn allows_pnpm_workspace_member_and_root_dep_issue_1797() {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;

        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"workspace","private":true,"devDependencies":{"vitest":"^2.0.0"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n  - playgrounds/*\n",
        )
        .unwrap();

        let connectors = dir.path().join("packages").join("connectors");
        let test_src = connectors.join("src");
        let test_pkg = dir.path().join("packages").join("test");
        fs::create_dir_all(&test_src).unwrap();
        fs::create_dir_all(test_pkg.join("src")).unwrap();
        fs::write(
            connectors.join("package.json"),
            r#"{"name":"@wagmi/connectors"}"#,
        )
        .unwrap();
        fs::write(test_pkg.join("package.json"), r#"{"name":"@wagmi/test"}"#).unwrap();

        let importer = test_src.join("safe.test.ts");
        let source =
            "import { config } from '@wagmi/test';\nimport { expect, test } from 'vitest';\n";
        fs::write(&importer, source).unwrap();
        // A second member file anchors `project_root` at the repo root (their
        // common ancestor's nearest manifest), matching a full repo scan.
        let other = test_pkg.join("src").join("index.ts");
        fs::write(&other, "export const config = {};\n").unwrap();

        let importer = fs::canonicalize(&importer).unwrap();
        let other = fs::canonicalize(&other).unwrap();
        let sf_importer = SourceFile {
            path: importer.clone(),
            language: Language::TypeScript,
        };
        let sf_other = SourceFile {
            path: other,
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&sf_importer, &sf_other];
        let project = ProjectCtx::load(&refs, &Config::default());

        let diags = run_oxc_with_project(&importer, source, &project);
        assert!(
            diags.is_empty(),
            "pnpm-workspace member + root devDep must not be flagged, got {diags:?}"
        );

        let missing_source = "import x from 'totally-not-declared';\n";
        let missing_file = test_src.join("missing.test.ts");
        fs::write(&missing_file, missing_source).unwrap();
        let missing_file = fs::canonicalize(&missing_file).unwrap();
        let diags = run_oxc_with_project(&missing_file, missing_source, &project);
        assert_eq!(
            diags.len(),
            1,
            "a specifier declared nowhere must still fire, got {diags:?}"
        );
    }

    // Regression #1800 (wagmi): a monorepo whose root `package.json` declares no
    // framework, with `next` listed only in a nested sub-package
    // (`playgrounds/next/package.json`). An `import ... from 'next'` in that
    // playground must not be flagged — the nearest-`package.json` lookup resolves
    // the dependency to the sub-package's manifest. A sibling playground file
    // importing a specifier declared in no manifest must still fire.
    #[test]
    fn allows_framework_dep_in_nested_subpackage_issue_1800() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"workspace","private":true}"#,
        )
        .unwrap();
        let app = dir.path().join("playgrounds").join("next").join("src").join("app");
        fs::create_dir_all(&app).unwrap();
        fs::write(
            dir.path().join("playgrounds").join("next").join("package.json"),
            r#"{"name":"@wagmi/next-playground","dependencies":{"next":"^15.0.0"}}"#,
        )
        .unwrap();
        let file = app.join("page.tsx");
        let source = "import { headers } from 'next/headers';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "framework dep declared in a nested sub-package must not be flagged, got {diags:?}"
        );

        let missing = app.join("missing.tsx");
        let missing_source = "import x from 'totally-undeclared-pkg';";
        fs::write(&missing, missing_source).unwrap();
        let diags = run_oxc_in_project(&missing, missing_source);
        assert_eq!(
            diags.len(),
            1,
            "a specifier declared in no manifest must still fire, got {diags:?}"
        );
    }

    // Regression #1529: Jest `modulePaths` adds in-repo resolution roots, so a
    // bare specifier resolving under one of them (`app/core/...` →
    // `<rootDir>/public/app/core/...`, `test/...` → `<rootDir>/public/test/...`)
    // is a project file, not an npm package. `package.json` correctly lists
    // neither `app` nor `test` as a dependency.
    #[test]
    fn allows_jest_module_paths_root_import_issue_1529() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"grafana"}"#).unwrap();
        fs::write(
            dir.path().join("jest.config.js"),
            "module.exports = { modulePaths: ['public', 'node_modules'] };\n",
        )
        .unwrap();
        // On-disk source that the configured root resolves to.
        let svc = dir.path().join("public").join("app").join("core").join("services");
        fs::create_dir_all(&svc).unwrap();
        fs::write(svc.join("context_srv.ts"), "export class ContextSrv {}\n").unwrap();
        let helpers = dir.path().join("public").join("test");
        fs::create_dir_all(&helpers).unwrap();
        fs::write(helpers.join("test-utils.ts"), "export const setupStore = () => {};\n").unwrap();

        let feature = dir.path().join("public").join("app").join("features");
        fs::create_dir_all(&feature).unwrap();
        let file = feature.join("dashboard.test.ts");
        let source = "import { ContextSrv } from 'app/core/services/context_srv';\nimport { setupStore } from 'test/test-utils';\n";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "imports resolving via Jest modulePaths must not be flagged, got {diags:?}"
        );
    }

    // `moduleDirectories` written with a `<rootDir>` token resolves the same way.
    #[test]
    fn allows_jest_module_directories_root_import_issue_1529() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"grafana"}"#).unwrap();
        fs::write(
            dir.path().join("jest.config.ts"),
            "export default { moduleDirectories: ['<rootDir>/public/app', 'node_modules'] };\n",
        )
        .unwrap();
        let svc = dir.path().join("public").join("app").join("core");
        fs::create_dir_all(&svc).unwrap();
        fs::write(svc.join("utils.ts"), "export const x = 1;\n").unwrap();

        let file = dir.path().join("public").join("app").join("page.test.ts");
        let source = "import { x } from 'core/utils';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "import resolving via Jest moduleDirectories must not be flagged, got {diags:?}"
        );
    }

    // The `"jest"` key inside `package.json` declares the roots just as a
    // standalone `jest.config.*` file does.
    #[test]
    fn allows_jest_config_in_package_json_issue_1529() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","jest":{"modulePaths":["src"]}}"#,
        )
        .unwrap();
        let lib = dir.path().join("src").join("lib");
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("helper.ts"), "export const h = 1;\n").unwrap();

        let file = dir.path().join("src").join("a.test.ts");
        let source = "import { h } from 'lib/helper';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "import resolving via package.json jest.modulePaths must not be flagged, got {diags:?}"
        );
    }

    // Regression #1588: SvelteKit adapters inject bare uppercase virtual module
    // specifiers (`HANDLER`, `ENV`, `SERVER`, `SHIMS`, `MANIFEST`) that their
    // Rollup plugin resolves at bundle time. They are never npm packages and are
    // intentionally absent from `package.json`, so they must not be flagged when
    // SvelteKit is detected for the importing file's package.
    #[test]
    fn allows_sveltekit_adapter_virtual_modules_issue_1588() {
        for spec in &["HANDLER", "ENV", "SERVER", "SHIMS", "MANIFEST"] {
            let dir = TempDir::new().unwrap();
            fs::write(
                dir.path().join("package.json"),
                r#"{"name":"@sveltejs/adapter-node","devDependencies":{"@sveltejs/kit":"^2.4.0"}}"#,
            )
            .unwrap();
            let src = dir.path().join("src");
            fs::create_dir_all(&src).unwrap();
            let file = src.join("index.js");
            let source = format!("import {{ x }} from '{spec}';");
            fs::write(&file, &source).unwrap();
            let diags = run_oxc_in_project(&file, &source);
            assert!(
                diags.is_empty(),
                "SvelteKit adapter virtual module `{spec}` must not be flagged, got {diags:?}"
            );
        }
    }

    // Negative-space guard for #1588: the same uppercase specifier in a
    // NON-SvelteKit project is a genuine implicit dependency and must still fire
    // — the exemption is gated on SvelteKit being detected.
    #[test]
    fn flags_uppercase_virtual_module_without_sveltekit_issue_1588() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"plain-app","dependencies":{}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("index.ts");
        let source = "import { Server } from 'SERVER';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "uppercase bare import in a non-SvelteKit project must still fire, got {diags:?}"
        );
    }

    // Negative-space guard for #1588: a genuinely undeclared package must still
    // fire in a SvelteKit project — the exemption is scoped to the known adapter
    // virtual-module names only, not arbitrary bare imports.
    #[test]
    fn flags_unlisted_dep_alongside_sveltekit_issue_1588() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@sveltejs/adapter-node","devDependencies":{"@sveltejs/kit":"^2.4.0"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("index.js");
        let source = "import x from 'totally-undeclared-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "an undeclared package in a SvelteKit project must still fire, got {diags:?}"
        );
    }

    // Regression #1366: SvelteKit reserves `$`-prefixed application aliases
    // (`$lib`/`$lib/…` → `src/lib`, `$app/…`, `$env/…`, `$service-worker`) that
    // its Vite plugin resolves to project source or generated code at build
    // time. They are never npm packages and are intentionally absent from
    // `package.json`, so they must not be flagged when SvelteKit is detected for
    // the importing file's package.
    #[test]
    fn allows_sveltekit_app_aliases_issue_1366() {
        for spec in &[
            "$lib",
            "$lib/utils/i18n",
            "$lib/managers/event-manager.svelte",
            "$app/navigation",
            "$app/stores",
            "$app/environment",
            "$env/static/private",
            "$env/dynamic/public",
            "$service-worker",
        ] {
            let dir = TempDir::new().unwrap();
            fs::write(
                dir.path().join("package.json"),
                r#"{"name":"web","devDependencies":{"@sveltejs/kit":"^2.4.0"}}"#,
            )
            .unwrap();
            let src = dir.path().join("src").join("lib");
            fs::create_dir_all(&src).unwrap();
            let file = src.join("a.ts");
            let source = format!("import {{ x }} from '{spec}';");
            fs::write(&file, &source).unwrap();
            let diags = run_oxc_in_project(&file, &source);
            assert!(
                diags.is_empty(),
                "SvelteKit app alias `{spec}` must not be flagged, got {diags:?}"
            );
        }
    }

    // Negative-space guard for #1366: in a NON-SvelteKit project the `$`-aliases
    // are genuine implicit dependencies and must still fire — the exemption is
    // gated on SvelteKit being detected.
    #[test]
    fn flags_app_alias_without_sveltekit_issue_1366() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"plain-app","dependencies":{}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("index.ts");
        let source = "import { t } from '$lib/utils/i18n';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "a `$`-alias in a non-SvelteKit project must still fire, got {diags:?}"
        );
    }

    // Negative-space guard for #1366: a genuinely undeclared package must still
    // fire in a SvelteKit project — the exemption is scoped to the reserved
    // `$`-aliases only, not to arbitrary `$`-prefixed or bare specifiers.
    #[test]
    fn flags_unlisted_dep_alongside_sveltekit_aliases_issue_1366() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"web","devDependencies":{"@sveltejs/kit":"^2.4.0"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("index.ts");
        // `$custom` is not a reserved SvelteKit alias; `lodash` is undeclared.
        let source = "import a from '$custom/thing';\nimport b from 'lodash';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            2,
            "non-reserved `$`-specifier and undeclared package must still fire, got {diags:?}"
        );
    }

    // Regression #1360: Node.js builtins missing from the recognized list
    // (`async_hooks` and friends) are provided by the runtime and never appear in
    // `package.json`. Both the bare form (`async_hooks`) and the `node:` scheme
    // (`node:async_hooks`) must be exempt.
    #[test]
    fn allows_missing_node_builtins_issue_1360() {
        for spec in &[
            "async_hooks",
            "node:async_hooks",
            "diagnostics_channel",
            "inspector",
            "trace_events",
            "wasi",
        ] {
            let dir = TempDir::new().unwrap();
            fs::write(
                dir.path().join("package.json"),
                r#"{"name":"@nestjs/core","dependencies":{}}"#,
            )
            .unwrap();
            let src = dir.path().join("packages").join("core").join("interceptors");
            fs::create_dir_all(&src).unwrap();
            let file = src.join("interceptors-consumer.ts");
            let source = format!("import {{ AsyncResource }} from '{spec}';");
            fs::write(&file, &source).unwrap();
            let diags = run_oxc_in_project(&file, &source);
            assert!(
                diags.is_empty(),
                "Node.js builtin `{spec}` must not be flagged, got {diags:?}"
            );
        }
    }

    // Negative-space guard for #1360: a genuinely unlisted bare import that is not
    // a Node.js builtin must still fire — the fix only extends the builtin list.
    #[test]
    fn flags_unlisted_non_builtin_issue_1360() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@nestjs/core","dependencies":{}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.ts");
        let source = "import x from 'definitely-not-a-builtin-pkg';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "a genuinely unlisted non-builtin import must still fire, got {diags:?}"
        );
    }

    // Regression #1374: a bare specifier whose type declarations are provided by
    // a `@types/X` package listed in devDependencies (the DefinitelyTyped
    // convention) is satisfied — TypeScript resolves `json-schema` to the
    // `@types/json-schema` declarations. The import must not be flagged.
    #[test]
    fn allows_import_satisfied_by_types_dep_issue_1374() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"payload","devDependencies":{"@types/json-schema":"7.0.15"}}"#,
        )
        .unwrap();
        let cfg = dir.path().join("src").join("config");
        fs::create_dir_all(&cfg).unwrap();
        let file = cfg.join("types.ts");
        // A value import reaches the dependency lookup (declaration-level
        // `import type` is exempted earlier); the `@types/` alias must satisfy it.
        let source = "import { JSONSchema4 } from 'json-schema';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "import satisfied by @types/X devDependency must not be flagged, got {diags:?}"
        );
    }

    // Scoped variant of #1374: TypeScript maps a scoped package `@foo/bar` to
    // `@types/foo__bar` (scope separator folded to a double underscore), so an
    // `@types/foo__bar` dependency satisfies an import from `@foo/bar`.
    #[test]
    fn allows_scoped_import_satisfied_by_types_dep_issue_1374() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","devDependencies":{"@types/foo__bar":"1.0.0"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.ts");
        let source = "import { X } from '@foo/bar';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "scoped import satisfied by @types/foo__bar must not be flagged, got {diags:?}"
        );
    }

    // Negative-space guard for #1374: an import with neither a matching runtime
    // dependency nor a matching `@types/X` dependency must still fire — the
    // exemption only suppresses imports a DefinitelyTyped package actually backs.
    #[test]
    fn flags_unlisted_dep_without_types_dep_issue_1374() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","devDependencies":{"@types/json-schema":"7.0.15"}}"#,
        )
        .unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("t.ts");
        let source = "import { x } from 'totally-missing';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "an import with no matching dep nor @types/X must still fire, got {diags:?}"
        );
    }

    // Negative-space guard for #1529: a genuinely undeclared package must still
    // fire even when Jest `modulePaths` is configured — its name does not resolve
    // to any source file under a configured root.
    #[test]
    fn flags_unlisted_dep_alongside_jest_module_paths_issue_1529() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"grafana"}"#).unwrap();
        fs::write(
            dir.path().join("jest.config.js"),
            "module.exports = { modulePaths: ['public', 'node_modules'] };\n",
        )
        .unwrap();
        let feature = dir.path().join("public").join("app").join("features");
        fs::create_dir_all(&feature).unwrap();
        let file = feature.join("dashboard.test.ts");
        // `lodash` does not exist under `public/` — must still fire.
        let source = "import _ from 'lodash';";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "an undeclared package not under any Jest root must still fire, got {diags:?}"
        );
    }
}
