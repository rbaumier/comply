use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "declaration" { return; }
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if prop_name != "font-family" { return; }

    let declaration = node.utf8_text(source).unwrap_or_default();
    let value = declaration
        .split_once(':')
        .map_or("", |(_, rest)| rest)
        .trim()
        .trim_end_matches(';');

    for segment in value.split(',') {
        let font = segment.trim();
        if font.is_empty() || font.starts_with('"') || font.starts_with('\'') || font.contains('(') {
            continue;
        }
        if font.split_whitespace().nth(1).is_some() {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("Unquoted multi-word font name `{font}`; wrap in quotes."),
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
    fn flags_unquoted_multi_word() {
        assert_eq!(run(".a { font-family: Times New Roman; }").len(), 1);
    }

    #[test]
    fn allows_quoted_multi_word() {
        assert!(run(r#".a { font-family: "Times New Roman"; }"#).is_empty());
    }

    #[test]
    fn allows_single_word() {
        assert!(run(".a { font-family: Arial, sans-serif; }").is_empty());
    }
}
