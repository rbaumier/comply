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
}
