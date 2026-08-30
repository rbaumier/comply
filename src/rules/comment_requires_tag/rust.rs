//! comment-requires-tag — Rust backend.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::comment_blocks::{self, RawComment};

pub struct Check;

type State = Vec<RawComment>;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["line_comment", "block_comment"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::new()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(collected) = state.and_then(|any| any.downcast_mut::<State>()) else {
            return;
        };
        if let Some(comment) = comment_blocks::from_tree_sitter(&node, ctx.source) {
            collected.push(comment);
        }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(comments) = state.and_then(|any| any.downcast::<State>().ok()) else {
            return;
        };
        diagnostics.extend(super::diagnose(*comments, ctx));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_untagged_line_comment() {
        let diagnostics = run("// caches the parsed manifest\nfn f() {}");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
    }

    #[test]
    fn flags_untagged_block_comment() {
        let source = "/* caches the parsed manifest\n   for the whole run */\nfn f() {}";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_a_run_of_lines_once() {
        let source = "// caches the parsed manifest\n// for the whole run\nfn f() {}";
        let diagnostics = run(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
    }

    #[test]
    fn flags_a_tag_that_only_reaches_the_second_line() {
        let source = "// caches the parsed manifest\n// why: reparsing costs a syscall\nfn f() {}";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_trailing_comment_after_code() {
        assert_eq!(run("fn f() { let n = 1; } // the counter\n").len(), 1);
    }

    #[test]
    fn allows_every_accepted_tag() {
        for tagged in [
            "// why: reparsing the manifest costs a syscall per call",
            "// gotcha: the broker replays the frame after a reconnect",
            "// TODO(#123): drop the shim once the upstream fix lands",
            "// FIXME(#123): the retry loop double-counts a timeout",
            "// WORKAROUND(upstream#704): the released crate still panics",
            "// HACK(#123): the header is rewritten in place",
            "// SAFETY: the pointer comes from a live Box",
        ] {
            assert!(run(&format!("{tagged}\nfn f() {{}}")).is_empty(), "flagged: {tagged}");
        }
    }

    #[test]
    fn allows_doc_comments() {
        assert!(run("/// The parsed manifest of the nearest crate.\nfn f() {}").is_empty());
        assert!(run("//! Manifest parsing.\nfn f() {}").is_empty());
        assert!(run("/** The parsed manifest. */\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_tool_directives() {
        assert!(run("// comply-ignore: rust-impl-debug-on-public-types\nfn f() {}").is_empty());
        assert!(run("// cspell:ignore repr\nfn f() {}").is_empty());
    }

    #[test]
    fn a_directive_does_not_shelter_the_prose_under_it() {
        // The directive splits the run, so the note below still answers on its own.
        let source = "// comply-ignore: rust-no-lossy-as-cast\n// the count is bounded\nfn f() {}";
        let diagnostics = run(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 2);
    }

    #[test]
    fn allows_license_header() {
        let source = "// Copyright 2026 the authors\n// SPDX-License-Identifier: MIT\nfn f() {}";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_decorative_separator() {
        assert!(run("// ---------------------------------\nfn f() {}").is_empty());
        assert!(run("// ── Helpers ──────────────────────\nfn f() {}").is_empty());
    }

    #[test]
    fn leaves_commented_out_code_to_its_own_rule() {
        assert!(run("// let x = compute(a, b);\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_a_file_with_no_comment() {
        assert!(run("fn f() {}").is_empty());
    }
}
