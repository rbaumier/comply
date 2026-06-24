//! dead-export — flag exported symbols with no importer in the project.
//!
//! A symbol that's exported but never imported from another file is dead
//! weight: it inflates the public surface of a module, ties maintainers to
//! an API no one uses, and hides from refactors that would otherwise delete
//! it. The index's per-symbol usage map is the authoritative oracle.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "dead-export",
    description: "Symbol is exported but never imported elsewhere in the project.",
    remediation: "Remove the export (and the symbol if unused internally), or verify the export is still needed for an external consumer. Unused exports bloat the module's public surface.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "imports"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::{CheckCtx, TextCheck};
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Build a project on disk, then run dead-export against `target_rel`.
    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile { path: p, language: lang });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
            lang: Language::TypeScript,
        };
        let diags = super::text::Check.check(&ctx);
        (dir, diags)
    }

    // Regression for #1556 — a component consumed only from `.md` / `.mdx` doc
    // pages (Docusaurus/Nextra/Astro ESM imports) must not be flagged dead: the
    // import index now scans those files, so the export has a real importer.
    #[test]
    fn no_fp_for_export_imported_only_from_markdown_issue_1556() {
        let files: Vec<(&str, &str)> = vec![
            (
                "components/DetailedExplanation.jsx",
                "export const DetailedExplanation = ({ children, title = 'Detailed Explanation' }) => children;\n",
            ),
            (
                "docs/faq/CodeStructure.md",
                "# Code structure\n\nimport { DetailedExplanation } from '../../components/DetailedExplanation'\n\n<DetailedExplanation title=\"Example\" />\n",
            ),
            (
                "docs/usage/side-effects.mdx",
                "import { DetailedExplanation } from '../../components/DetailedExplanation'\n\n<DetailedExplanation />\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "components/DetailedExplanation.jsx");
        assert!(
            diags.iter().all(|d| !d.message.contains("DetailedExplanation")),
            "component imported from MDX/Markdown must not be flagged dead, got: {diags:?}"
        );
    }

    // Regression for #2302 — TSLint custom rule files follow the plugin
    // convention: export a class named `Rule` that extends `AbstractRule` (or
    // `Rules.AbstractRule`). TSLint discovers them by loading the file and
    // calling `new Rule()`, so no `.ts` file ever imports `Rule`. The class is
    // consumed by the TSLint runtime by convention, like the AWS Lambda handler
    // in #1771, so dead-export must not flag it.
    #[test]
    fn no_fp_for_tslint_rule_class_issue_2302() {
        let files: Vec<(&str, &str)> = vec![
            (
                "tools/tslint/validateImportForEsmCjsInteropRule.ts",
                "import { RuleFailure, Rules } from 'tslint';\n\
                 import * as ts from 'typescript';\n\
                 export class Rule extends Rules.AbstractRule {\n\
                   override apply(sourceFile: ts.SourceFile): RuleFailure[] {\n\
                     return [];\n\
                   }\n\
                 }\n",
            ),
            ("tools/build.ts", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "tools/tslint/validateImportForEsmCjsInteropRule.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("Rule")),
            "TSLint custom rule class must not be flagged dead, got: {diags:?}"
        );
    }

    // Variant of #2302 — the AbstractRule base is imported directly (not via the
    // `Rules` namespace) as `import { Rules, AbstractRule } from 'tslint'` and the
    // class extends the bare `AbstractRule`. Both heritage shapes must be exempt.
    #[test]
    fn no_fp_for_tslint_rule_class_bare_abstract_rule_issue_2302() {
        let files: Vec<(&str, &str)> = vec![
            (
                "tools/tslint/noImplicitOverrideAbstractRule.ts",
                "import { AbstractRule } from 'tslint/lib/rules';\n\
                 export class Rule extends AbstractRule {\n\
                   apply() {\n\
                     return [];\n\
                   }\n\
                 }\n",
            ),
            ("tools/build.ts", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "tools/tslint/noImplicitOverrideAbstractRule.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("Rule")),
            "TSLint rule extending a bare AbstractRule must not be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #2302 — an ordinary `export class Rule {}` that
    // does NOT extend `AbstractRule` and whose file does NOT import from `tslint`
    // is a plain dead export and must still be flagged.
    #[test]
    fn still_flags_plain_rule_class_without_tslint_issue_2302() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/rule.ts",
                "export class Rule {\n\
                   apply() {}\n\
                 }\n",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/rule.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("Rule")),
            "a plain Rule class not extending AbstractRule must still be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #2302 — the exemption is scoped to the convention
    // `Rule` class. A genuinely dead helper export in a tslint-importing rule
    // file (anything other than the `Rule` class) must still be flagged.
    #[test]
    fn still_flags_dead_helper_in_tslint_rule_file_issue_2302() {
        let files: Vec<(&str, &str)> = vec![
            (
                "tools/tslint/noFooRule.ts",
                "import { Rules } from 'tslint';\n\
                 export class Rule extends Rules.AbstractRule {\n\
                   apply() { return []; }\n\
                 }\n\
                 export const __DEAD_HELPER = 1;\n",
            ),
            ("tools/build.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tools/tslint/noFooRule.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("__DEAD_HELPER")),
            "a dead helper alongside the Rule class must still be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #2302 — a `Rule` class extending `AbstractRule`
    // but in a file that does NOT import from `tslint` is not a TSLint rule (the
    // base could be any local `AbstractRule`), so it stays subject to the rule.
    #[test]
    fn still_flags_rule_extending_abstract_rule_without_tslint_import_issue_2302() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/myRule.ts",
                "import { AbstractRule } from './base';\n\
                 export class Rule extends AbstractRule {\n\
                   apply() {}\n\
                 }\n",
            ),
            ("src/base.ts", "export class AbstractRule {}\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/myRule.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("Rule")),
            "Rule extending a non-tslint AbstractRule must still be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #1556 — an export imported from no file at all
    // (not `.ts`/`.tsx`/`.md`/`.mdx`) is genuinely dead and must still fire.
    #[test]
    fn still_flags_export_imported_nowhere_issue_1556() {
        let files: Vec<(&str, &str)> = vec![
            (
                "components/Orphan.jsx",
                "export const Orphan = () => null;\n",
            ),
            (
                "docs/guide.md",
                "# Guide\n\nThis page mentions the word import but does not import Orphan.\n",
            ),
            ("src/other.ts", "export const used = 1;\nimport './side';\n"),
            ("src/side.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "components/Orphan.jsx");
        assert!(
            diags.iter().any(|d| d.message.contains("Orphan")),
            "export imported nowhere must still be flagged, got: {diags:?}"
        );
    }

    // Regression for #3346 (huntabyte/shadcn-svelte) — files under a shadcn
    // component registry's distribution root are copied into a consumer's project
    // by the registry CLI and read as source text by the build step, never
    // imported as modules within the repo. A `registry.json` manifest declaring
    // them via `items[].files[].path` marks the distribution root, so their
    // exports are consumed downstream and must not be flagged dead.
    #[test]
    fn no_fp_for_shadcn_registry_component_file_issue_3346() {
        let files: Vec<(&str, &str)> = vec![
            (
                "docs/registry.json",
                r#"{
                    "$schema": "https://shadcn-svelte.com/schema/registry.json",
                    "name": "shadcn-svelte",
                    "homepage": "https://shadcn-svelte.com",
                    "items": [
                        {
                            "name": "sidebar",
                            "type": "registry:ui",
                            "files": [
                                { "path": "src/lib/registry/ui/sidebar/index.ts", "type": "registry:ui" }
                            ]
                        },
                        {
                            "name": "dialog",
                            "type": "registry:ui",
                            "files": [
                                { "path": "src/lib/registry/ui/dialog/index.ts", "type": "registry:ui" }
                            ]
                        }
                    ]
                }"#,
            ),
            (
                "docs/src/lib/registry/ui/sidebar/index.ts",
                "export function useSidebar() {}\n",
            ),
            // A second app file so the index is not in single-file mode.
            ("docs/src/routes/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "docs/src/lib/registry/ui/sidebar/index.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("useSidebar")),
            "a shadcn registry source file must not be flagged dead, got: {diags:?}"
        );
    }

    // Negative-space guard for #3346 — the exemption is scoped to the registry's
    // distribution root. A genuinely dead export OUTSIDE that root, in the same
    // project as a `registry.json`, must still be flagged.
    #[test]
    fn still_flags_dead_export_outside_registry_root_issue_3346() {
        let files: Vec<(&str, &str)> = vec![
            (
                "docs/registry.json",
                r#"{
                    "$schema": "https://shadcn-svelte.com/schema/registry.json",
                    "name": "shadcn-svelte",
                    "homepage": "https://shadcn-svelte.com",
                    "items": [
                        {
                            "name": "sidebar",
                            "type": "registry:ui",
                            "files": [
                                { "path": "src/lib/registry/ui/sidebar/index.ts", "type": "registry:ui" }
                            ]
                        }
                    ]
                }"#,
            ),
            (
                "docs/src/lib/registry/ui/sidebar/index.ts",
                "export function useSidebar() {}\n",
            ),
            (
                "docs/src/lib/utils/orphan.ts",
                "export const deadHelper = 1;\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "docs/src/lib/utils/orphan.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("deadHelper")),
            "a dead export outside the registry root must still be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #3346 — a `registry.json` that lacks the shadcn
    // `$schema` marker is an unrelated tool's config (npm/Terraform registry
    // metadata), not a shadcn component registry, so it must NOT exempt sibling
    // files from the rule.
    #[test]
    fn still_flags_when_registry_json_is_not_shadcn_issue_3346() {
        let files: Vec<(&str, &str)> = vec![
            (
                "registry.json",
                r#"{ "name": "some-tool", "modules": ["a", "b"] }"#,
            ),
            (
                "src/lib/registry/ui/sidebar/index.ts",
                "export function useSidebar() {}\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) =
            run_on_project(&files, "src/lib/registry/ui/sidebar/index.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("useSidebar")),
            "a non-shadcn registry.json must not exempt sibling files, got: {diags:?}"
        );
    }

    // Regression for #4777 (didi/LogicFlow) — documentation demo components are
    // loaded by the docs framework (Rspress/Dumi) via a `<code src="…">`
    // directive that points at a directory, resolving to its `index.tsx`. No TS
    // `import` names the file, but the markdown include is a real cross-file
    // consumption, so the demo's `default` export must not be flagged dead.
    #[test]
    fn no_fp_for_demo_referenced_via_code_src_in_markdown_issue_4777() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/tutorial/advanced/edge/animation/index.tsx",
                "export default function App() {\n  return null;\n}\n",
            ),
            (
                "docs/tutorial/edge.md",
                "# Edge animation\n\n<code id=\"edge-animation\" src=\"../../src/tutorial/advanced/edge/animation\"></code>\n",
            ),
        ];
        let (_dir, diags) =
            run_on_project(&files, "src/tutorial/advanced/edge/animation/index.tsx");
        assert!(
            diags.iter().all(|d| !d.message.contains("default")),
            "a demo referenced via <code src> in markdown must not be flagged dead, got: {diags:?}"
        );
    }

    // Regression for #4777 — VitePress `<<< @/path` snippet includes consume a
    // source file at build time with no TS `import`. A file referenced only via
    // such a snippet must not be flagged dead.
    #[test]
    fn no_fp_for_demo_referenced_via_vitepress_snippet_issue_4777() {
        let files: Vec<(&str, &str)> = vec![
            (
                "examples/snippet.ts",
                "export const helper = () => 1;\n",
            ),
            (
                "docs/guide.md",
                "# Guide\n\n<<< @/examples/snippet.ts\n",
            ),
            // Make `@/` resolve to the project root via tsconfig paths.
            (
                "tsconfig.json",
                r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./*"] } } }"#,
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "examples/snippet.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("helper")),
            "a file referenced via a VitePress <<< snippet must not be flagged dead, got: {diags:?}"
        );
    }

    // Negative-space guard for #4777 — a demo-style file referenced by no TS
    // import and no markdown include (the markdown merely mentions the word
    // `src` in prose) is genuinely dead and must still be flagged.
    #[test]
    fn still_flags_export_with_no_markdown_include_issue_4777() {
        let files: Vec<(&str, &str)> = vec![
            (
                "src/widgets/orphan/index.tsx",
                "export default function Orphan() {\n  return null;\n}\n",
            ),
            (
                "docs/notes.md",
                "# Notes\n\nThis page talks about the src directory but includes nothing.\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/widgets/orphan/index.tsx");
        assert!(
            diags.iter().any(|d| d.message.contains("default")),
            "an export referenced by no import and no include must still be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #4777 — a `<code src="…">` shown only as an
    // example inside a fenced code block (framework docs documenting the
    // directive itself) is an illustration, not a real include, so it must NOT
    // exempt the referenced file.
    #[test]
    fn still_flags_when_code_src_is_inside_a_code_fence_issue_4777() {
        // The relative path WOULD resolve to `src/widgets/panel/index.tsx` if the
        // include were honoured; it is fenced, so it must be ignored and the
        // export stays flagged.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/widgets/panel/index.tsx",
                "export default function Panel() {\n  return null;\n}\n",
            ),
            (
                "docs/howto.md",
                "# How to embed a demo\n\n```md\n<code src=\"../src/widgets/panel\"></code>\n```\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/widgets/panel/index.tsx");
        assert!(
            diags.iter().any(|d| d.message.contains("default")),
            "a <code src> inside a code fence is an example, not an include, so it must still be flagged, got: {diags:?}"
        );
    }

    // Regression for #5403 (partykit/partykit) — a PartyKit server class declared
    // as `main` in `partykit.json` is loaded by the PartyKit runtime, never
    // through a static import, so its `default` export has no in-repo importer yet
    // is a live entry point and must not be flagged dead.
    #[test]
    fn no_fp_for_partykit_main_entry_class_issue_5403() {
        let files: Vec<(&str, &str)> = vec![
            (
                "partykit.json",
                r#"{ "name": "partykit-site", "main": "party/server.ts", "parties": { "presence": "party/presence.ts" } }"#,
            ),
            (
                "party/server.ts",
                "import type * as Party from \"partykit/server\";\n\
                 export default class NoopServer implements Party.Server {}\n",
            ),
            // A second app file so the index is not in single-file mode.
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "party/server.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("default")),
            "a PartyKit `main` entry class must not be flagged dead, got: {diags:?}"
        );
    }

    // Regression for #5403 — the same for a `parties.<name>` entry: the server
    // class declared under `parties.presence` is equally framework-loaded.
    #[test]
    fn no_fp_for_partykit_parties_entry_class_issue_5403() {
        let files: Vec<(&str, &str)> = vec![
            (
                "partykit.json",
                r#"{ "name": "partykit-site", "main": "party/server.ts", "parties": { "presence": "party/presence.ts" } }"#,
            ),
            (
                "party/presence.ts",
                "import type * as Party from \"partykit/server\";\n\
                 export default class PresenceServer implements Party.Server {\n\
                   constructor(public room: Party.Room) {}\n\
                 }\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "party/presence.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("default")),
            "a PartyKit `parties.<name>` entry class must not be flagged dead, got: {diags:?}"
        );
    }

    // Regression for #5403 — convention fallback: a `party/`-directory server
    // class NOT listed in `partykit.json` (partial config) is still exempt
    // because its `default` export implements `Party.Server` and a `partykit.json`
    // marks the project as a PartyKit app.
    #[test]
    fn no_fp_for_partykit_convention_server_class_issue_5403() {
        let files: Vec<(&str, &str)> = vec![
            (
                "partykit.json",
                r#"{ "name": "partykit-site", "main": "party/server.ts" }"#,
            ),
            (
                "party/server.ts",
                "import type * as Party from \"partykit/server\";\n\
                 export default class MainServer implements Party.Server {}\n",
            ),
            (
                "party/extra.ts",
                "import type * as Party from \"partykit/server\";\n\
                 export default class ExtraServer implements Party.Server {}\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "party/extra.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("default")),
            "an unlisted `party/` server class must be exempt via convention, got: {diags:?}"
        );
    }

    // Negative-space guard for #5403 — a genuinely-unused export in an ordinary
    // module of a PartyKit project (outside `party/`, not in `partykit.json`)
    // must still be flagged.
    #[test]
    fn still_flags_dead_export_in_ordinary_module_of_partykit_project_issue_5403() {
        let files: Vec<(&str, &str)> = vec![
            (
                "partykit.json",
                r#"{ "name": "partykit-site", "main": "party/server.ts" }"#,
            ),
            (
                "party/server.ts",
                "import type * as Party from \"partykit/server\";\n\
                 export default class MainServer implements Party.Server {}\n",
            ),
            ("src/orphan.ts", "export const deadHelper = 1;\n"),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/orphan.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("deadHelper")),
            "a dead export in an ordinary module of a PartyKit project must still be flagged, got: {diags:?}"
        );
    }

    // Negative-space guard for #5403 — a `party/`-directory class with no
    // `partykit.json` in the project is not a PartyKit entry (the convention
    // fallback is gated on a manifest), so a genuinely-dead default export still
    // fires.
    #[test]
    fn still_flags_party_dir_class_without_partykit_manifest_issue_5403() {
        let files: Vec<(&str, &str)> = vec![
            (
                "party/server.ts",
                "export default class NotAPartyServer {}\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "party/server.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("default")),
            "a `party/` class without a partykit.json must still be flagged, got: {diags:?}"
        );
    }

    // Regression for #5403 — convention fallback covers the plural `parties/`
    // directory and the named-import `extends Server` heritage form.
    #[test]
    fn no_fp_for_partykit_convention_parties_dir_extends_server_issue_5403() {
        let files: Vec<(&str, &str)> = vec![
            (
                "partykit.json",
                r#"{ "name": "partykit-site", "main": "parties/main.ts" }"#,
            ),
            (
                "parties/chat.ts",
                "import { Server } from \"partykit/server\";\n\
                 export default class ChatServer extends Server {}\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "parties/chat.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("default")),
            "a `parties/` server class extending Server must be exempt, got: {diags:?}"
        );
    }

    // Negative-space guard for #5403 — a `party/` class whose default export
    // extends an unrelated namespaced `*.Server` (not `Party.Server`) is not a
    // PartyKit server, so a genuinely-dead default export still fires.
    #[test]
    fn still_flags_party_dir_class_extending_unrelated_namespaced_server_issue_5403() {
        let files: Vec<(&str, &str)> = vec![
            (
                "partykit.json",
                r#"{ "name": "partykit-site", "main": "party/server.ts" }"#,
            ),
            (
                "party/server.ts",
                "import type * as Party from \"partykit/server\";\n\
                 export default class MainServer implements Party.Server {}\n",
            ),
            (
                "party/http.ts",
                "import * as http from \"node:http\";\n\
                 export default class MyHttp extends http.Server {}\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "party/http.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("default")),
            "a party/ class extending an unrelated *.Server must still be flagged, got: {diags:?}"
        );
    }

    // Regression for #6081 (urql-graphql/urql) — an ESLint shareable-config file
    // referenced by path from `package.json#eslintConfig.extends` is loaded by
    // ESLint's config system by path, never through a module `import`, so its
    // named exports (`parser`, `parserOptions`, `extends`, …) have no in-repo
    // importer yet are live. The whole file is a config entry point.
    #[test]
    fn no_fp_for_eslint_preset_referenced_via_eslint_config_extends_issue_6081() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{ "name": "app", "eslintConfig": { "root": true, "extends": ["./scripts/eslint/preset.js"] } }"#,
            ),
            (
                "scripts/eslint/preset.js",
                "module.exports = {\n\
                   parser: '@typescript-eslint/parser',\n\
                   parserOptions: { project: true },\n\
                   ignorePatterns: ['dist/'],\n\
                   extends: ['eslint:recommended'],\n\
                 };\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "scripts/eslint/preset.js");
        assert!(
            diags.is_empty(),
            "an eslint preset referenced via eslintConfig.extends must not be flagged dead, got: {diags:?}"
        );
    }

    // Regression for #6081 — a shared Rollup config referenced from a sibling
    // workspace package's build script by a `../../scripts/…` path. The
    // referencing manifest is not an ancestor of the config file, so recognition
    // must scan workspace-root manifests, not just the file's nearest one. Rollup
    // loads the config by path (`rollup -c …/config.mjs`), never through an
    // import, so its `export default` is live.
    #[test]
    fn no_fp_for_shared_rollup_config_referenced_from_sibling_package_issue_6081() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{ "name": "monorepo", "private": true, "workspaces": ["packages/*"] }"#,
            ),
            (
                "packages/core/package.json",
                r#"{ "name": "@app/core", "scripts": { "build": "rollup -c ../../scripts/rollup/config.mjs" } }"#,
            ),
            (
                "packages/core/src/index.ts",
                "export const core = 1;\n",
            ),
            (
                "scripts/rollup/config.mjs",
                "export default [\n  { input: 'src/index.ts', output: { file: 'dist/index.js' } },\n];\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "scripts/rollup/config.mjs");
        assert!(
            diags.iter().all(|d| !d.message.contains("default")),
            "a shared rollup config referenced from a sibling package must not be flagged dead, got: {diags:?}"
        );
    }

    // Negative-space guard for #6081 — a genuinely-unused internal export in an
    // ordinary module of a project that DOES reference config files by path must
    // still be flagged. The exemption is scoped to the referenced config files,
    // not the whole project.
    #[test]
    fn still_flags_dead_export_in_ordinary_module_with_config_references_issue_6081() {
        let files: Vec<(&str, &str)> = vec![
            (
                "package.json",
                r#"{ "name": "app", "eslintConfig": { "extends": ["./scripts/eslint/preset.js"] } }"#,
            ),
            (
                "scripts/eslint/preset.js",
                "module.exports = { parser: 'x' };\n",
            ),
            (
                "src/orphan.ts",
                "export function deadHelper() {}\n",
            ),
            ("src/app.ts", "export const used = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/orphan.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("deadHelper")),
            "a dead export in an ordinary module must still be flagged, got: {diags:?}"
        );
    }
}
