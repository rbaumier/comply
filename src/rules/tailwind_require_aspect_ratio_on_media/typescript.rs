//! AstCheck on JSX elements: `<img>` / `<video>` must have an `aspect-*`
//! class (in `className`) or both `width` and `height` attributes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value, jsx_element_tag_name};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] =>
    |node, source, ctx, diagnostics|
    let Some(tag) = jsx_element_tag_name(node, source) else { return; };
    if tag != "img" && tag != "video" { return; }

    let mut has_width = false;
    let mut has_height = false;
    let mut has_aspect_class = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = jsx_attribute_name(child, source) else { continue; };
        match name {
            "width" => has_width = true,
            "height" => has_height = true,
            "className" | "class" => {
                if let Some(value) = jsx_attribute_string_value(child, source)
                    && value.split_whitespace().any(|c| c.starts_with("aspect-"))
                {
                    has_aspect_class = true;
                }
            }
            _ => {}
        }
    }

    if has_aspect_class || (has_width && has_height) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`<{tag}>` lacks aspect ratio — add a Tailwind `aspect-*` class or both `width` and `height` to prevent layout shift."
        ),
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

    fn run(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_img_without_aspect_or_dims() {
        assert_eq!(run(r#"const x = <img src="/a.png" />;"#).len(), 1);
    }

    #[test]
    fn flags_video_without_aspect_or_dims() {
        assert_eq!(run(r#"const x = <video src="/a.mp4" />;"#).len(), 1);
    }

    #[test]
    fn allows_img_with_aspect_class() {
        assert!(run(r#"const x = <img src="/a.png" className="aspect-video" />;"#).is_empty());
    }

    #[test]
    fn allows_img_with_width_and_height() {
        assert!(run(r#"const x = <img src="/a.png" width={200} height={100} />;"#).is_empty());
    }

    #[test]
    fn flags_img_with_only_width() {
        assert_eq!(
            run(r#"const x = <img src="/a.png" width={200} />;"#).len(),
            1
        );
    }
}
