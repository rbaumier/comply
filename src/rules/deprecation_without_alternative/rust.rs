use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["attribute_item"] => |node, source, ctx, diagnostics|
    if crate::rules::rust_helpers::is_under_tests_dir(ctx.path) {
        return;
    }

    let Some(attribute) = attribute_node(node) else {
        return;
    };

    // Only the `#[deprecated]` *declaration* attribute is in scope. A lint
    // attribute such as `#[allow(deprecated)]` / `#[expect(deprecated)]` is a
    // suppression *usage*, not a deprecation declaration: its path identifier is
    // `allow`/`expect`, and `deprecated` only appears inside its argument list.
    // Discriminate on the attribute path rather than on substring presence.
    if attribute_path(attribute, source) != Some("deprecated") {
        return;
    }

    // `#[deprecated = "msg"]` shorthand: the `attribute` node carries a `value`
    // field (the message expression). It is equivalent to `note = "msg"`, so the
    // migration path is documented — do not flag.
    if attribute.child_by_field_name("value").is_some() {
        return;
    }

    // `#[deprecated(...)]`: the `arguments` field is a `token_tree`. Flag unless it
    // names a `note` key. `since` alone is still missing the alternative.
    // Bare `#[deprecated]` has no `arguments` field and is always flagged.
    if attribute
        .child_by_field_name("arguments")
        .is_some_and(|arguments| token_tree_has_note(arguments, source))
    {
        return;
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

/// Returns the `attribute` node wrapped by an `attribute_item`, i.e. the node
/// spanning `deprecated = "..."` inside `#[deprecated = "..."]`.
fn attribute_node(item: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = item.walk();
    item.children(&mut cursor).find(|c| c.kind() == "attribute")
}

/// Returns the path identifier of an `attribute` node, i.e. `deprecated` for
/// `#[deprecated(...)]` and `allow` for `#[allow(deprecated)]`. The first
/// `identifier` child is the attribute name. Returns `None` for path-qualified
/// attributes (e.g. `#[some::path]`) since the leading segment is a
/// `scoped_identifier`.
fn attribute_path<'a>(attribute: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = attribute.walk();
    let path = attribute
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")?;
    path.utf8_text(source).ok()
}

/// Returns true if the `token_tree` arguments of a `#[deprecated(...)]` attribute
/// contain a top-level `note` key. The token tree flattens its contents, so the
/// `note` identifier appears as a direct `identifier` child.
fn token_tree_has_note(token_tree: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = token_tree.walk();
    token_tree
        .children(&mut cursor)
        .any(|c| c.kind() == "identifier" && c.utf8_text(source) == Ok("note"))
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
    fn allows_deprecated_eq_shorthand() {
        // From axum-extra/src/extract/query.rs — the `= "msg"` shorthand is
        // equivalent to `note = "msg"` (Closes #1516).
        assert!(
            run("#[deprecated = \"see documentation\"]\npub struct Query<T>(pub T);").is_empty()
        );
        assert!(
            run("#[deprecated = \"Use new_fn instead\"]\npub fn old() {}").is_empty()
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
