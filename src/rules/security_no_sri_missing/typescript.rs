//! security-no-sri-missing backend —
//! `<script src="https://...">` or `<link rel="stylesheet" href="https://...">`
//! without an `integrity` attribute.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] prefilter = ["<script", "<link"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "script" && tag != "link" {
        return;
    }

    let mut cursor = node.walk();
    let attrs: Vec<_> = node
        .children(&mut cursor)
        .filter(|c| c.kind() == "jsx_attribute")
        .collect();

    let mut has_integrity = false;
    let mut external_url: Option<String> = None;
    let mut is_stylesheet_link = false;

    for attr in &attrs {
        let name = crate::rules::jsx::jsx_attribute_name(*attr, source);
        match name {
            Some("integrity") => has_integrity = true,
            Some("src") | Some("href") => {
                if let Some(val) = crate::rules::jsx::jsx_attribute_string_value(*attr, source)
                    && (val.starts_with("https://") || val.starts_with("http://") || val.starts_with("//"))
                {
                    external_url = Some(val.to_string());
                }
            }
            Some("rel") => {
                if let Some(val) = crate::rules::jsx::jsx_attribute_string_value(*attr, source)
                    && val.eq_ignore_ascii_case("stylesheet")
                {
                    is_stylesheet_link = true;
                }
            }
            _ => {}
        }
    }

    // <link> only matters when rel="stylesheet".
    if tag == "link" && !is_stylesheet_link {
        return;
    }

    let Some(url) = external_url else {
        return;
    };
    if has_integrity {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "<{tag}> loads {url} without `integrity` — add an SRI hash to prevent CDN tampering."
        ),
        Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_external_script_without_integrity() {
        let src = r#"const x = <script src="https://cdn.example.com/lib.js" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_external_stylesheet_without_integrity() {
        let src = r#"const x = <link rel="stylesheet" href="https://cdn.example.com/lib.css" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_script_with_integrity() {
        let src = r#"const x = <script src="https://cdn.example.com/lib.js" integrity="sha384-abc" crossOrigin="anonymous" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_relative_script_without_integrity() {
        let src = r#"const x = <script src="/local.js" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_stylesheet_link() {
        let src = r#"const x = <link rel="icon" href="https://example.com/fav.ico" />;"#;
        assert!(run(src).is_empty());
    }
}
