//! Flag `fetch('/api/...')` or `fetch(\`/api/...\`)` — but only in files that
//! import from `@tanstack/start` / `@tanstack/react-start`. We can't prove an
//! equivalent server fn exists; the import gate keeps us from flagging plain
//! React/Next/Remix codebases that legitimately call their own `/api/*`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(name) = callee.utf8_text(source) else { return; };
    if name != "fetch" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first) = first_named_arg(args) else { return; };
    let Some(literal) = literal_string_value(first, source) else { return; };
    if !literal.starts_with("/api/") { return; }
    if !file_uses_tanstack_start(source) { return; }

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

fn file_uses_tanstack_start(source: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(source) else { return false; };
    text.contains("@tanstack/start") || text.contains("@tanstack/react-start")
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

    const TANSTACK_IMPORT: &str = "import { createServerFn } from '@tanstack/start';\n";

    #[test]
    fn flags_fetch_api_string() {
        let src = format!("{TANSTACK_IMPORT}fetch('/api/users');");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_fetch_api_template() {
        let src = format!("{TANSTACK_IMPORT}fetch(`/api/users`);");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn allows_external_fetch() {
        let src = format!("{TANSTACK_IMPORT}fetch('https://example.com/data');");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_non_api_path() {
        let src = format!("{TANSTACK_IMPORT}fetch('/health');");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_when_no_tanstack_start_import() {
        // No TanStack Start in the file → don't flag (could be Next/Remix/etc.).
        assert!(run("fetch('/api/users');").is_empty());
    }
}
