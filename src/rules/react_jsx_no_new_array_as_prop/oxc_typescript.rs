//! react-jsx-no-new-array-as-prop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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
        // In test files a component is rendered once and never re-rendered,
        // so the "new reference every render" cost does not apply.
        // Storybook stories are also single-render by nature.
        if ctx.file.path_segments.in_test_dir || ctx.file.path_segments.in_storybook {
            return;
        }

        // When the React Compiler is enabled it auto-memoises inline prop
        // references, so manual hoisting is redundant noise and can interfere
        // with the compiler's optimisation analysis.
        if ctx.project.has_dependency("babel-plugin-react-compiler") {
            return;
        }

        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let attr_name = name_ident.name.as_str();

            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ArrayExpression(arr) = &container.expression else {
                continue;
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, arr.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "array literal as value of JSX prop `{attr_name}` creates a new reference every render — extract to a constant or use `useMemo`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use crate::rules::test_helpers::{
        run_oxc_tsx, run_oxc_tsx_with_file_ctx, run_oxc_tsx_with_project,
    };

    fn react_compiler_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project
            .dev_dependencies
            .insert("babel-plugin-react-compiler".to_string(), "^1.0.0".to_string());
        project
    }

    fn test_file_ctx() -> FileCtx {
        FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        }
    }

    fn storybook_file_ctx() -> FileCtx {
        FileCtx {
            path_segments: PathSegments { in_storybook: true, ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn flags_array_literal_in_prod_file() {
        let src = "const x = <DataTable data={[row1, row2]} />;";
        assert_eq!(run_oxc_tsx(src, &Check).len(), 1);
    }

    #[test]
    fn no_fp_in_test_file_dot_test_tsx() {
        // Regression: issue #442 — render() in tests is a single render, no re-render cost.
        let src = "render(<DataTable data={[row1, row2]} columns={columns} />);";
        assert!(run_oxc_tsx_with_file_ctx(src, &Check, &test_file_ctx()).is_empty());
    }

    #[test]
    fn no_fp_in_spec_file() {
        let src = "render(<AsyncMultiSelect options={[{ value: 'a', label: 'A' }]} />);";
        assert!(run_oxc_tsx_with_file_ctx(src, &Check, &test_file_ctx()).is_empty());
    }

    #[test]
    fn no_fp_in_tests_dir() {
        let src = "render(<Comp items={[1, 2, 3]} />);";
        assert!(run_oxc_tsx_with_file_ctx(src, &Check, &test_file_ctx()).is_empty());
    }

    #[test]
    fn no_fp_in_storybook_file() {
        let src = "export const Default = () => <Comp items={['a', 'b']} />;";
        assert!(run_oxc_tsx_with_file_ctx(src, &Check, &storybook_file_ctx()).is_empty());
    }

    #[test]
    fn allows_identifier_in_prod_file() {
        let src = "const x = <Comp items={items} />;";
        assert!(run_oxc_tsx(src, &Check).is_empty());
    }

    #[test]
    fn no_fp_when_react_compiler_enabled() {
        // Regression: issue #442 — the React Compiler auto-memoises inline
        // references, so hoisting is redundant noise.
        let src = "const x = <AsyncMultiSelect options={[{ value: 'a', label: 'A' }]} />;";
        assert!(run_oxc_tsx_with_project(src, &Check, &react_compiler_project()).is_empty());
    }

    #[test]
    fn still_flags_without_react_compiler() {
        let src = "const x = <DataTable data={[row1, row2]} />;";
        assert_eq!(
            run_oxc_tsx_with_project(src, &Check, &ProjectCtx::empty()).len(),
            1
        );
    }
}
