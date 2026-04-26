use crate::diagnostic::{Diagnostic, Severity};

const PREFIXES: &[&str] = &["-webkit-", "-moz-", "-ms-", "-o-"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "declaration" { return; }
    let mut c = node.walk();
    let Some(prop) = node.children(&mut c).find(|n| n.kind() == "property_name") else { return; };
    let name = prop.utf8_text(source).unwrap_or_default();
    if !PREFIXES.iter().any(|p| name.starts_with(p)) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &prop,
        super::META.id,
        format!("Vendor-prefixed property `{name}`; remove the prefix and rely on autoprefixer."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_css(s, &Check)
    }

    #[test]
    fn flags_webkit_transform() {
        assert_eq!(run(".a { -webkit-transform: rotate(45deg); }").len(), 1);
    }

    #[test]
    fn flags_moz_user_select() {
        assert_eq!(run(".a { -moz-user-select: none; }").len(), 1);
    }

    #[test]
    fn allows_unprefixed_transform() {
        assert!(run(".a { transform: rotate(45deg); }").is_empty());
    }
}
