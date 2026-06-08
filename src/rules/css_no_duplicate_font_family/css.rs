use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["declaration"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if prop_name != "font-family" { return; }

    // Build comma-separated font-name segments from value-side children.
    let mut prop_seen = false;
    let mut segments: Vec<(String, tree_sitter::Node)> = Vec::new();
    let mut current_words: Vec<String> = Vec::new();
    let mut current_anchor: Option<tree_sitter::Node> = None;
    for ch in &kids {
        if ch.kind() == "property_name" {
            prop_seen = true;
            continue;
        }
        if !prop_seen { continue; }
        let txt = ch.utf8_text(source).unwrap_or("").trim();
        if txt == "," {
            if let Some(anchor) = current_anchor.take() {
                segments.push((current_words.join(" "), anchor));
            }
            current_words.clear();
            continue;
        }
        match ch.kind() {
            "plain_value" => {
                current_words.push(txt.to_string());
                if current_anchor.is_none() { current_anchor = Some(*ch); }
            }
            "string_value" => {
                let unq = txt.trim_matches(|c| c == '"' || c == '\'').to_string();
                current_words.push(unq);
                if current_anchor.is_none() { current_anchor = Some(*ch); }
            }
            ":" | ";" => {}
            _ => {}
        }
    }
    if let Some(anchor) = current_anchor.take() {
        segments.push((current_words.join(" "), anchor));
    }

    let mut seen: Vec<String> = Vec::new();
    for (name, anchor) in &segments {
        let normalized = name.to_ascii_lowercase();
        if normalized.is_empty() { continue; }
        if seen.iter().any(|n| n == &normalized) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                anchor,
                super::META.id,
                format!("Duplicate font `{name}` in font-family list."),
                Severity::Warning,
            ));
        } else {
            seen.push(normalized);
        }
    }
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    #[test]
    fn flags_duplicate_font() {
        let css = ".a { font-family: Arial, Helvetica, Arial, sans-serif; }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_distinct_fonts() {
        let css = ".a { font-family: Arial, Helvetica, sans-serif; }";
        assert!(run(css).is_empty());
    }
}
