//! unused-enum-member backend — flag TypeScript enum members declared in
//! the current file but never referenced anywhere within that file.
//!
//! Scope is intentionally file-local. Cross-file usage is out of scope here:
//! exported enums whose *enum identifier* is never imported are already
//! covered by the dead-export pipeline. The actionable case this rule
//! catches is members that are declared and forgotten — typically inside
//! file-local enums or when an enum's surface drifts from its callers.
//!
//! Algorithm:
//! 1. Walk the program once, collecting every `enum_declaration` and its
//!    member names + declaration lines.
//! 2. Walk the program a second time, recording every
//!    `EnumName.MemberName` access via `member_expression` / TS's
//!    `property_access_expression` shape, skipping subtrees rooted at an
//!    `enum_declaration` (the body's identifiers are declarations, not
//!    usages).
//! 3. Diff: any member with zero recorded accesses produces a diagnostic.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::{FxHashMap, FxHashSet};

fn text_of<'a>(node: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("")
}

fn collect_enums(
    root: tree_sitter::Node,
    source: &[u8],
    out: &mut FxHashMap<String, Vec<(String, usize)>>,
) {
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current.kind() == "enum_declaration" {
            // Exported enums are consumed cross-file; their members
            // aren't expected to be referenced locally.
            if let Some(parent) = current.parent() {
                if parent.kind() == "export_statement" {
                    continue;
                }
            }
            if text_of(current, source).starts_with("export ") {
                continue;
            }
            if let Some(name_node) = current.child_by_field_name("name") {
                let enum_name = text_of(name_node, source).to_string();
                if let Some(body) = current.child_by_field_name("body") {
                    let mut members = Vec::new();
                    let mut c = body.walk();
                    for child in body.named_children(&mut c) {
                        if child.kind() != "enum_assignment"
                            && child.kind() != "property_identifier"
                        {
                            // Some grammars expose enum members as
                            // `enum_assignment` (with an initializer) or
                            // bare `property_identifier` (no initializer).
                            // Other shapes are ignored.
                            continue;
                        }
                        let name_node = if child.kind() == "enum_assignment" {
                            child
                                .child_by_field_name("name")
                                .or_else(|| child.named_child(0))
                        } else {
                            Some(child)
                        };
                        let Some(n) = name_node else { continue };
                        let member_name = text_of(n, source).to_string();
                        if member_name.is_empty() {
                            continue;
                        }
                        let line = child.start_position().row + 1;
                        members.push((member_name, line));
                    }
                    if !members.is_empty() {
                        out.insert(enum_name, members);
                    }
                }
            }
            // Don't recurse into the enum body — its identifiers are
            // declarations, not usages.
            continue;
        }
        let mut c = current.walk();
        for child in current.named_children(&mut c) {
            stack.push(child);
        }
    }
}

fn collect_usages(
    root: tree_sitter::Node,
    source: &[u8],
    enums: &FxHashMap<String, Vec<(String, usize)>>,
    used: &mut FxHashSet<(String, String)>,
) {
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        // Skip enum declaration subtrees entirely — references inside the
        // declaration body are part of the definition.
        if current.kind() == "enum_declaration" {
            continue;
        }

        let kind = current.kind();
        if kind == "member_expression" || kind == "property_access_expression" {
            let obj = current.child_by_field_name("object");
            let prop = current.child_by_field_name("property");
            if let (Some(obj), Some(prop)) = (obj, prop) {
                let obj_name = text_of(obj, source);
                if enums.contains_key(obj_name) {
                    let prop_name = text_of(prop, source);
                    used.insert((obj_name.to_string(), prop_name.to_string()));
                }
            }
        }

        let mut c = current.walk();
        for child in current.named_children(&mut c) {
            stack.push(child);
        }
    }
}

crate::ast_check! { on ["program"] prefilter = ["enum"] => |node, source, ctx, diagnostics|
    let mut enums: FxHashMap<String, Vec<(String, usize)>> = FxHashMap::default();
    collect_enums(node, source, &mut enums);

    if enums.is_empty() {
        return;
    }

    let mut used: FxHashSet<(String, String)> = FxHashSet::default();
    collect_usages(node, source, &enums, &mut used);

    for (enum_name, members) in &enums {
        for (member_name, line) in members {
            if !used.contains(&(enum_name.clone(), member_name.clone())) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: *line,
                    column: 1,
                    rule_id: "unused-enum-member".into(),
                    message: format!(
                        "enum member `{enum_name}.{member_name}` is never referenced in this file."
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
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_unused_enum_member() {
        let source = r#"
enum Color {
    Red,
    Green,
    Blue,
}
const x = Color.Red;
const y = Color.Green;
"#;
        let diags = run_ts(source, &Check);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Blue"));
    }

    #[test]
    fn allows_all_used_members() {
        let source = r#"
enum Status {
    Active,
    Inactive,
}
const a = Status.Active;
const b = Status.Inactive;
"#;
        let diags = run_ts(source, &Check);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_no_enums() {
        let source = "const x = 1;";
        let diags = run_ts(source, &Check);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_all_unused_in_unused_enum() {
        let source = r#"
enum Direction {
    Up,
    Down,
    Left,
    Right,
}
"#;
        let diags = run_ts(source, &Check);
        assert_eq!(diags.len(), 4);
    }

    #[test]
    fn skips_exported_enums() {
        let source = r#"
export enum CastState {
    IDLE,
    PLAYING,
    PAUSED,
}
"#;
        assert!(run_ts(source, &Check).is_empty());
    }

    #[test]
    fn handles_initialized_members() {
        let source = r#"
enum Code {
    Ok = 200,
    NotFound = 404,
    Teapot = 418,
}
const a = Code.Ok;
const b = Code.NotFound;
"#;
        let diags = run_ts(source, &Check);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Teapot"));
    }
}
