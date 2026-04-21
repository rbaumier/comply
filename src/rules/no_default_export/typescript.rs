use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "export_statement" {
                return;
            }
            let text = node.utf8_text(source).unwrap_or("");
            if !text.starts_with("export default ") && !text.starts_with("export default\n") {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: "Default exports are forbidden — use a named export instead.".into(),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_default_function() {
        assert_eq!(run("export default function foo() {}").len(), 1);
    }

    #[test]
    fn flags_default_class() {
        assert_eq!(run("export default class Foo {}").len(), 1);
    }

    #[test]
    fn flags_default_expression() {
        assert_eq!(run("const x = 1; export default x;").len(), 1);
    }

    #[test]
    fn allows_named_export() {
        assert!(run("export function foo() {}").is_empty());
    }

    #[test]
    fn allows_named_class_export() {
        assert!(run("export class Foo {}").is_empty());
    }

    #[test]
    fn allows_re_export_default() {
        assert!(run("export { default } from './foo';").is_empty());
    }
}
