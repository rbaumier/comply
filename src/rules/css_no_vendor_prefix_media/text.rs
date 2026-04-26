use crate::diagnostic::{Diagnostic, Severity};

const PREFIXES: &[&str] = &["-webkit-", "-moz-", "-ms-", "-o-"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "media_statement" { return; }
    let text = node.utf8_text(source).unwrap_or_default();
    // Only consider the media query header (before the `{` block).
    let header = text.split('{').next().unwrap_or(text);
    let lower = header.to_ascii_lowercase();
    let mut hit: Option<&str> = None;
    for p in PREFIXES {
        // Find a `(`-prefixed feature name with vendor prefix, e.g. `(-webkit-min-...`
        let needle = format!("({p}");
        if lower.contains(&needle) {
            hit = Some(p);
            break;
        }
    }
    let Some(prefix) = hit else { return; };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Vendor-prefixed media feature using `{prefix}`; remove the prefix and rely on autoprefixer."),
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
    fn flags_webkit_min_device_pixel_ratio() {
        let css = "@media (-webkit-min-device-pixel-ratio: 2) { .a { color: red; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_moz_prefix() {
        let css = "@media (-moz-min-device-pixel-ratio: 2) { .a { color: red; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_unprefixed_resolution() {
        let css = "@media (min-resolution: 2dppx) { .a { color: red; } }";
        assert!(run(css).is_empty());
    }
}
