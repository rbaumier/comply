use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["maintainer_instruction"] => |node, source, ctx, diagnostics|
    let _ = source;
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "MAINTAINER is deprecated; use `LABEL maintainer=...` instead.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_maintainer() {
        assert_eq!(run("FROM node:20\nMAINTAINER user@example.com\n").len(), 1);
    }

    #[test]
    fn allows_label_maintainer() {
        assert!(run("FROM node:20\nLABEL maintainer=\"user@example.com\"\n").is_empty());
    }
}
