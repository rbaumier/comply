use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract the assignment target from bracket-notation: `arr[0] = ...` -> `arr[0]`
fn bracket_target(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let bracket_end = trimmed.find(']')?;
    let _bracket_start = trimmed[..bracket_end].find('[')?;
    // Must have ` = ` after the `]`
    let after = trimmed[bracket_end + 1..].trim_start();
    if after.starts_with('=') && !after.starts_with("==") {
        Some(&trimmed[..bracket_end + 1])
    } else {
        None
    }
}

/// Extract the key from `.set("key", ...)` -> `("key"` portion as `<receiver>.set(<key>`
fn map_set_target(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let pos = trimmed.find(".set(")?;
    let receiver = trimmed[..pos].trim();
    let args_start = pos + 5; // skip ".set("
    let rest = &trimmed[args_start..];
    // Find the first comma to isolate the key argument
    let comma = rest.find(',')?;
    let key = rest[..comma].trim();
    Some(format!("{}.set({})", receiver, key))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for i in 0..lines.len().saturating_sub(1) {
            // Check bracket notation: arr[x] = ... ; arr[x] = ...
            if let (Some(t1), Some(t2)) = (bracket_target(lines[i]), bracket_target(lines[i + 1]))
                && t1 == t2 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 2, // second assignment
                        column: 1,
                        rule_id: "no-element-overwrite".into(),
                        message: format!("`{}` is assigned on the previous line and immediately overwritten.", t1),
                        severity: Severity::Error,
                    });
                    continue;
                }
            // Check .set() calls
            if let (Some(t1), Some(t2)) = (map_set_target(lines[i]), map_set_target(lines[i + 1]))
                && t1 == t2 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 2,
                        column: 1,
                        rule_id: "no-element-overwrite".into(),
                        message: "`.set()` with the same key on the previous line — first write is dead.".into(),
                        severity: Severity::Error,
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
    fn flags_consecutive_bracket_writes() {
        let src = "arr[0] = 1;\narr[0] = 2;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_consecutive_map_set() {
        let src = "map.set(\"key\", 1);\nmap.set(\"key\", 2);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_different_indices() {
        let src = "arr[0] = 1;\narr[1] = 2;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_different_keys() {
        let src = "map.set(\"a\", 1);\nmap.set(\"b\", 2);";
        assert!(run(src).is_empty());
    }
}
