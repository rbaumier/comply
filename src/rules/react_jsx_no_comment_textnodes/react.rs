//! react-jsx-no-comment-textnodes backend — comments as JSX text children.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_text"] => |node, source, ctx, diagnostics|
    // jsx_text nodes are text content inside JSX elements
    let Ok(text) = node.utf8_text(source) else { return };
    let trimmed = text.trim();

    // Check if the text looks like a comment
    let is_line_comment = trimmed.starts_with("//") && !trimmed.starts_with("///");
    let is_block_comment = trimmed.starts_with("/*") && trimmed.ends_with("*/");

    if !is_line_comment && !is_block_comment {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-jsx-no-comment-textnodes".into(),
        message: "Comment as JSX text child will be rendered as \
                  visible text. Use `{/* comment */}` instead."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_line_comment_in_jsx() {
        let src = r#"
function App() {
    return (
        <div>
            // this is a comment
            <span>hello</span>
        </div>
    );
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_block_comment_in_jsx() {
        let src = r#"
function App() {
    return (
        <div>
            /* this is a comment */
            <span>hello</span>
        </div>
    );
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_proper_jsx_comment() {
        let src = r#"
function App() {
    return (
        <div>
            {/* this is a proper comment */}
            <span>hello</span>
        </div>
    );
}
"#;
        assert!(run_on(src).is_empty());
    }
}
