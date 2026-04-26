use crate::diagnostic::{Diagnostic, Severity};

const PREFIXES: &[&str] = &["@-webkit-", "@-moz-", "@-ms-", "@-o-"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !matches!(node.kind(), "at_rule" | "keyframes_statement") { return; }
    let mut c = node.walk();
    let Some(kw) = node.children(&mut c).find(|n| n.kind() == "at_keyword") else { return; };
    let text = kw.utf8_text(source).unwrap_or_default();
    if !PREFIXES.iter().any(|p| text.starts_with(p)) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &kw,
        super::META.id,
        format!("Vendor-prefixed at-rule `{text}`; remove the prefix and rely on autoprefixer."),
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
    fn flags_webkit_keyframes() {
        let css = "@-webkit-keyframes slide { from { left: 0; } to { left: 100px; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_moz_document() {
        assert_eq!(
            run("@-moz-document url-prefix() { .a { color: red; } }").len(),
            1
        );
    }

    #[test]
    fn allows_unprefixed_keyframes() {
        let css = "@keyframes slide { from { left: 0; } to { left: 100px; } }";
        assert!(run(css).is_empty());
    }
}
