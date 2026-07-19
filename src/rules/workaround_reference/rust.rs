use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return };
    if !super::has_keyword(text) { return; }
    if super::has_reference(text) { return; }
    if super::has_reason_clause(text) { return; }

    let row = node.start_position().row;
    let lines: Vec<&str> = ctx.source.lines().collect();
    let lookahead = (row + 1..=(row + 2).min(lines.len().saturating_sub(1)))
        .any(|i| super::has_reference(lines[i]));
    if lookahead { return; }

    // Lookback: a nearby preceding comment line may already state why the hack
    // is needed, leaving no ticket to require.
    let lookback = (row.saturating_sub(2)..row).any(|i| {
        let line = lines[i].trim_start();
        line.starts_with("//") && super::has_reason_clause(line)
    });
    if lookback { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Workaround/hack comment without an issue reference — \
         add a link or ticket number.".into(),
        Severity::Error,
    ));
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_workaround_without_ref() {
        assert_eq!(run("// Workaround for fish\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_workaround_with_issue_ref() {
        assert!(run("// Workaround for a fish bug (see #739, #279)\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_workaround_with_url() {
        assert!(run("// Workaround for https://github.com/org/repo/issues/1\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_ref_on_next_line() {
        assert!(run("// Workaround for fish bug\n// See #739\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_hack_without_ref() {
        assert_eq!(run("// hack to fix rendering\nfn f() {}").len(), 1);
    }

    #[test]
    fn compat_alone_does_not_trigger() {
        // `compat` is a compatibility-domain noun/adjective, not a workaround
        // marker, so it does not fire on its own.
        assert!(run("// compat shim for old API\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_compatibility_notes() {
        // deno false positives: `compat` as a standalone compatibility noun.
        assert!(run("// node compat test\nfn f() {}").is_empty());
        assert!(run("// Backwards compat. cb() is undocumented\nfn f() {}").is_empty());
        assert!(run("// (node compat)\nfn f() {}").is_empty());
        assert!(run("// compat surface does for reporter detection\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_compat_workaround() {
        // `compat` next to a genuine `workaround` keyword still fires.
        assert_eq!(run("// compat workaround for old API\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_hack_with_colon() {
        assert_eq!(run("// hack: patch the output\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_jira_ref() {
        assert!(run("// Workaround for PROJ-123\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_hack_with_inline_reason() {
        // Reason connector on the same line as the keyword, mirroring how an
        // inline issue reference is accepted.
        assert!(run("// hack because the API can't expose codes directly\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_hack_with_reason_two_lines_up() {
        // Far edge of the 2-line lookback window: reason on row-2, an unrelated
        // comment on row-1.
        let src = "    // because the API is limited\n    // see the implementation note\n    // hack to patch the output\n    fn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_hack_with_reason_on_preceding_line() {
        // hexyl colors.rs:30 — the second line of a self-explanatory comment
        // block; the line above already states the reason ("isn't designed").
        let src = "    // owo_colors' API isn't designed to get the terminal codes directly for\n    // dynamic colors, so we use this hack to get them from the LHS of some text.\n    fn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_hack_when_preceding_comment_has_no_reason() {
        let src = "    // some unrelated comment\n    // hack to fix rendering\n    fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_hack_when_reason_word_is_in_preceding_code() {
        // A reason connector in a non-comment line must not suppress: the
        // explanation has to live in the comment block, not arbitrary code.
        let src = "    let msg = \"because of latency\";\n    // hack to fix rendering\n    fn f() {}";
        assert_eq!(run(src).len(), 1);
    }
}
