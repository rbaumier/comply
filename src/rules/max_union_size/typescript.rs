//! max-union-size backend — flag union types with more than 5 members.

use crate::diagnostic::{Diagnostic, Severity};

/// Count the total leaf type members in a (possibly nested) union_type.
/// tree-sitter represents `A | B | C | D | E | F` as a left-recursive tree:
///   union_type(union_type(union_type(..., A, B), C), ..., F)
/// so direct child count only shows 2-3, not the actual member count.
fn count_union_members(node: tree_sitter::Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "|" {
            continue;
        }
        if child.kind() == "union_type" {
            count += count_union_members(child);
        } else {
            count += 1;
        }
    }
    count
}

crate::ast_check! { on ["union_type"] => |node, source, ctx, diagnostics|
    // Only flag the outermost union_type (skip nested ones that are children of a union_type).
    if let Some(parent) = node.parent()
        && parent.kind() == "union_type"
    {
        return;
    }

    let max = ctx.config.threshold("max-union-size", "max", ctx.lang);
    let count = count_union_members(node) as usize;

    if count > max {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "max-union-size".into(),
            message: format!(
                "Union type has {count} members (max: {max}) — consider extracting a type alias."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_large_union_in_type_alias() {
        let src = "type Status = A | B | C | D | E | F;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_large_union_in_annotation() {
        let src = "function foo(x: A | B | C | D | E | F) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_small_union() {
        let src = "type Status = A | B | C;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_five_members() {
        let src = "type X = A | B | C | D | E;";
        assert!(run_on(src).is_empty());
    }
}
