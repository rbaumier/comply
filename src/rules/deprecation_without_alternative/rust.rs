use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["attribute_item"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");

    if !text.contains("deprecated") {
        return;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
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
    fn ignores_non_deprecated_attrs() {
        assert!(run("#[derive(Debug)]\nstruct S;").is_empty());
    }

    #[test]
    fn flags_deprecated_on_struct() {
        assert_eq!(run("#[deprecated]\npub struct Old;").len(), 1);
    }
}
