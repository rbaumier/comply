use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "source_file" { return; }
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        match c.kind() {
            "comment" => continue,
            "from_instruction" | "arg_instruction" => return,
            _ => {
                let pos = c.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: "First non-comment instruction must be FROM (or ARG before FROM).".into(),
                    severity: Severity::Warning,
                    span: Some((c.byte_range().start, c.byte_range().len())),
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_run_before_from() {
        assert_eq!(run("RUN echo hello\nFROM node:20\n").len(), 1);
    }

    #[test]
    fn allows_from_first() {
        assert!(run("FROM node:20\nRUN echo hello\n").is_empty());
    }

    #[test]
    fn allows_arg_before_from() {
        assert!(run("ARG VERSION=20\nFROM node:$VERSION\n").is_empty());
    }

    #[test]
    fn allows_comments_before_from() {
        assert!(run("# header comment\nFROM node:20\n").is_empty());
    }
}
