use crate::diagnostic::{Diagnostic, Severity};

const KNOWN: &[&str] = &[
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "aspect-ratio",
    "min-aspect-ratio",
    "max-aspect-ratio",
    "orientation",
    "resolution",
    "min-resolution",
    "max-resolution",
    "color",
    "min-color",
    "max-color",
    "color-index",
    "min-color-index",
    "max-color-index",
    "monochrome",
    "min-monochrome",
    "max-monochrome",
    "scan",
    "grid",
    "hover",
    "any-hover",
    "pointer",
    "any-pointer",
    "prefers-reduced-motion",
    "prefers-color-scheme",
    "prefers-contrast",
    "prefers-reduced-transparency",
    "prefers-reduced-data",
    "forced-colors",
    "inverted-colors",
    "display-mode",
    "device-width",
    "device-height",
    "device-aspect-ratio",
    "min-device-width",
    "max-device-width",
    "min-device-height",
    "max-device-height",
    "color-gamut",
    "dynamic-range",
    "video-dynamic-range",
    "overflow-block",
    "overflow-inline",
    "update",
    "scripting",
];

fn inside_media(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "media_statement" {
            return true;
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { on ["feature_query", "parenthesized_value"] => |node, source, ctx, diagnostics|
    // tree-sitter-css represents `(min-width: 768px)` as a `feature_query`
    // (or `parenthesized_value` in older grammars). The first plain_value
    // inside is the feature name.
    if !inside_media(node) { return; }
    let mut c = node.walk();
    let Some(name_node) = node.children(&mut c).find(|n| n.kind() == "feature_name" || n.kind() == "plain_value") else { return; };
    let name = name_node.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if name.is_empty() { return; }
    if name.starts_with("-webkit-") || name.starts_with("-moz-") || name.starts_with("-ms-") { return; }
    if KNOWN.iter().any(|k| *k == name) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Unknown media feature `{name}`."),
        Severity::Warning,
    ));
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
    fn flags_unknown_feature() {
        let css = "@media (unknwon-feature: value) { .a { color: red; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_known_feature() {
        let css = "@media (min-width: 768px) { .a { color: red; } }";
        assert!(run(css).is_empty());
    }
}
