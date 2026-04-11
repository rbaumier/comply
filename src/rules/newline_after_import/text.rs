use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("import ") || trimmed.starts_with("import{")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        if lines.is_empty() {
            return Vec::new();
        }

        // Find the last import line index.
        let mut last_import_idx: Option<usize> = None;
        for (idx, line) in lines.iter().enumerate() {
            if is_import_line(line) {
                last_import_idx = Some(idx);
            }
        }

        let last_import_idx = match last_import_idx {
            Some(idx) => idx,
            None => return Vec::new(), // no imports
        };

        // Find the next non-empty line after the last import.
        let next_code_idx = lines
            .iter()
            .enumerate()
            .skip(last_import_idx + 1)
            .find(|(_, l)| !l.trim().is_empty())
            .map(|(i, _)| i);

        if let Some(next_idx) = next_code_idx {
            // If the next non-empty line is an import, skip (it's not the last import).
            if is_import_line(lines[next_idx]) {
                return Vec::new();
            }
            // Check if there's at least one blank line between last import and next code.
            if next_idx == last_import_idx + 1 {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: last_import_idx + 1,
                    column: 1,
                    rule_id: "newline-after-import".into(),
                    message: "Expected a blank line after the last import statement.".into(),
                    severity: Severity::Warning,
                }];
            }
        }

        Vec::new()
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
    fn flags_missing_newline() {
        let src = r#"import { a } from 'a';
const x = 1;
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
    }

    #[test]
    fn allows_blank_line_after_import() {
        let src = r#"import { a } from 'a';

const x = 1;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_consecutive_imports_without_blank() {
        let src = r#"import { a } from 'a';
import { b } from 'b';

const x = 1;
"#;
        assert!(run(src).is_empty());
    }
}
