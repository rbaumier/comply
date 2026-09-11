use crate::diagnostic::Diagnostic;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::comment_blocks;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        super::banner_diagnostics(comment_blocks::from_oxc(semantic, ctx.source), ctx)
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_a_section_banner() {
        let src = "\
// ─── Import a centrale catalogue ──────────────────────────────────────────
export async function importCatalogue() {}";
        let flags = run(src);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].line, 1);
    }

    #[test]
    fn flags_a_bare_separator_line() {
        assert_eq!(run("// ==========================\nconst count = 1;").len(), 1);
    }

    #[test]
    fn flags_a_rule_that_only_closes_the_line() {
        assert_eq!(run("// Handlers -----------------\nconst count = 1;").len(), 1);
    }

    #[test]
    fn flags_each_rule_of_a_boxed_label() {
        let src = "\
// ==========
// Handlers
// ==========
const count = 1;";
        let lines: Vec<usize> = run(src).iter().map(|flag| flag.line).collect();
        assert_eq!(lines, [1, 3]);
    }

    #[test]
    fn flags_a_banner_in_a_block_comment() {
        assert_eq!(run("/* ---- Section ---- */\nconst count = 1;").len(), 1);
    }

    #[test]
    fn a_trailing_ellipsis_is_prose() {
        assert!(run("// Waits for the pool to drain...\nconst count = 1;").is_empty());
    }

    #[test]
    fn a_directory_tree_is_not_a_banner() {
        let src = "\
// src/
// ├── index.ts
// │   └── routes.ts
const count = 1;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn a_rule_inside_a_fenced_sample_is_not_a_banner() {
        let src = "\
/**
 * Parses front matter.
 * ```md
 * ---
 * title: Hello
 * ---
 * ```
 */
export function parse() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn a_markdown_heading_is_not_a_banner() {
        let src = "\
/**
 * ### Usage
 * Call it once.
 */
export function boot() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn a_license_header_is_exempt() {
        let src = "\
/*! *****************************************************************************
Copyright (c) Microsoft Corporation.
***************************************************************************** */
export function helper() {}";
        assert!(run(src).is_empty());
    }
}
