use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "cmd_instruction" { return; }
    // Walk previous siblings up to the most recent FROM. If we find another
    // cmd_instruction in that range, flag the current one as a duplicate.
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "from_instruction" => return,
            "cmd_instruction" => {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: "Multiple CMD instructions in the same stage; only the last is honored.".into(),
                    severity: Severity::Warning,
                    span: Some((node.byte_range().start, node.byte_range().len())),
                });
                return;
            }
            _ => {}
        }
        prev = p.prev_sibling();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_duplicate_cmd() {
        assert_eq!(
            run("FROM node:20\nCMD [\"node\"]\nCMD [\"npm\", \"start\"]\n").len(),
            1
        );
    }

    #[test]
    fn allows_single_cmd() {
        assert!(run("FROM node:20\nCMD [\"node\", \"server.js\"]\n").is_empty());
    }
}
