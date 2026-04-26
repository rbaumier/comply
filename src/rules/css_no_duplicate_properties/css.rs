use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["block"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let decls: Vec<_> = node
        .children(&mut c)
        .filter(|n| n.kind() == "declaration")
        .collect();
    let mut names: Vec<(String, tree_sitter::Node)> = Vec::new();
    for decl in &decls {
        let mut dc = decl.walk();
        if let Some(prop) = decl.children(&mut dc).find(|n| n.kind() == "property_name") {
            let name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
            // Skip custom properties (handled by another rule).
            if name.starts_with("--") {
                names.push((name, *decl));
                continue;
            }
            names.push((name, *decl));
        }
    }
    // Flag any duplicate that is not consecutive with its previous occurrence.
    for i in 0..names.len() {
        let (name, decl) = (&names[i].0, names[i].1);
        if name.starts_with("--") { continue; }
        let prev = names.iter().take(i).rposition(|(n, _)| n == name);
        let Some(p) = prev else { continue; };
        // If consecutive (p == i-1), skip — fallback pattern allowed.
        if p + 1 == i { continue; }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &decl,
            super::META.id,
            format!("Duplicate property `{name}` in block."),
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
    fn flags_non_consecutive_duplicate() {
        let css = ".a { color: red; font-size: 14px; color: blue; }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_consecutive_fallback() {
        let css = ".a { color: red; color: blue; }";
        assert!(run(css).is_empty());
    }
}
