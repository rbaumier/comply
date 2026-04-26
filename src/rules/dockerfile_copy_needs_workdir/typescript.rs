use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["copy_instruction"] => |node, source, ctx, diagnostics|
    // Collect path children. The last `path` is the destination.
    let mut path_nodes: Vec<tree_sitter::Node> = Vec::new();
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        if c.kind() == "path" {
            path_nodes.push(c);
        }
    }
    if path_nodes.len() < 2 { return; }
    let dest = path_nodes.last().copied().unwrap();
    let dest_text = dest.utf8_text(source).unwrap_or("");
    let trimmed = dest_text.trim();
    if trimmed.starts_with('/') || trimmed.starts_with('$') {
        return;
    }

    // Walk previous siblings looking for a workdir_instruction.
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "workdir_instruction" { return; }
        prev = p.prev_sibling();
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "COPY with relative destination requires a prior WORKDIR.".into(),
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
    fn flags_copy_relative_without_workdir() {
        assert_eq!(run("FROM node:20\nCOPY package.json .\n").len(), 1);
    }

    #[test]
    fn allows_copy_with_prior_workdir() {
        assert!(run("FROM node:20\nWORKDIR /app\nCOPY package.json .\n").is_empty());
    }

    #[test]
    fn allows_copy_absolute_destination() {
        assert!(run("FROM node:20\nCOPY package.json /app/package.json\n").is_empty());
    }
}
