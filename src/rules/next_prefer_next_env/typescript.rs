//! next-prefer-next-env backend.
//!
//! Flags member access ending in `__NEXT_DATA__`. The legacy global isn't
//! a stable API; modern Next.js apps should rely on `process.env.NEXT_PUBLIC_*`
//! variables, which are inlined at build time.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn property_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("property")?.utf8_text(source).ok()
}

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    let Some(prop) = property_name(node, source) else { return };
    if prop != "__NEXT_DATA__" {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-prefer-next-env".into(),
        message: "Don't read `__NEXT_DATA__`. Use `process.env.NEXT_PUBLIC_*` for build-time configuration.".into(),
        severity: Severity::Warning,
        span: Some((range.start, range.len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_and_file(
            source,
            &Check,
            project,
            &FileCtx::default(),
        )
    }

    #[test]
    fn flags_window_next_data() {
        let src = "const v = window.__NEXT_DATA__.props;";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_process_env_next_public() {
        let src = "const v = process.env.NEXT_PUBLIC_API_URL;";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "const v = window.__NEXT_DATA__;";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
