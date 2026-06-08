//! dockerfile-copy-from-not-self tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

fn from_value(param_text: &str) -> Option<&str> {
    let stripped = param_text.strip_prefix("--from=")?;
    Some(stripped.trim())
}

fn current_stage_alias<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut prev = node.prev_sibling();
    while let Some(sibling) = prev {
        if sibling.kind() == "from_instruction" {
            for j in 0..sibling.child_count() {
                let sub = sibling.child(j).unwrap();
                if sub.kind() == "image_alias" {
                    return std::str::from_utf8(&source[sub.byte_range()])
                        .ok()
                        .map(str::trim);
                }
            }
            return None;
        }
        prev = sibling.prev_sibling();
    }
    None
}

crate::ast_check! { on ["copy_instruction"] => |node, source, ctx, diagnostics|
    let mut target: Option<&str> = None;
    let mut param_node: Option<tree_sitter::Node> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() != "param" { continue; }
        let Ok(t) = std::str::from_utf8(&source[child.byte_range()]) else { continue; };
        if let Some(v) = from_value(t) {
            target = Some(v);
            param_node = Some(child);
            break;
        }
    }
    let Some(target) = target else { return; };
    let Some(current) = current_stage_alias(node, source) else { return; };
    if target != current { return; }
    let highlight = param_node.unwrap_or(node);
    let pos = highlight.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!("`COPY --from={target}` references the current build stage."),
        severity: Severity::Warning,
        span: Some((highlight.byte_range().start, highlight.byte_range().len())),
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "Dockerfile")
    }

    #[test]
    fn flags_self_reference() {
        let src = "FROM node:20 AS build\nCOPY --from=build /app /app\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_other_stage() {
        let src = "FROM node:20 AS build\nFROM alpine AS runtime\nCOPY --from=build /app /app\n";
        assert!(run(src).is_empty());
    }
}
