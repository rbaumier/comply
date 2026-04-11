use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Check;

/// Extract inline type annotation from a parameter line — the text after `:` up to `,` or `)`.
/// Returns Some(annotation) if it's a multi-type annotation (contains `|` or `{`).
fn extract_complex_annotation(line: &str) -> Vec<String> {
    let mut results = Vec::new();
    let trimmed = line.trim();

    // Look for parameter annotations in function signatures
    // Find all `: Type` patterns
    let mut search_start = 0;
    while let Some(colon_pos) = trimmed[search_start..].find(": ") {
        let abs_colon = search_start + colon_pos + 2;
        if abs_colon >= trimmed.len() {
            break;
        }
        let rest = &trimmed[abs_colon..];

        // Find the end of this type annotation (comma, closing paren, opening brace, or end)
        let mut depth = 0;
        let mut end = rest.len();
        for (i, ch) in rest.char_indices() {
            match ch {
                '<' | '(' | '{' => depth += 1,
                '>' | ')' | '}' => {
                    if depth == 0 {
                        end = i;
                        break;
                    }
                    depth -= 1;
                }
                ',' if depth == 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }

        let annotation = rest[..end].trim();

        // Must be complex: contains `|`, `&`, or is an object type `{`
        if (annotation.contains(" | ") || annotation.contains(" & "))
            && annotation.len() > 5
        {
            results.push(annotation.to_string());
        }

        search_start = abs_colon + end;
    }

    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Collect all complex annotations and their line numbers
        let mut annotation_lines: HashMap<String, Vec<usize>> = HashMap::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            for annotation in extract_complex_annotation(line) {
                annotation_lines
                    .entry(annotation)
                    .or_default()
                    .push(idx + 1);
            }
        }

        let mut diagnostics = Vec::new();
        for (annotation, lines) in &annotation_lines {
            if lines.len() >= 2 {
                for &line_num in lines {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: line_num,
                        column: 1,
                        rule_id: "use-type-alias".into(),
                        message: format!(
                            "Inline type `{}` appears {} times — extract a type alias.",
                            annotation,
                            lines.len()
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
        }

        // Sort by line number for deterministic output
        diagnostics.sort_by_key(|d| d.line);
        diagnostics
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
    fn flags_repeated_union_annotation() {
        let src = r#"
function foo(x: string | number) {}
function bar(y: string | number) {}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_repeated_intersection_annotation() {
        let src = r#"
function foo(x: Foo & Bar) {}
function bar(y: Foo & Bar) {}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_unique_annotations() {
        let src = r#"
function foo(x: string | number) {}
function bar(y: boolean | null) {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_simple_annotations() {
        let src = r#"
function foo(x: string) {}
function bar(y: string) {}
"#;
        assert!(run(src).is_empty());
    }
}
