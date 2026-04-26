use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "comment" { return; }
    let text = node.utf8_text(source).unwrap_or_default();
    let inner = text.strip_prefix("/*").and_then(|s| s.strip_suffix("*/")).unwrap_or(text);
    if !inner.trim().is_empty() { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Empty comment; remove or add content.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_empty_with_space() {
        assert_eq!(run("/* */").len(), 1);
    }

    #[test]
    fn flags_empty_no_space() {
        assert_eq!(run("/**/").len(), 1);
    }

    #[test]
    fn allows_comment_with_text() {
        assert!(run("/* some text */").is_empty());
    }
}
