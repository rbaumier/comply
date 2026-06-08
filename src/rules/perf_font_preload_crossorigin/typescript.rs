//! AST backend — flags JSX `<link rel="preload" as="font">` missing
//! `crossorigin` or `type="font/woff2"`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] prefilter = ["preload"] => |node, source, ctx, diagnostics|
    if jsx_element_tag_name(node, source) != Some("link") {
        return;
    }

    let mut rel: Option<String> = None;
    let mut as_attr: Option<String> = None;
    let mut has_crossorigin = false;
    let mut type_attr: Option<String> = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = jsx_attribute_name(child, source) else { continue };
        match name {
            "rel" => rel = jsx_attribute_string_value(child, source).map(str::to_owned),
            "as"  => as_attr = jsx_attribute_string_value(child, source).map(str::to_owned),
            "crossOrigin" | "crossorigin" => has_crossorigin = true,
            "type" => type_attr = jsx_attribute_string_value(child, source).map(str::to_owned),
            _ => {}
        }
    }

    // Only applies to `<link rel="preload" as="font">`
    if rel.as_deref() != Some("preload") || as_attr.as_deref() != Some("font") {
        return;
    }

    let missing_cors = !has_crossorigin;
    let missing_type = type_attr.as_deref() != Some("font/woff2");

    if missing_cors || missing_type {
        let msg = match (missing_cors, missing_type) {
            (true, true) => "Font preload `<link>` is missing both `crossorigin` and `type=\"font/woff2\"`.",
            (true, false) => "Font preload `<link>` is missing `crossorigin` — fonts are fetched in CORS mode.",
            (false, true) => "Font preload `<link>` should declare `type=\"font/woff2\"` so the preload matches the CSSOM request.",
            (false, false) => unreachable!(),
        };
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            msg.into(),
            Severity::Warning,
        ));
    }
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_missing_crossorigin_and_type() {
        assert_eq!(
            run(r#"const x = <link rel="preload" as="font" href="/f.woff2" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_missing_crossorigin_only() {
        assert_eq!(
            run(r#"const x = <link rel="preload" as="font" type="font/woff2" href="/f.woff2" />;"#)
                .len(),
            1
        );
    }

    #[test]
    fn allows_complete_font_preload() {
        assert!(
            run(r#"const x = <link rel="preload" as="font" type="font/woff2" crossOrigin="anonymous" href="/f.woff2" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn ignores_non_font_preload() {
        assert!(run(r#"const x = <link rel="preload" as="script" href="/a.js" />;"#).is_empty());
    }
}
