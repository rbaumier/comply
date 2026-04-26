use crate::diagnostic::{Diagnostic, Severity};

const DEPRECATED: &[&str] = &[
    "tty",
    "tv",
    "projection",
    "handheld",
    "braille",
    "embossed",
    "aural",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "media_statement" { return; }
    let text = node.utf8_text(source).unwrap_or_default();
    let header = text.split_once('{').map_or(text, |(head, _)| head);
    for name in DEPRECATED {
        let present = header
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
            .any(|part| part.eq_ignore_ascii_case(name));
        if present {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("Deprecated media type `{name}`; use `screen`, `print`, or `all`."),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_tv_media_type() {
        let css = "@media tv { .a { color: red; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_screen_media_type() {
        let css = "@media screen { .a { color: red; } }";
        assert!(run(css).is_empty());
    }
}
