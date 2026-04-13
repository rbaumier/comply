//! a11y-no-distracting-elements AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const DISTRACTING: &[&str] = &["marquee", "blink"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_opening_element" && node.kind() != "jsx_self_closing_element" {
        return;
    }

    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if DISTRACTING.contains(&tag) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-no-distracting-elements".into(),
            message: format!("Do not use `<{tag}>`. It is deprecated and distracting."),
            severity: Severity::Error,
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
    fn flags_marquee() {
        let d = run(r#"const x = <marquee>scrolling text</marquee>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("marquee"));
    }

    #[test]
    fn flags_blink() {
        let d = run(r#"const x = <blink>blinking text</blink>;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("blink"));
    }

    #[test]
    fn allows_normal_elements() {
        assert!(run(r#"const x = <div>hello</div>;"#).is_empty());
    }

    #[test]
    fn flags_self_closing_marquee() {
        let d = run(r#"const x = <marquee />;"#);
        assert_eq!(d.len(), 1);
    }
}
