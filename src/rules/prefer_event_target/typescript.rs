use crate::diagnostic::{Diagnostic, Severity};

/// Packages whose `EventEmitter` is a different class — keep them out of the
/// suggestion ("use `EventTarget`") since they don't follow Node's API.
const IGNORED_PACKAGES: &[&str] = &["@angular/core", "eventemitter3"];

/// Walk a node's subtree iteratively, calling `visit` on every descendant
/// (excluding the root node itself). Mirrors `walker::walk_tree` semantics
/// but rooted at an arbitrary node so we can scope the check to the program
/// node only.
fn walk_subtree<'t, F: FnMut(tree_sitter::Node<'t>)>(root: tree_sitter::Node<'t>, mut visit: F) {
    let mut cursor = root.walk();
    if !cursor.goto_first_child() {
        return;
    }
    'outer: loop {
        let n = cursor.node();
        if !n.is_error() && !n.is_missing() {
            visit(n);
            if cursor.goto_first_child() {
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'outer;
            }
            if !cursor.goto_parent() {
                return;
            }
            // Stop when we walk back up to the root.
            if cursor.node().id() == root.id() {
                return;
            }
        }
    }
}

/// Returns true if the file imports the identifier `EventEmitter` from one of
/// the ignored packages — usage of that identifier should not be flagged then.
fn imports_event_emitter_from_ignored(
    program: tree_sitter::Node<'_>,
    source: &[u8],
) -> bool {
    let mut cursor = program.walk();
    for child in program.children(&mut cursor) {
        if child.kind() != "import_statement" {
            continue;
        }
        let Some(source_node) = child.child_by_field_name("source") else {
            continue;
        };
        let Ok(raw) = std::str::from_utf8(&source[source_node.byte_range()]) else {
            continue;
        };
        let spec = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
        if !IGNORED_PACKAGES.contains(&spec) {
            continue;
        }
        let mut has_ee = false;
        walk_subtree(child, |n| {
            if has_ee {
                return;
            }
            if n.kind() == "identifier"
                && let Ok(name) = std::str::from_utf8(&source[n.byte_range()])
                && name == "EventEmitter"
            {
                has_ee = true;
            }
        });
        if has_ee {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Emit only from the `program` node so the per-file import-context check
    // runs once, then walk the subtree to collect violations.
    if node.kind() != "program" {
        return;
    }
    if imports_event_emitter_from_ignored(node, source) {
        return;
    }

    walk_subtree(node, |n| {
        match n.kind() {
            "new_expression" => {
                let Some(constructor) = n.child_by_field_name("constructor") else {
                    return;
                };
                if constructor.kind() != "identifier" {
                    return;
                }
                let Ok(name) = std::str::from_utf8(&source[constructor.byte_range()]) else {
                    return;
                };
                if name == "EventEmitter" {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &n,
                        "prefer-event-target",
                        "Prefer `EventTarget` over `EventEmitter`.".into(),
                        Severity::Warning,
                    ));
                }
            }
            "class_heritage" => {
                // The superclass identifier is wrapped in an `extends_clause`
                // (TS grammar) or sits directly under `class_heritage` (JS).
                // Walk descendants and flag the first `identifier` /
                // `type_identifier` whose exact text is `EventEmitter`.
                walk_subtree(n, |inner| {
                    if inner.kind() != "identifier" && inner.kind() != "type_identifier" {
                        return;
                    }
                    let Ok(name) = std::str::from_utf8(&source[inner.byte_range()]) else {
                        return;
                    };
                    if name == "EventEmitter" {
                        diagnostics.push(Diagnostic::at_node(
                            ctx.path,
                            &inner,
                            "prefer-event-target",
                            "Prefer `EventTarget` over `EventEmitter`.".into(),
                            Severity::Warning,
                        ));
                    }
                });
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_extends_event_emitter() {
        let d = run_ts("class MyEmitter extends EventEmitter {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_new_event_emitter() {
        let d = run_ts("const emitter = new EventEmitter();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_event_target() {
        assert!(run_ts("class MyTarget extends EventTarget {}").is_empty());
    }

    #[test]
    fn allows_import_from_ignored_package() {
        assert!(run_ts(r#"import { EventEmitter } from "eventemitter3";"#).is_empty());
    }

    #[test]
    fn allows_angular_event_emitter() {
        assert!(run_ts(r#"import { EventEmitter } from "@angular/core";"#).is_empty());
    }

    #[test]
    fn does_not_flag_event_emitter_ex() {
        assert!(run_ts("class MyEmitter extends EventEmitterEx {}").is_empty());
    }
}
