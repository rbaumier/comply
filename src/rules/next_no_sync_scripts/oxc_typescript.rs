//! OxcCheck backend for next-no-sync-scripts.
//!
//! Flags `<script src="...">` JSX tags that lack `async`/`defer` in
//! Next.js projects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_jsx_attr(attrs: &oxc_ast::ast::JSXAttributeItems, name: &str) -> bool {
    attrs.iter().any(|item| {
        if let oxc_ast::ast::JSXAttributeItem::Attribute(attr) = item {
            if let oxc_ast::ast::JSXAttributeName::Identifier(id) = &attr.name {
                return id.name.as_str() == name;
            }
        }
        false
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.project.framework != Framework::NextJs {
            return;
        }

        let AstKind::JSXOpeningElement(el) = node.kind() else {
            return;
        };

        // Must be a <script> tag
        let oxc_ast::ast::JSXElementName::Identifier(tag) = &el.name else {
            return;
        };
        if tag.name.as_str() != "script" {
            return;
        }

        // Must have `src` attribute
        if !has_jsx_attr(&el.attributes, "src") {
            return;
        }

        // Must NOT have `async` or `defer`
        if has_jsx_attr(&el.attributes, "async") || has_jsx_attr(&el.attributes, "defer") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, el.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `<Script>` from `next/script` instead of a synchronous `<script src>` tag."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::project::{Framework, ProjectCtx};

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(source, &Check, &next_project())
    }

    #[test]
    fn flags_sync_script_with_src() {
        assert_eq!(
            run("export default function Page() { return <script src='/a.js' />; }").len(),
            1
        );
    }

    #[test]
    fn allows_async_script() {
        assert!(
            run("export default function Page() { return <script src='/a.js' async />; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_defer_script() {
        assert!(
            run("export default function Page() { return <script src='/a.js' defer />; }")
                .is_empty()
        );
    }

    #[test]
    fn ignores_inline_script_without_src() {
        assert!(
            run("export default function Page() { return <script>{`alert(1)`}</script>; }")
                .is_empty()
        );
    }
}
