use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "stylesheet" { return; }
    let mut c = node.walk();
    let mut seen: Vec<String> = Vec::new();
    for kid in node.children(&mut c) {
        let kind = kid.kind();
        let is_import = kind == "import_statement"
            || (kind == "at_rule" && first_keyword(&kid, source).eq_ignore_ascii_case("@import"));
        if !is_import { continue; }
        let Some(target) = extract_import_target(&kid, source) else { continue; };
        if seen.iter().any(|s| s == &target) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &kid,
                super::META.id,
                format!("Duplicate `@import` of {target}."),
                Severity::Warning,
            ));
        } else {
            seen.push(target);
        }
    }
}

fn first_keyword(node: &tree_sitter::Node<'_>, source: &[u8]) -> String {
    let mut c = node.walk();
    node.children(&mut c)
        .find(|n| n.kind() == "at_keyword")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or_default()
        .to_string()
}

fn extract_import_target(node: &tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let mut c = node.walk();
    for child in node.children(&mut c) {
        match child.kind() {
            "string_value" => {
                let raw = child.utf8_text(source).ok()?;
                return Some(strip_quotes(raw).to_string());
            }
            "call_expression" => {
                // url(...) form
                let mut cc = child.walk();
                for grand in child.children(&mut cc) {
                    if grand.kind() == "arguments" {
                        let mut gc = grand.walk();
                        for arg in grand.children(&mut gc) {
                            if arg.kind() == "string_value" || arg.kind() == "plain_value" {
                                let raw = arg.utf8_text(source).ok()?;
                                return Some(strip_quotes(raw).to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_quotes(s: &str) -> &str {
    let t = s.trim();
    let bytes = t.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        &t[1..t.len() - 1]
    } else {
        t
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
    fn flags_duplicate_string_imports() {
        let css = "@import \"a.css\"; @import \"b.css\"; @import \"a.css\";";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_duplicate_url_imports() {
        let css = "@import url(\"a.css\"); @import url(\"a.css\");";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_unique_imports() {
        let css = "@import \"a.css\"; @import \"b.css\"; @import \"c.css\";";
        assert!(run(css).is_empty());
    }
}
