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
        if crate::rules::path_utils::is_framework_entry_point(ctx.path, ctx.project) {
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

    #[test]
    fn allows_framework_entry_file_suffix() {
        let project = crate::project::ProjectCtx::for_test_with_framework("tanstack-router");
        let diags = crate::rules::test_helpers::run_ts_with_project_and_path(
            "export default function RouteComponent() {}",
            &Check,
            &project,
            std::path::Path::new("src/posts.lazy.tsx"),
        );
        assert!(
            diags.is_empty(),
            "TanStack route suffix entry files require default exports: {diags:?}"
        );
    }
}
