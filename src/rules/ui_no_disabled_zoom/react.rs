//! Flags `<meta name="viewport" content="...user-scalable=no...">`.

use crate::diagnostic::{Diagnostic, Severity};

fn content_disables_zoom(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    for part in lower.split(',') {
        let trimmed = part.trim();
        if trimmed.starts_with("user-scalable") {
            if let Some((_, v)) = trimmed.split_once('=') {
                let val = v.trim();
                if val == "no" || val == "0" {
                    return true;
                }
            }
        }
        if trimmed.starts_with("maximum-scale") {
            if let Some((_, v)) = trimmed.split_once('=') {
                if let Ok(scale) = v.trim().parse::<f64>() {
                    if scale <= 1.0 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = node.child_by_field_name("name") else { return };
    let tag_text = tag.utf8_text(source).ok().unwrap_or("");
    if tag_text != "meta" {
        return;
    }

    let mut is_viewport = false;
    let mut content_value = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(child, source) else {
            continue;
        };
        match attr_name.as_ref() {
            "name" => {
                if let Some(val) = crate::rules::jsx::jsx_attribute_value(child) {
                    let text = val.utf8_text(source).ok().unwrap_or("");
                    let unquoted = text.trim_matches(|c| c == '\'' || c == '"');
                    if unquoted == "viewport" {
                        is_viewport = true;
                    }
                }
            }
            "content" => {
                if let Some(val) = crate::rules::jsx::jsx_attribute_value(child) {
                    let text = val.utf8_text(source).ok().unwrap_or("");
                    content_value = text.trim_matches(|c| c == '\'' || c == '"').to_string();
                }
            }
            _ => {}
        }
    }

    if is_viewport && content_disables_zoom(&content_value) {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: node.start_position().row + 1,
            column: node.start_position().column + 1,
            rule_id: super::META.id.into(),
            message: "Viewport meta disables pinch-to-zoom — accessibility violation.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_user_scalable_no() {
        assert_eq!(
            run(r#"<meta name="viewport" content="width=device-width, user-scalable=no" />"#).len(),
            1
        );
    }

    #[test]
    fn flags_maximum_scale_1() {
        assert_eq!(
            run(r#"<meta name="viewport" content="width=device-width, maximum-scale=1" />"#).len(),
            1
        );
    }

    #[test]
    fn allows_normal_viewport() {
        assert!(
            run(r#"<meta name="viewport" content="width=device-width, initial-scale=1" />"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_maximum_scale_above_1() {
        assert!(
            run(r#"<meta name="viewport" content="width=device-width, maximum-scale=5" />"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_non_viewport_meta() {
        assert!(run(r#"<meta name="description" content="user-scalable=no" />"#).is_empty());
    }
}
