//! comment-requires-tag — TypeScript/JavaScript backend.

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
        super::diagnose(comment_blocks::from_oxc(semantic, ctx.source), ctx)
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_untagged_line_comment() {
        let diagnostics = run("// fetch the session\nconst session = load();");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
    }

    #[test]
    fn flags_a_run_of_lines_once() {
        let source = "// fetch the session\n// then hand it to the router\nconst s = load();";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_untagged_block_comment() {
        assert_eq!(run("/* fetch the session */\nconst s = load();").len(), 1);
    }

    #[test]
    fn allows_every_accepted_tag() {
        for tagged in [
            "// why: the loader runs before the router resolves the match",
            "// gotcha: getSession returns null for 200ms after a refresh",
            "// TODO(#123): drop the shim once the upstream fix lands",
            "// WORKAROUND(upstream#704): the released package still throws",
        ] {
            assert!(run(&format!("{tagged}\nconst s = load();")).is_empty(), "flagged: {tagged}");
        }
    }

    #[test]
    fn allows_jsdoc() {
        assert!(run("/** The active session. */\nexport const s = load();").is_empty());
        assert!(run("/**\n * The active session.\n */\nexport const s = load();").is_empty());
    }

    #[test]
    fn allows_triple_slash_reference() {
        assert!(run("/// <reference types=\"vite/client\" />\nconst s = load();").is_empty());
    }

    #[test]
    fn allows_tool_directives() {
        for directive in [
            "// eslint-disable-next-line no-console",
            "// @ts-expect-error the upstream types are wrong",
            "// biome-ignore lint/suspicious/noExplicitAny: boundary",
            "// prettier-ignore",
            "// oxlint-disable-next-line no-console",
            "/* istanbul ignore next */",
            "/* c8 ignore next */",
            "// @vitest-environment jsdom",
            "// #region session",
        ] {
            assert!(run(&format!("{directive}\nconst s = load();")).is_empty(), "flagged: {directive}");
        }
    }

    #[test]
    fn allows_license_header() {
        assert!(run("// Copyright 2026 the authors\nconst s = load();").is_empty());
    }

    #[test]
    fn allows_decorative_separator() {
        assert!(run("// =================================\nconst s = load();").is_empty());
    }

    #[test]
    fn leaves_commented_out_code_to_its_own_rule() {
        assert!(run("// const previous = load();\nconst s = load();").is_empty());
    }
}
