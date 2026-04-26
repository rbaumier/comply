//! symmetric-pairs backend — walk `export_statement` nodes for
//! `function_declaration` children and flag missing symmetric counterparts.
//!
//! Detection: collect all exported function names, then check each against
//! known prefix pairs (get/set, add/remove, open/close, start/stop,
//! create/delete).

use crate::diagnostic::{Diagnostic, Severity};

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

const PREFIXES: &[&str] = &[
    "get", "set", "add", "remove", "open", "close", "start", "stop", "create", "delete", "destroy",
];

/// Split a function name into (prefix, suffix) if it matches a known prefix.
fn split_prefix(name: &str) -> Option<(&str, &str)> {
    for &pfx in PREFIXES {
        if name.len() > pfx.len() && name.starts_with(pfx) {
            let rest = &name[pfx.len()..];
            if rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                return Some((pfx, rest));
            }
        }
    }
    None
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Run once at the program root — collect all exported fn names, then check.
    let mut exports: Vec<(usize, String)> = Vec::new();
    let mut cursor = node.walk();

    for child in node.named_children(&mut cursor) {
        if child.kind() != "export_statement" {
            continue;
        }
        // Look for function_declaration inside the export
        let mut inner_cursor = child.walk();
        for inner in child.named_children(&mut inner_cursor) {
            let kind = inner.kind();
            if kind != "function_declaration" && kind != "function" {
                continue;
            }
            let Some(name_node) = inner.child_by_field_name("name") else {
                continue;
            };
            let name_text = &source[name_node.byte_range()];
            let Ok(name) = std::str::from_utf8(name_text) else {
                continue;
            };
            exports.push((name_node.start_position().row + 1, name.to_string()));
        }
    }

    let names: Vec<&str> = exports.iter().map(|(_, n)| n.as_str()).collect();

    for (line_num, name) in &exports {
        if let Some((prefix, suffix)) = split_prefix(name) {
            let counterparts: Vec<&str> = PAIRS
                .iter()
                .filter(|(p, _)| *p == prefix)
                .map(|(_, c)| *c)
                .collect();

            let has_pair = counterparts.iter().any(|cp| {
                let expected = format!("{cp}{suffix}");
                names.contains(&expected.as_str())
            });

            if !has_pair {
                let expected_names: Vec<String> = counterparts
                    .iter()
                    .map(|cp| format!("{cp}{suffix}"))
                    .collect();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: *line_num,
                    column: 1,
                    rule_id: "symmetric-pairs".into(),
                    message: format!(
                        "`export function {name}` has no symmetric counterpart — expected {}.",
                        expected_names.join(" or "),
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_get_without_set() {
        let d = run_on("export function getFoo() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setFoo"));
    }

    #[test]
    fn allows_get_with_set() {
        let src = "export function getFoo() {}\nexport function setFoo() {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_open_without_close() {
        let d = run_on("export function openConnection() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("closeConnection"));
    }

    #[test]
    fn flags_create_without_delete_or_destroy() {
        let d = run_on("export function createUser() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("deleteUser") || d[0].message.contains("destroyUser"));
    }

    #[test]
    fn allows_create_with_destroy() {
        let src = "export function createUser() {}\nexport function destroyUser() {}";
        let d = run_on(src);
        assert!(!d.iter().any(|d| d.message.contains("createUser")));
    }

    #[test]
    fn ignores_non_exported_functions() {
        assert!(run_on("function getFoo() {}").is_empty());
    }
}
