//! dockerfile-require-dockerignore tree-sitter backend.
//!
//! Flags any `COPY . <dest>` whose immediately-preceding sibling is not a
//! comment mentioning `dockerignore`. Without a `.dockerignore` file the broad
//! copy ships secrets, build artefacts, and `node_modules` into the image.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["copy_instruction"] => |node, source, ctx, diagnostics|
    if !copy_is_dot(node, source) {
        return;
    }
    if let Some(prev) = node.prev_sibling()
        && prev.kind() == "comment"
        && prev
            .utf8_text(source)
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains("dockerignore")
    {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "Broad `COPY .` without `.dockerignore` acknowledgement — make sure `node_modules`, `.git`, `.env` are excluded.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn copy_is_dot(copy: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = copy.walk();
    let paths: Vec<&str> = copy
        .children(&mut cursor)
        .filter(|c| c.kind() == "path")
        .filter_map(|c| c.utf8_text(source).ok())
        .collect();
    paths.len() >= 2 && paths[0] == "."
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_copy_all_without_comment() {
        assert_eq!(run("COPY . .\n").len(), 1);
    }

    #[test]
    fn allows_copy_all_with_dockerignore_comment() {
        let src = "# .dockerignore excludes node_modules and .env\nCOPY . .\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_specific_copy() {
        assert!(run("COPY package.json ./\n").is_empty());
    }
}
