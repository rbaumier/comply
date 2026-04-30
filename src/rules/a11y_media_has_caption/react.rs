//! a11y-media-has-caption AST backend.
//!
//! Flags `<video>` and `<audio>` elements without a
//! `<track kind="captions">` child.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

/// Check whether a `jsx_attribute` node has a string value matching `expected`.
fn jsx_attr_has_value(attr: tree_sitter::Node, source: &[u8], expected: &str) -> bool {
    crate::rules::jsx::jsx_attribute_string_value(attr, source) == Some(expected)
}

/// Walk descendants of `parent` looking for `<track kind="captions" />`.
fn has_caption_track(parent: tree_sitter::Node, source: &[u8]) -> bool {
    let mut stack = vec![parent];
    while let Some(current) = stack.pop() {
        let k = current.kind();
        if (k == "jsx_self_closing_element" || k == "jsx_opening_element")
            && let Some(name) = current.child_by_field_name("name")
            && name.utf8_text(source).ok() == Some("track")
        {
            let mut cursor = current.walk();
            for child in current.children(&mut cursor) {
                if jsx_attribute_name(child, source) == Some("kind")
                    && jsx_attr_has_value(child, source, "captions")
                {
                    return true;
                }
            }
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(tag) = name_node.utf8_text(source) else {
        return;
    };
    if tag != "video" && tag != "audio" {
        return;
    }

    // For self-closing <video /> or <audio />, there can be no children.
    if node.kind() == "jsx_self_closing_element" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-media-has-caption".into(),
            message: format!("`<{tag}>` elements must have a `<track kind=\"captions\">` child for accessibility."),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }

    // For opening elements, check the parent jsx_element for track children.
    let Some(parent) = node.parent() else {
        return;
    };
    if parent.kind() != "jsx_element" {
        return;
    }

    if !has_caption_track(parent, source) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-media-has-caption".into(),
            message: format!("`<{tag}>` elements must have a `<track kind=\"captions\">` child for accessibility."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_video_without_track() {
        let d = run(r#"const x = <video src="movie.mp4"></video>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("video"));
    }

    #[test]
    fn allows_video_with_caption_track() {
        let src = r#"const x = <video src="movie.mp4"><track kind="captions" src="captions.vtt" /></video>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_audio_without_track() {
        let d = run(r#"const x = <audio src="song.mp3"></audio>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("audio"));
    }
}
