//! Flag `fetch('/api/...')` or `fetch(\`/api/...\`)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(name) = callee.utf8_text(source) else { return; };
    if name != "fetch" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first) = first_named_arg(args) else { return; };
    let Some(literal) = literal_string_value(first, source) else { return; };
    if !literal.starts_with("/api/") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Don't `fetch('{literal}')` your own app. Call a `createServerFn` \
             instead for typed, round-trip-free access."
        ),
        Severity::Warning,
    ));
}

fn first_named_arg<'a>(args: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = args.walk();
    args.children(&mut cursor).find(|c| c.is_named())
}

fn literal_string_value<'a>(n: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match n.kind() {
        "string" => {
            let text = n.utf8_text(source).ok()?;
            Some(text.trim_matches(|c| c == '"' || c == '\''))
        }
        "template_string" => {
            // Only treat as a plain literal if there are no ${...} substitutions.
            let mut cursor = n.walk();
            let has_subst = n
                .children(&mut cursor)
                .any(|c| c.kind() == "template_substitution");
            if has_subst { return None; }
            let text = n.utf8_text(source).ok()?;
            Some(text.trim_matches('`'))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_fetch_api_string() {
        assert_eq!(run("fetch('/api/users');").len(), 1);
    }

    #[test]
    fn flags_fetch_api_template() {
        assert_eq!(run("fetch(`/api/users`);").len(), 1);
    }

    #[test]
    fn allows_external_fetch() {
        assert!(run("fetch('https://example.com/data');").is_empty());
    }

    #[test]
    fn allows_non_api_path() {
        assert!(run("fetch('/health');").is_empty());
    }
}
