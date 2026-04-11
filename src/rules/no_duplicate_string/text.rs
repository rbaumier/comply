use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Check;

const MIN_STRING_LEN: usize = 10;
const THRESHOLD: usize = 3;

/// Extract all quoted strings (single or double) from a line.
fn extract_strings(line: &str) -> Vec<String> {
    let mut results = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = i + 1;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    let s: String = chars[start..i].iter().collect();
                    if s.len() >= MIN_STRING_LEN {
                        results.push(s);
                    }
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // First pass: count occurrences and record all line numbers.
        let mut occurrences: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for s in extract_strings(line) {
                occurrences.entry(s).or_default().push(idx + 1);
            }
        }

        // Second pass: flag the 3rd+ occurrence of each string.
        let mut diagnostics = Vec::new();
        for (s, lines) in &occurrences {
            if lines.len() >= THRESHOLD {
                for &line_num in &lines[THRESHOLD - 1..] {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: line_num,
                        column: 1,
                        rule_id: "no-duplicate-string".into(),
                        message: format!(
                            "String `\"{}\"` appears {} times — extract to a constant.",
                            s,
                            lines.len()
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
        }
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
    fn flags_string_appearing_three_times() {
        let src = r#"
const a = "hello world";
const b = "hello world";
const c = "hello world";
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3 times"));
    }

    #[test]
    fn flags_fourth_occurrence_too() {
        let src = r#"
const a = "repeated str";
const b = "repeated str";
const c = "repeated str";
const d = "repeated str";
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn ignores_short_strings() {
        let src = r#"
const a = "short";
const b = "short";
const c = "short";
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_two_occurrences() {
        let src = r#"
const a = "long enough string";
const b = "long enough string";
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_single_quotes() {
        let src = r#"
const a = 'single quote str';
const b = 'single quote str';
const c = 'single quote str';
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
