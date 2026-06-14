use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["attribute_item"] => |node, source, ctx, diagnostics|
    if crate::rules::rust_helpers::is_under_tests_dir(ctx.path) {
        return;
    }

    // Only the `#[deprecated]` *declaration* attribute is in scope. A lint
    // attribute such as `#[allow(deprecated)]` / `#[expect(deprecated)]` is a
    // suppression *usage*, not a deprecation declaration: its path identifier is
    // `allow`/`expect`, and `deprecated` only appears inside its argument list.
    // Discriminate on the attribute path rather than on substring presence.
    if attribute_path(node, source) != Some("deprecated") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");

    // #[deprecated] without arguments — always flag.
    // #[deprecated(...)] — flag if no `note` key.
    if let Some(paren_start) = text.find('(') {
        let inner = &text[paren_start..];
        if inner.contains("note") {
            return;
        }
        // `since` alone without `note` is still missing the alternative.
    } else {
        // Bare `#[deprecated]` — no message at all.
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`#[deprecated]` without a `note` — add `note = \"Use X instead\"` \
         so callers know the migration path."
            .into(),
        Severity::Warning,
    ));
}

/// Returns the path identifier of an `attribute_item` node, i.e. `deprecated`
/// for `#[deprecated(...)]` and `allow` for `#[allow(deprecated)]`. In
/// tree-sitter-rust the `attribute_item` wraps an `attribute` node whose first
/// `identifier` child is the attribute name; any arguments live in a following
/// `token_tree`. Returns `None` for path-qualified attributes (e.g.
/// `#[some::path]`) since the leading segment is a `scoped_identifier`.
fn attribute_path<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    let attribute = node
        .children(&mut cursor)
        .find(|c| c.kind() == "attribute")?;
    let mut attr_cursor = attribute.walk();
    let path = attribute
        .children(&mut attr_cursor)
        .find(|c| c.kind() == "identifier")?;
    path.utf8_text(source).ok()
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
    fn flags_bare_deprecated() {
        assert_eq!(run("#[deprecated]\npub fn old() {}").len(), 1);
    }

    #[test]
    fn flags_deprecated_with_since_only() {
        assert_eq!(
            run("#[deprecated(since = \"0.6.0\")]\npub fn old() {}").len(),
            1
        );
    }

    #[test]
    fn allows_deprecated_with_note() {
        assert!(
            run("#[deprecated(since = \"0.6.0\", note = \"Use new_fn instead\")]\npub fn old() {}")
                .is_empty()
        );
    }

    #[test]
    fn allows_deprecated_note_only() {
        assert!(
            run("#[deprecated(note = \"Use new_fn\")]\npub fn old() {}").is_empty()
        );
    }

    #[test]
    fn allows_deprecated_without_note_in_tests_dir() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "#[deprecated]\npub fn old() {}", "axum/src/routing/tests/merge.rs")
            .is_empty()
        );
    }

    #[test]
    fn ignores_non_deprecated_attrs() {
        assert!(run("#[derive(Debug)]\nstruct S;").is_empty());
    }

    #[test]
    fn flags_deprecated_on_struct() {
        assert_eq!(run("#[deprecated]\npub struct Old;").len(), 1);
    }

    #[test]
    fn ignores_allow_deprecated_suppression() {
        // From leptos reactive_graph/src/wrappers.rs — `#[allow(deprecated)]` is
        // a lint suppression, not a deprecation declaration (Closes #1483).
        let src = "#[allow(deprecated)]\nimpl<T> From<MaybeSignal<T>> for Signal<T>\n\
                   where T: Send + Sync + 'static {\n\
                   fn from(value: MaybeSignal<T>) -> Self {\n\
                   match value {\n\
                   MaybeSignal::Static(value) => Signal::stored(value),\n\
                   MaybeSignal::Dynamic(signal) => signal,\n\
                   }\n}\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_expect_deprecated_suppression() {
        assert!(run("#[expect(deprecated)]\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_warn_and_deny_deprecated() {
        assert!(run("#[warn(deprecated)]\nfn f() {}").is_empty());
        assert!(run("#[deny(deprecated)]\nfn f() {}").is_empty());
    }
}
