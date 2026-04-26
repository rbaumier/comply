//! dockerfile-copy-from-known-stage tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

fn from_value<'a>(param_text: &'a str) -> Option<&'a str> {
    // param_text looks like `--from=build`.
    let stripped = param_text.strip_prefix("--from=")?;
    Some(stripped.trim())
}

fn collect_stage_aliases<'a>(root: tree_sitter::Node<'a>, source: &'a [u8]) -> Vec<&'a str> {
    let mut out = Vec::new();
    for i in 0..root.child_count() {
        let child = root.child(i).unwrap();
        if child.kind() != "from_instruction" {
            continue;
        }
        for j in 0..child.child_count() {
            let sub = child.child(j).unwrap();
            if sub.kind() == "image_alias" {
                if let Ok(t) = std::str::from_utf8(&source[sub.byte_range()]) {
                    out.push(t.trim());
                }
            }
        }
    }
    out
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "copy_instruction" { return; }
    // Find --from= param.
    let mut from_target: Option<&str> = None;
    let mut param_node: Option<tree_sitter::Node> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() != "param" { continue; }
        let Ok(t) = std::str::from_utf8(&source[child.byte_range()]) else { continue; };
        if let Some(v) = from_value(t) {
            from_target = Some(v);
            param_node = Some(child);
            break;
        }
    }
    let Some(target) = from_target else { return; };
    // Numeric stage indices are valid.
    if target.parse::<u32>().is_ok() { return; }
    // External images (contain `:` or `/`) are valid `--from=registry/image:tag`.
    if target.contains(':') || target.contains('/') { return; }
    // Walk up to source_file root.
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }
    let aliases = collect_stage_aliases(root, source);
    if aliases.iter().any(|a| *a == target) { return; }
    let highlight = param_node.unwrap_or(node);
    let pos = highlight.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!("`--from={target}` does not match any known build stage."),
        severity: Severity::Warning,
        span: Some((highlight.byte_range().start, highlight.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_unknown_stage() {
        let src = "FROM node:20 AS build\nCOPY --from=typo /app /app\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_known_stage() {
        let src = "FROM node:20 AS build\nCOPY --from=build /app /app\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_numeric_stage() {
        let src = "FROM node:20\nFROM alpine\nCOPY --from=0 /app /app\n";
        assert!(run(src).is_empty());
    }
}
