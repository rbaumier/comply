use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["export_statement"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if crate::rules::path_utils::is_config_file(ctx.path) {
            return;
        }
        let path_str = ctx.path.to_string_lossy();
        if ctx.project.framework_entry_dirs().any(|dir| path_str.contains(dir)) {
            return;
        }
        let source = ctx.source.as_bytes();
        let text = node.utf8_text(source).unwrap_or("");
        if !text.starts_with("export default ") && !text.starts_with("export default\n") {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Default exports are forbidden — use a named export instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
