use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(raw) = node.utf8_text(source) else { return };
    if !super::mentions_history(raw) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Comment narrates history (`was`, `previously`, `refactored`, `rewritten`). Describe current behaviour — history lives in git log.".into(),
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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_previously_used() {
        let src = "// previously used HashMap\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_was_refactored() {
        let src = "// was refactored from a Vec\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_rewritten() {
        let src = "// rewritten in v3\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_neutral_comment() {
        assert!(run("// caches results\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_descriptive_was() {
        assert!(run("// whether it was the initial value\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_doc_comments() {
        assert!(run("/// Return to the previously set panic hook\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_doc_comment_with_was() {
        assert!(run("/// Track which source stream the value was received from\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_runtime_filesystem_condition() {
        // Regression for issue #3242: "was deleted" with an indefinite/domain
        // subject describes a runtime filesystem event, not code history.
        assert!(run("// some file was deleted\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_ambiguous_verb_with_code_subject() {
        assert_eq!(run("// the validateUser function was removed\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_ambiguous_verb_with_camelcase_subject() {
        // A bare camelCase/PascalCase identifier is a code artifact even without
        // an artifact noun like "function".
        assert_eq!(run("// validateUser was removed\nfn f() {}").len(), 1);
        assert_eq!(run("// MyComponent was renamed\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_in_favor_of_marker() {
        assert_eq!(run("// The old cache layer was removed in favor of Redis\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_config_in_favor_of() {
        // Regression for issue #4784: "in favor of" in a lint/config comment
        // describes the current configuration choice, not code history.
        assert!(run("// Disabled in favor of import/no-duplicates\nfn f() {}").is_empty());
        assert!(run("// off in favor of our custom rule\nfn f() {}").is_empty());
        assert!(run("// prefer x in favor of y\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_was_replaced_test_assertion() {
        // Regression for issue #4526: "content was replaced" is a test-assertion
        // description (subject "content" is not a code artifact), not code history.
        assert!(run("// Check if node content was replaced correctly\nfn f() {}").is_empty());
        assert!(run("// the value was replaced at runtime\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_was_replaced_with_code_subject() {
        assert_eq!(run("// the function was replaced with a hook\nfn f() {}").len(), 1);
        assert_eq!(run("// this method was replaced by useFoo\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_previously_called_runtime_invocation() {
        // Regression for issue #6066: "next() was previously called" documents a
        // prior runtime invocation of the iterator's next() method (state-machine
        // doc), not a code rename. The subject is a call expression.
        let src = "// next() was previously called, the current record has already been returned\nfn f() {}";
        assert!(run(src).is_empty());
        assert!(run("// self.poll() was previously called\nfn f() {}").is_empty());
        assert!(run("// Iterator::next() was previously called\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_previously_called_rename() {
        // The rename reading ("X was previously called oldName") still fires: the
        // subject is a noun, not a call expression.
        assert_eq!(run("// previously called fooBar\nfn f() {}").len(), 1);
        assert_eq!(run("// this method was previously called oldName\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_version_anchored_external_change() {
        // Regression for issue #6145: a code-artifact subject followed by a
        // version-anchored "in" clause documents an external API change tied to
        // a release, not this project's own history.
        assert!(run("// getCallSite was renamed in Node 23.3.0 / 22.12.0\nfn f() {}").is_empty());
        assert!(run("// fooBar was removed in v2.0.0\nfn f() {}").is_empty());
        assert!(run("// fooBar was renamed in 2.0\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_conditional_refactored_clause() {
        // Regression for issue #6917: "if we refactored X" is a conditional
        // (hypothetical) design musing, not past code-history narration.
        let src = "// Alternatively, if we refactored the grep interfaces to pass along the\n// full set of matches, then that might also help here.\nfn f() {}";
        assert!(run(src).is_empty());
        assert!(run("// unless they refactored it, this stays\nfn f() {}").is_empty());
        // "rewrote" is not a history word, so a conditional rewrite reads clean.
        assert!(run("// if you rewrote this it would simplify\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_when_refactored_past_narration() {
        // "when" is deliberately excluded from the conditional exemption:
        // "when we refactored the API" narrates a past event (code history).
        assert_eq!(run("// when we refactored the API, the cache broke\nfn f() {}").len(), 1);
        // A pronoun clause without a conditional conjunction still narrates.
        assert_eq!(run("// we refactored the parser into two passes\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_past_refactored_without_conditional_head() {
        // Past-tense narration without a conditional head must still fire.
        assert_eq!(run("// was refactored from a Vec\nfn f() {}").len(), 1);
        assert_eq!(run("// the module was rewritten in v3\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_modal_passive_refactored() {
        // The pre-existing modal-passive exemption still holds.
        assert!(run("// this should be refactored later\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_rewritten_attributive_adjective() {
        // Regression for issue #7489: in a columnar/compaction data system,
        // `rewritten <noun>` is domain terminology (a data row physically
        // rewritten into a new fragment), not code-history narration. The word
        // is an attributive adjective — followed by the noun it modifies, not in
        // a verb slot — so it must not fire.
        assert!(run("// maps a rewritten row's group-local index to a new address\nfn f() {}").is_empty());
        assert!(run("// Rewritten old row addresses must reference only fragments\nfn f() {}").is_empty());
        assert!(run("// Rewritten old rows are mapped positionally onto the new rows\nfn f() {}").is_empty());
        assert!(run("// Deleted offsets inside a rewritten fragment.\nfn f() {}").is_empty());
        // Generalizes across domains — not a row/fragment word list.
        assert!(run("// forwards the rewritten packet to the next hop\nfn f() {}").is_empty());
        assert!(run("// caches the refactored query plan\nfn f() {}").is_empty());
        // A quantifier heads the noun phrase too: "one rewritten <noun>".
        assert!(run("// at most one rewritten fragment per group\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_rewritten_refactored_verb_forms() {
        // The verb (history) readings still fire: passive with a non-"was"
        // auxiliary, a verb-complement follower, and an object pronoun (the verb
        // takes an object, not a noun it modifies).
        assert_eq!(run("// these fragments were rewritten during the last pass\nfn f() {}").len(), 1);
        assert_eq!(run("// later refactored into two passes\nfn f() {}").len(), 1);
        assert_eq!(run("// refactored it to drop the intermediate buffer\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_rename_without_version_anchor() {
        // Negative space for #6145: an internal rename with no version anchor is
        // still code-history narration and must fire.
        assert_eq!(run("// getCallSite was renamed to getCallSites\nfn f() {}").len(), 1);
        // A version anchored to an "in" clause is required: a bare version in the
        // subject, a single-component version, or a version several words past
        // the "in" does not document an external release change.
        assert_eq!(run("// fooBar 2.0 was renamed\nfn f() {}").len(), 1);
        assert_eq!(run("// fooBar was renamed in v22\nfn f() {}").len(), 1);
        assert_eq!(run("// fooBar was renamed in the migration to v2.0 schema\nfn f() {}").len(), 1);
    }
}
