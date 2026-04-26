use crate::diagnostic::{Diagnostic, Severity};

const GENERIC: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
    "emoji",
    "math",
    "fangsong",
    "inherit",
    "initial",
    "unset",
    "revert",
    "revert-layer",
];

crate::ast_check! { on ["declaration"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if prop_name != "font-family" { return; }

    // Skip if value uses var() — author may already include a generic via the variable.
    if kids.iter().any(|n| n.kind() == "call_expression") { return; }

    // Build the last segment after the final comma.
    let mut prop_seen = false;
    let mut last_segment_words: Vec<String> = Vec::new();
    let mut last_was_string = false;
    for ch in &kids {
        if ch.kind() == "property_name" {
            prop_seen = true;
            continue;
        }
        if !prop_seen { continue; }
        let txt = ch.utf8_text(source).unwrap_or("").trim();
        if txt == "," {
            last_segment_words.clear();
            last_was_string = false;
            continue;
        }
        match ch.kind() {
            "plain_value" => {
                last_segment_words.push(txt.to_ascii_lowercase());
                last_was_string = false;
            }
            "string_value" => {
                last_segment_words.push(txt.trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase());
                last_was_string = true;
            }
            _ => {}
        }
    }
    if last_segment_words.is_empty() { return; }
    // A quoted final value is NEVER generic.
    if last_was_string {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Font stack should end with a generic family (e.g. `sans-serif`).".into(),
            Severity::Warning,
        ));
        return;
    }
    let last = last_segment_words.join(" ");
    if !GENERIC.iter().any(|g| *g == last) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Font stack ends with `{last}`; add a generic family fallback."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_missing_generic() {
        assert_eq!(run(".a { font-family: Arial; }").len(), 1);
    }

    #[test]
    fn allows_with_generic() {
        assert!(run(".a { font-family: Arial, sans-serif; }").is_empty());
    }
}
