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
}
