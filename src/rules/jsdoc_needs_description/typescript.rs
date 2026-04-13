//! jsdoc-needs-description backend — flag JSDoc blocks that have tags but no description.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "comment" {
        return;
    }
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !text.starts_with("/**") {
        return;
    }

    let mut has_tag = false;
    let mut has_description = false;

    for line in text.lines() {
        let trimmed = line.trim();
        let content = trimmed
            .trim_start_matches("/**")
            .trim_start_matches("*/")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();

        if content.is_empty() || content == "/" {
            continue;
        }

        if content.starts_with('@') {
            has_tag = true;
        } else {
            has_description = true;
        }
    }

    if has_tag && !has_description {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "jsdoc-needs-description".into(),
            message: "JSDoc block contains only tags — add a prose description explaining what this does and why.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_tags_only_jsdoc() {
        let source = r#"
/**
 * @param x - the input
 * @returns the output
 */
function foo(x: number): number { return x; }
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("only tags"));
    }

    #[test]
    fn flags_single_line_tag_only() {
        let source = "/** @deprecated */\nfunction old() {}";
        let d = run_on(source);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_jsdoc_with_description() {
        let source = r#"
/**
 * Computes the square of a number.
 * @param x - the input
 * @returns the squared value
 */
function square(x: number): number { return x * x; }
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_jsdoc_with_description_only() {
        let source = r#"
/**
 * This function does something important.
 */
function important() {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_empty_jsdoc() {
        let source = r#"
/**
 */
function foo() {}
"#;
        assert!(run_on(source).is_empty());
    }
}
