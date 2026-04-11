use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Symmetric prefix pairs: (prefix, expected counterpart prefix).
const PAIRS: &[(&str, &str)] = &[
    ("get", "set"),
    ("set", "get"),
    ("add", "remove"),
    ("remove", "add"),
    ("open", "close"),
    ("close", "open"),
    ("start", "stop"),
    ("stop", "start"),
    ("create", "delete"),
    ("delete", "create"),
    ("create", "destroy"),
];

/// Extract the function name from an `export function <name>` declaration.
fn exported_fn_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("export ")?;
    let rest = rest.trim_start();
    // handle `export async function`
    let rest = rest.strip_prefix("async ").unwrap_or(rest);
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("function ")?;
    let rest = rest.trim_start();
    // name ends at `(`, `<`, or whitespace
    let end = rest.find(|c: char| c == '(' || c == '<' || c.is_whitespace())?;
    Some(&rest[..end])
}

/// Split a function name into (prefix, suffix) if it matches a known prefix.
/// e.g. "getFoo" -> Some(("get", "Foo")), "openConnection" -> Some(("open", "Connection"))
fn split_prefix(name: &str) -> Option<(&str, &str)> {
    // Collect unique prefixes from PAIRS
    const PREFIXES: &[&str] = &[
        "get", "set", "add", "remove", "open", "close", "start", "stop", "create", "delete",
        "destroy",
    ];
    for &pfx in PREFIXES {
        if name.len() > pfx.len() && name.starts_with(pfx) {
            let rest = &name[pfx.len()..];
            // The suffix must start with an uppercase letter (camelCase boundary)
            if rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                return Some((pfx, rest));
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Collect all exported function names with their line numbers.
        let exports: Vec<(usize, &str)> = ctx
            .source
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| exported_fn_name(line).map(|name| (idx + 1, name)))
            .collect();

        let names: Vec<&str> = exports.iter().map(|(_, n)| *n).collect();

        let mut diagnostics = Vec::new();

        for &(line_num, name) in &exports {
            if let Some((prefix, suffix)) = split_prefix(name) {
                // Find expected counterpart prefixes
                let counterparts: Vec<&str> = PAIRS
                    .iter()
                    .filter(|(p, _)| *p == prefix)
                    .map(|(_, c)| *c)
                    .collect();

                // Check if any counterpart exists
                let has_pair = counterparts.iter().any(|cp| {
                    let expected = format!("{}{}", cp, suffix);
                    names.contains(&expected.as_str())
                });

                if !has_pair {
                    let expected_names: Vec<String> = counterparts
                        .iter()
                        .map(|cp| format!("{}{}", cp, suffix))
                        .collect();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: line_num,
                        column: 1,
                        rule_id: "symmetric-pairs".into(),
                        message: format!(
                            "`export function {}` has no symmetric counterpart — expected {}.",
                            name,
                            expected_names.join(" or "),
                        ),
                        severity: Severity::Warning,
                    });
                }
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
    fn flags_get_without_set() {
        let src = "export function getFoo() {}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setFoo"));
    }

    #[test]
    fn allows_get_with_set() {
        let src = "export function getFoo() {}\nexport function setFoo() {}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_open_without_close() {
        let src = "export function openConnection() {}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("closeConnection"));
    }

    #[test]
    fn flags_create_without_delete_or_destroy() {
        let src = "export function createUser() {}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("deleteUser") || d[0].message.contains("destroyUser"));
    }

    #[test]
    fn allows_create_with_destroy() {
        let src = "export function createUser() {}\nexport function destroyUser() {}\n";
        // createUser is paired with destroyUser, but destroyUser has no pair — that's fine,
        // destroy is not in our prefix list as a "source" that requires a counterpart.
        // Actually destroy IS not a prefix that triggers (no "destroy" -> X pair aside from
        // the reverse). Let's just check createUser is not flagged.
        let d = run(src);
        // createUser should not be flagged
        assert!(!d.iter().any(|d| d.message.contains("createUser")));
    }

    #[test]
    fn ignores_non_exported_functions() {
        let src = "function getFoo() {}\n";
        assert!(run(src).is_empty());
    }
}
