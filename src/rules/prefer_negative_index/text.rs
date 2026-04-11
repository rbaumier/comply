use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Methods where `.length - N` can be replaced with a negative index.
const METHODS: &[&str] = &["slice", "splice", "toSpliced", "at", "with", "subarray"];

/// Detects patterns like `foo.slice(foo.length - N)` where the `.length`
/// receiver matches the method call receiver.
///
/// Looks for `<ident>.<method>(<ident>.length - ` on a single line.
fn find_length_minus(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    for method in METHODS {
        let needle = format!(".{}(", method);
        let mut start = 0;
        while let Some(pos) = line[start..].find(&needle) {
            let abs = start + pos;
            // Extract the receiver name before `.<method>(`
            if let Some(receiver) = extract_receiver(&line[..abs]) {
                let after_paren = abs + needle.len();
                if check_length_minus_arg(line, after_paren, receiver) {
                    hits.push(abs);
                }
            }
            start = abs + needle.len();
        }
    }
    hits
}

/// Extract the identifier immediately before a position (the receiver of the method call).
/// Handles dotted paths like `foo.bar`.
fn extract_receiver(before: &str) -> Option<&str> {
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    // Walk backwards to find start of identifier (including dots for member access)
    let end = trimmed.len();
    let mut i = end;
    for ch in trimmed.chars().rev() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '.' {
            i -= ch.len_utf8();
        } else {
            break;
        }
    }
    let ident = &trimmed[i..end];
    if ident.is_empty() || ident.starts_with('.') || ident.ends_with('.') {
        return None;
    }
    Some(ident)
}

/// Check if the argument at `start` position in `line` matches `<receiver>.length - `.
fn check_length_minus_arg(line: &str, start: usize, receiver: &str) -> bool {
    let rest = &line[start..];
    let trimmed = rest.trim_start();
    // Must start with `<receiver>.length`
    if let Some(after) = trimmed.strip_prefix(receiver)
        && let Some(after_len) = after.strip_prefix(".length") {
            let after_len = after_len.trim_start();
            // Followed by `- ` (subtraction)
            return after_len.starts_with('-');
        }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_length_minus(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-negative-index".into(),
                    message: "Prefer negative index over `.length - index`.".into(),
                    severity: Severity::Warning,
                });
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
    fn flags_slice_length_minus() {
        let d = run("const x = str.slice(str.length - 3);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_splice_length_minus() {
        let d = run("arr.splice(arr.length - 1, 1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_at_length_minus() {
        let d = run("const last = arr.at(arr.length - 1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_negative_index() {
        assert!(run("const x = str.slice(-3);").is_empty());
    }

    #[test]
    fn allows_different_receiver() {
        // Different receiver — not a match
        assert!(run("const x = str.slice(other.length - 3);").is_empty());
    }

    #[test]
    fn allows_normal_slice() {
        assert!(run("const x = str.slice(0, 5);").is_empty());
    }
}
