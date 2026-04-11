//! prefer-single-call — flag consecutive `.push()` / `.classList.add()` / `.classList.remove()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Extract a "receiver.method" key from a trimmed line.
///
/// Matches patterns like:
///   `arr.push(`         -> Some("arr.push")
///   `el.classList.add(`  -> Some("el.classList.add")
///   `el.classList.remove(` -> Some("el.classList.remove")
fn extract_call_key(trimmed: &str) -> Option<String> {
    // Try classList.add / classList.remove first (longer pattern)
    for method in [".classList.add(", ".classList.remove("] {
        if let Some(pos) = trimmed.find(method) {
            if pos == 0 {
                continue;
            }
            let receiver = &trimmed[..pos];
            if receiver.bytes().all(|b| is_ident_char(b) || b == b'.') && !receiver.is_empty() {
                // key without trailing '('
                return Some(format!("{}{}", receiver, &method[..method.len() - 1]));
            }
        }
    }

    // Try .push(
    if let Some(pos) = trimmed.find(".push(") {
        if pos == 0 {
            return None;
        }
        let receiver = &trimmed[..pos];
        if receiver.bytes().all(|b| is_ident_char(b) || b == b'.') && !receiver.is_empty() {
            return Some(format!("{receiver}.push"));
        }
    }

    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        let mut prev_key: Option<String> = None;
        let mut prev_line_idx: usize = 0;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if let Some(key) = extract_call_key(trimmed) {
                if let Some(ref pk) = prev_key
                    && *pk == key && idx == prev_line_idx + 1 {
                        let col = line.find(trimmed).unwrap_or(0);
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: col + 1,
                            rule_id: "prefer-single-call".into(),
                            message: format!("Combine consecutive `{key}()` calls into one."),
                            severity: Severity::Warning,
                        });
                    }
                prev_key = Some(key);
                prev_line_idx = idx;
            } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
                prev_key = None;
            }
        }
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
    fn flags_consecutive_push() {
        let d = run("arr.push(1);\narr.push(2);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("arr.push"));
    }

    #[test]
    fn flags_three_consecutive_push() {
        let d = run("arr.push(1);\narr.push(2);\narr.push(3);");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn flags_classlist_add() {
        let d = run("el.classList.add('a');\nel.classList.add('b');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("classList.add"));
    }

    #[test]
    fn flags_classlist_remove() {
        let d = run("el.classList.remove('a');\nel.classList.remove('b');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_push() {
        assert!(run("arr.push(1);").is_empty());
    }

    #[test]
    fn allows_different_receivers() {
        assert!(run("arr1.push(1);\narr2.push(2);").is_empty());
    }

    #[test]
    fn allows_non_consecutive() {
        assert!(run("arr.push(1);\nconsole.log('x');\narr.push(2);").is_empty());
    }
}
