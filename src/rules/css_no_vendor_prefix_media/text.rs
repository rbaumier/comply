use crate::diagnostic::{Diagnostic, Severity};

const PREFIXES: &[&str] = &["-webkit-", "-moz-", "-ms-", "-o-"];

crate::ast_check! { on ["media_statement"] prefilter = ["-webkit-", "-moz-", "-ms-", "-o-"] => |node, source, ctx, diagnostics|
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
