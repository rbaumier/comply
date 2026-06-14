//! react-jsx-no-new-array-as-prop oxc backend.
//!
//! Skipped when the file does not import React (the new-reference-per-render
//! rationale is React-specific; non-React JSX frameworks have different
//! reconciliation models).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

/// React-Compiler detection for the per-node fast path. The project-level
/// answer (`ProjectCtx::uses_react_compiler`) is memoized per directory behind
/// a `Mutex`; wrap it in the lock-free thread-local file slot so a JSX-dense
/// file takes the lock at most once instead of on every opening element.
fn project_uses_react_compiler(ctx: &CheckCtx) -> bool {
    crate::oxc_helpers::cached_file_bool(
        ctx.source,
        crate::oxc_helpers::SLOT_REACT_COMPILER,
        || ctx.project.uses_react_compiler(ctx.path),
    )
}

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

        // The new-reference-per-render concern is React-specific. A file that
        // uses JSX without importing React targets a non-React framework
        // (remix/ui, SolidJS, Preact, Vue JSX) with a different reconciliation
        // model, so the rule does not apply.
        if !crate::oxc_helpers::imports_react(ctx.source) {
            return;
        }

        // When React Compiler is enabled it auto-memoises inline prop
        // references, so manual hoisting is redundant noise and can interfere
        // with the compiler's optimisation analysis.
        if project_uses_react_compiler(ctx) {
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn run_in_project(dir: &std::path::Path, file_rel: &str, source: &str) -> Vec<Diagnostic> {
        let file_path = dir.join(file_rel);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let lang = Language::from_path(&file_path).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: lang,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, source, &project);

        let kinds = Check.interested_kinds();
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let ty = node.kind().ty();
            if kinds.contains(&ty) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
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
        let src = "import React from 'react';\nconst x = <DataTable data={[row1, row2]} />;";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    #[test]
    fn skips_non_react_jsx_framework() {
        // Issue #1669: a .tsx file using a non-React JSX framework (remix/ui)
        // must not be flagged — the new-reference-per-render rationale is
        // React-specific.
        let src = "import { Frame } from 'remix/ui';\nconst x = <Frame data={[row1, row2]} />;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn no_fp_in_test_file_dot_test_tsx() {
        // Regression: issue #442 — render() in tests is a single render, no re-render cost.
        let src = "render(<DataTable data={[row1, row2]} columns={columns} />);";
        assert!(crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &test_file_ctx()).is_empty());
    }

    #[test]
    fn no_fp_in_spec_file() {
        let src = "render(<AsyncMultiSelect options={[{ value: 'a', label: 'A' }]} />);";
        assert!(crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &test_file_ctx()).is_empty());
    }

    #[test]
    fn no_fp_in_tests_dir() {
        let src = "render(<Comp items={[1, 2, 3]} />);";
        assert!(crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &test_file_ctx()).is_empty());
    }

    #[test]
    fn no_fp_in_storybook_file() {
        let src = "export const Default = () => <Comp items={['a', 'b']} />;";
        assert!(crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &storybook_file_ctx()).is_empty());
    }

    #[test]
    fn allows_identifier_in_prod_file() {
        let src = "import React from 'react';\nconst x = <Comp items={items} />;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn flags_when_no_react_compiler() {
        let dir = TempDir::new().unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <DataTable data={[row1, row2]} />;",
        );
        assert_eq!(d.len(), 1, "baseline: should flag inline array: {d:?}");
    }

    #[test]
    fn flags_when_file_imports_react() {
        // Negative-space guard for issue #1669: a file that imports React still
        // fires on an inline array prop.
        let dir = TempDir::new().unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <DataTable data={[row1, row2]} />;",
        );
        assert_eq!(d.len(), 1, "React file with inline array still flags: {d:?}");
    }

    #[test]
    fn skips_when_package_json_declares_react_compiler() {
        // Regression: issue #442 — React Compiler auto-memoises inline arrays.
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"babel-plugin-react-compiler":"^1.0.0"}}"#,
        )
        .unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <AsyncMultiSelect options={[{ value: 'a', label: 'A' }]} />;",
        );
        assert!(d.is_empty(), "react-compiler dep declared: {d:?}");
    }

    #[test]
    fn skips_when_vite_config_references_react_compiler() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("vite.config.ts"),
            "import react from '@vitejs/plugin-react';\nexport default { plugins: [react({ babel: { plugins: ['babel-plugin-react-compiler'] } })] };",
        )
        .unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <DataTable data={[row1, row2]} />;",
        );
        assert!(
            d.is_empty(),
            "React Compiler memoises inline arrays — rule must stay silent: {d:?}"
        );
    }

    #[test]
    fn flags_when_vite_config_does_not_mention_react_compiler() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("vite.config.ts"),
            "import react from '@vitejs/plugin-react';\nexport default { plugins: [react()] };",
        )
        .unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <DataTable data={[row1, row2]} />;",
        );
        assert_eq!(
            d.len(),
            1,
            "vite without react-compiler: rule still applies: {d:?}"
        );
    }
}
