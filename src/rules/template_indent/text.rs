//! template-indent backend — detect template literals with uniform leading
//! whitespace that could be stripped.
//!
//! Simplified version: scans for backtick-delimited template literals and
//! checks if all non-empty content lines share a common leading whitespace
//! prefix of 4+ spaces (indicating inherited code indentation).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Minimum common whitespace prefix (in spaces) to flag.
const MIN_INDENT: usize = 4;

/// Find template literals and check their indentation.
/// This is a simplified text-based approach: we find backtick pairs and
/// analyze the content lines between them.
fn find_indented_templates(source: &str) -> Vec<(usize, usize)> {
    let mut results = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        // Look for a line containing a backtick that opens a template
        let line = lines[i];
        if let Some(bt_pos) = line.find('`') {
            // Check this isn't a single-line template (closing backtick on same line after opening)
            let after_bt = &line[bt_pos + 1..];
            if after_bt.contains('`') {
                // Single-line template — skip
                i += 1;
                continue;
            }

            // Collect lines until closing backtick
            let start_line = i;
            let mut template_lines = Vec::new();
            let mut j = i + 1;
            let mut found_close = false;
            while j < lines.len() {
                if lines[j].contains('`') {
                    found_close = true;
                    // Include content before the closing backtick
                    let close_pos = lines[j].find('`').unwrap();
                    let before_close = &lines[j][..close_pos];
                    if !before_close.trim().is_empty() {
                        template_lines.push(before_close);
                    }
                    break;
                }
                template_lines.push(lines[j]);
                j += 1;
            }

            if found_close && template_lines.len() >= 2 {
                // Check if all non-empty lines share a common leading whitespace
                let min_indent = common_leading_whitespace(&template_lines);
                if min_indent >= MIN_INDENT {
                    results.push((start_line + 1, min_indent)); // 1-indexed
                }
            }
            i = if found_close { j + 1 } else { j };
        } else {
            i += 1;
        }
    }
    results
}

/// Compute the minimum leading whitespace across all non-empty lines.
fn common_leading_whitespace(lines: &[&str]) -> usize {
    let mut min_ws = usize::MAX;
    let mut has_content = false;

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        has_content = true;
        let leading = line.len() - line.trim_start().len();
        min_ws = min_ws.min(leading);
    }

    if has_content {
        min_ws
    } else {
        0
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        find_indented_templates(ctx.source)
            .into_iter()
            .map(|(line, indent)| Diagnostic {
                path: ctx.path.to_path_buf(),
                line,
                column: 1,
                rule_id: "template-indent".into(),
                message: format!(
                    "Template literal has {indent} spaces of common leading indentation \
                     inherited from the surrounding code — strip it or use a dedent helper."
                ),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_indented_template() {
        let src = r#"
function foo() {
    const html = `
        <div>
            <p>Hello</p>
        </div>
    `;
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("common leading indentation"));
    }

    #[test]
    fn allows_template_without_excess_indent() {
        let src = r#"
const html = `
<div>
  <p>Hello</p>
</div>
`;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_line_template() {
        assert!(run("const x = `hello world`;").is_empty());
    }

    #[test]
    fn allows_template_with_minimal_indent() {
        let src = "const x = `\n  a\n  b\n`;\n";
        // 2 spaces is below the MIN_INDENT threshold
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_deeply_nested_template() {
        let src = r#"
if (true) {
    if (true) {
        const sql = `
            SELECT *
            FROM users
            WHERE id = 1
        `;
    }
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
