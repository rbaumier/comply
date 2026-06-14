//! react-jsx-no-new-object-as-prop OxcCheck backend.
//!
//! Flags `jsx_attribute` nodes whose value is an inline object literal.
//!
//! Skipped when the file does not import React (the new-reference-per-render
//! rationale is React-specific; non-React JSX frameworks have different
//! reconciliation models), or when the project is configured with
//! `babel-plugin-react-compiler` (auto-memoising inline objects/arrays),
//! detected via a dep entry in `package.json` or a reference inside
//! `vite.config.*`, `next.config.*`, or `babel.config.*` walking up from the file.

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
        // The new-reference-per-render concern is React-specific. A file that
        // uses JSX without importing React targets a non-React framework
        // (remix/ui, SolidJS, Preact, Vue JSX) with a different reconciliation
        // model, so the rule does not apply.
        if !crate::oxc_helpers::imports_react(ctx.source) {
            return;
        }
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
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };

            let attr_name = name_ident.name.as_str();
            let (line, column) =
                byte_offset_to_line_col(ctx.source, obj.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Object literal as value of JSX prop `{attr_name}` creates a new reference every render \u{2014} extract to a constant or wrap in `useMemo`."
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
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

    #[test]
    fn flags_when_no_react_compiler() {
        let dir = TempDir::new().unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <Comp style={{ color: 'red' }} />;",
        );
        assert_eq!(d.len(), 1, "baseline: should flag inline object: {d:?}");
    }

    #[test]
    fn skips_non_react_jsx_framework() {
        // Issue #1669: a .tsx file using a non-React JSX framework (remix/ui)
        // must not be flagged — the new-reference-per-render rationale is
        // React-specific.
        let dir = TempDir::new().unwrap();
        let d = run_in_project(
            dir.path(),
            "src/time.tsx",
            "import { Frame } from 'remix/ui';\nconst x = <Frame style={{ display: 'flex' }} />;",
        );
        assert!(d.is_empty(), "non-React JSX framework must not be flagged: {d:?}");
    }

    #[test]
    fn flags_when_file_imports_react() {
        // Negative-space guard: a file that does import React still fires.
        let dir = TempDir::new().unwrap();
        let d = run_in_project(
            dir.path(),
            "src/time.tsx",
            "import React from 'react';\nconst x = <Frame style={{ display: 'flex' }} />;",
        );
        assert_eq!(d.len(), 1, "React file with inline object still flags: {d:?}");
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
            "import React from 'react';\nconst x = <Comp style={{ color: 'red' }} />;",
        );
        assert!(
            d.is_empty(),
            "React Compiler memoises inline objects — rule must stay silent: {d:?}"
        );
    }

    #[test]
    fn skips_when_next_config_references_react_compiler() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("next.config.js"),
            "module.exports = { experimental: { reactCompiler: true }, babel: { plugins: ['babel-plugin-react-compiler'] } };",
        )
        .unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <Comp config={{ a: 1 }} />;",
        );
        assert!(d.is_empty(), "next.config with react-compiler: {d:?}");
    }

    #[test]
    fn skips_when_package_json_declares_react_compiler() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"babel-plugin-react-compiler":"^1.0.0"}}"#,
        )
        .unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "import React from 'react';\nconst x = <Comp style={{ color: 'red' }} />;",
        );
        assert!(d.is_empty(), "react-compiler dep declared: {d:?}");
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
            "import React from 'react';\nconst x = <Comp style={{ color: 'red' }} />;",
        );
        assert_eq!(
            d.len(),
            1,
            "vite without react-compiler: rule still applies: {d:?}"
        );
    }
}
