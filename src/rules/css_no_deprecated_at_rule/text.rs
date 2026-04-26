use crate::diagnostic::{Diagnostic, Severity};

const DEPRECATED: &[(&str, &str)] = &[
    (
        "@document",
        "use feature queries (`@supports`) or scoping at the markup level",
    ),
    (
        "@viewport",
        "use a `<meta name=\"viewport\">` tag in HTML instead",
    ),
    (
        "@-ms-viewport",
        "use a `<meta name=\"viewport\">` tag in HTML instead",
    ),
];

crate::ast_check! { on ["at_rule", "keyframes_statement", "media_statement"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(kw) = node.children(&mut c).find(|n| n.kind() == "at_keyword") else { return; };
    let text = kw.utf8_text(source).unwrap_or_default();
    let lower = text.to_ascii_lowercase();
    for (name, hint) in DEPRECATED {
        if lower == *name {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &kw,
                super::META.id,
                format!("`{name}` is deprecated; {hint}."),
                Severity::Warning,
            ));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_css(s, &Check)
    }

    #[test]
    fn flags_viewport() {
        assert_eq!(run("@viewport { width: device-width; }").len(), 1);
    }

    #[test]
    fn flags_document() {
        assert_eq!(
            run("@document url-prefix() { .a { color: red; } }").len(),
            1
        );
    }

    #[test]
    fn allows_supports() {
        assert!(run("@supports (display: grid) { .a { display: grid; } }").is_empty());
    }

    #[test]
    fn allows_media() {
        assert!(run("@media (width: 100vw) { .a { color: red; } }").is_empty());
    }
}
