//! dockerfile-require-non-root-user tree-sitter backend.
//!
//! Walks `from_instruction` boundaries to track stages. The final stage must
//! end with a `USER` instruction whose argument is neither root nor uid 0.
//! Containers running as root expand the blast radius of any RCE.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    let mut saw_from = false;
    let mut last_user: Option<String> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "from_instruction" => {
                saw_from = true;
                // New stage resets the active user.
                last_user = None;
            }
            "user_instruction" => {
                let mut uc = child.walk();
                let arg = child
                    .children(&mut uc)
                    .find(|n| n.kind() == "unquoted_string")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .trim();
                last_user = Some(arg.to_string());
            }
            _ => {}
        }
    }
    if !saw_from {
        return;
    }
    let flagged = match last_user.as_deref() {
        None => true,
        Some(u) => u == "root" || u == "0" || u.starts_with("root:") || u.starts_with("0:"),
    };
    if flagged {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Dockerfile must drop to a non-root USER before CMD.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_missing_user() {
        let src = "FROM node:22.12\nCMD [\"node\", \"a.js\"]\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_user_root() {
        let src = "FROM node:22.12\nUSER root\nCMD [\"node\"]\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_root_user() {
        let src = "FROM node:22.12\nUSER node\nCMD [\"node\"]\n";
        assert!(run(src).is_empty());
    }
}
