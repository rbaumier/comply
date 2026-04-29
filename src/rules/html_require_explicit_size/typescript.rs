//! html-require-explicit-size AST backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["img", "video"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "img" && tag != "video" {
        return;
    }

    let mut cursor = node.walk();
    let mut has_width = false;
    let mut has_height = false;
    for child in node.children(&mut cursor) {
        match crate::rules::jsx::jsx_attribute_name(child, source) {
            Some("width") => has_width = true,
            Some("height") => has_height = true,
            _ => {}
        }
    }
    if has_width && has_height {
        return;
    }

    let missing = match (has_width, has_height) {
        (false, false) => "`width` and `height`",
        (false, true) => "`width`",
        (true, false) => "`height`",
        _ => unreachable!(),
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "html-require-explicit-size".into(),
        message: format!("`<{tag}>` is missing {missing} — causes layout shift."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_img_without_size() {
        assert_eq!(run(r#"const x = <img src="x.png" />;"#).len(), 1);
    }

    #[test]
    fn flags_img_with_only_width() {
        let d = run(r#"const x = <img src="x.png" width={100} />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("height"));
    }

    #[test]
    fn flags_video_without_size() {
        assert_eq!(run(r#"const x = <video src="x.mp4" />;"#).len(), 1);
    }

    #[test]
    fn allows_img_with_both() {
        assert!(run(r#"const x = <img src="x.png" width={100} height={100} />;"#).is_empty());
    }

    #[test]
    fn allows_video_with_both() {
        assert!(run(r#"const x = <video width="320" height="240" />;"#).is_empty());
    }

    #[test]
    fn ignores_non_media() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
