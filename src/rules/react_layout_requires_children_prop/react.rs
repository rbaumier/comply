//! react-layout-requires-children-prop backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn is_layout_file(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem == "layout")
}

fn first_parameter_text<'a>(fn_node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let params = fn_node.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        let kind = child.kind();
        if kind == "(" || kind == ")" || kind == "," {
            continue;
        }
        return child.utf8_text(source).ok();
    }
    None
}

fn exported_function<'a>(
    export: tree_sitter::Node<'a>,
    _source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = export.walk();
    let mut saw_default = false;
    for child in export.children(&mut cursor) {
        if child.kind() == "default" {
            saw_default = true;
            continue;
        }
        if !saw_default {
            continue;
        }
        match child.kind() {
            "function_declaration"
            | "function"
            | "function_expression"
            | "arrow_function" => return Some(child),
            _ => continue,
        }
    }
    None
}

crate::ast_check! { on ["export_statement"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if !ctx.file.path_segments.in_app_router {
        return;
    }
    if !is_layout_file(ctx.path) {
        return;
    }
    let Some(fn_node) = exported_function(node, source) else { return };
    let param_text = first_parameter_text(fn_node, source).unwrap_or("");
    if param_text.contains("children") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-layout-requires-children-prop".into(),
        message: "This layout's default export doesn't accept `children`. \
                  The router passes nested routes via `children` — drop it and \
                  the layout renders an empty page."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn app_router_file_ctx() -> FileCtx {
        FileCtx {
            path_segments: PathSegments {
                in_app_router: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_file_and_path(
            source,
            &Check,
            &next_project(),
            &app_router_file_ctx(),
            path,
        )
    }

    #[test]
    fn flags_layout_without_children() {
        let src = r#"
export default function Layout() {
    return <div>fixed</div>;
}
"#;
        assert_eq!(run(src, "/proj/app/layout.tsx").len(), 1);
    }

    #[test]
    fn allows_layout_with_destructured_children() {
        let src = r#"
export default function Layout({ children }: { children: React.ReactNode }) {
    return <div>{children}</div>;
}
"#;
        assert!(run(src, "/proj/app/layout.tsx").is_empty());
    }

    #[test]
    fn allows_layout_with_children_on_typed_props() {
        let src = r#"
export default function Layout(props: { children: React.ReactNode }) {
    return <div>{props.children}</div>;
}
"#;
        assert!(run(src, "/proj/app/layout.tsx").is_empty());
    }

    #[test]
    fn ignores_non_layout_file() {
        let src = r#"
export default function Page() {
    return <div />;
}
"#;
        assert!(run(src, "/proj/app/page.tsx").is_empty());
    }

    #[test]
    fn ignores_file_outside_app_router() {
        let diags = crate::rules::test_helpers::run_tsx_with_project_file_and_path(
            "export default function Layout() { return <div />; }",
            &Check,
            &next_project(),
            &FileCtx::default(),
            "/proj/pages/layout.tsx",
        );
        assert!(diags.is_empty());
    }
}
