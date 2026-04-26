//! dockerfile-no-secrets-in-arg tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

const SECRET_SUBSTRINGS: &[&str] = &["SECRET", "TOKEN", "PASSWORD", "PASSWD", "APIKEY"];

crate::ast_check! { on ["arg_instruction"] => |node, source, ctx, diagnostics|
    // arg_instruction children: ARG, unquoted_string (name), optional `=`, optional unquoted_string (value).
    let mut name: Option<&str> = None;
    let mut saw_eq = false;
    let mut value: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        match child.kind() {
            "unquoted_string" => {
                let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
                if name.is_none() {
                    name = Some(text);
                } else if saw_eq {
                    value = Some(text);
                }
            }
            "=" => saw_eq = true,
            _ => {}
        }
    }
    let Some(key) = name else { return; };
    if !saw_eq { return; }
    let v = value.unwrap_or("");
    if v.is_empty() { return; }
    if !is_secret_name(key) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!("ARG `{key}` has a secret-like default; use `--mount=type=secret` instead."),
        severity: Severity::Error,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_SUBSTRINGS.iter().any(|m| upper.contains(m)) || upper.ends_with("_KEY")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_arg_with_secret_default() {
        assert_eq!(run("ARG NPM_TOKEN=abcdef\n").len(), 1);
    }

    #[test]
    fn allows_arg_without_default() {
        assert!(run("ARG NPM_TOKEN\n").is_empty());
    }

    #[test]
    fn allows_non_secret_arg() {
        assert!(run("ARG NODE_VERSION=22.12\n").is_empty());
    }
}
