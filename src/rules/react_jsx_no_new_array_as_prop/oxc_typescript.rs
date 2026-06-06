//! react-jsx-no-new-array-as-prop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

const REACT_COMPILER_DEP: &str = "babel-plugin-react-compiler";

/// Config files that may opt the project into React Compiler.
const COMPILER_CONFIG_FILES: &[&str] = &[
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mts",
    "vite.config.mjs",
    "vite.config.cts",
    "vite.config.cjs",
    "next.config.ts",
    "next.config.js",
    "next.config.mjs",
    "next.config.cjs",
    "babel.config.ts",
    "babel.config.js",
    "babel.config.mjs",
    "babel.config.cjs",
    "babel.config.json",
    ".babelrc",
    ".babelrc.json",
    ".babelrc.js",
    ".babelrc.cjs",
];

/// True when the project ships React Compiler — either declared as a
/// dependency or referenced inside a bundler / babel config.
///
/// The result depends only on the file path and project, not the node, but
/// `run` is invoked per `JSXOpeningElement`. The underlying check walks the
/// directory tree stat-ing config files, so memoize it per path — otherwise a
/// JSX-dense file pays the full filesystem walk on every opening element.
fn project_uses_react_compiler(ctx: &CheckCtx) -> bool {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;
    thread_local! {
        static CACHE: RefCell<HashMap<PathBuf, bool>> = RefCell::new(HashMap::new());
    }
    if let Some(v) = CACHE.with(|c| c.borrow().get(ctx.path).copied()) {
        return v;
    }
    let v = compute_uses_react_compiler(ctx);
    CACHE.with(|c| c.borrow_mut().insert(ctx.path.to_path_buf(), v));
    v
}

/// The config-file walk is bounded by `project_root` (or the nearest
/// directory that contains a `package.json`) so it never escapes the project
/// boundary into a monorepo root, home directory, or `/`.
fn compute_uses_react_compiler(ctx: &CheckCtx) -> bool {
    if let Some(pkg) = ctx.project.nearest_package_json(ctx.path)
        && pkg.has_dep_or_engine(REACT_COMPILER_DEP)
    {
        return true;
    }

    // Determine the upper bound for the config-file walk: prefer the explicit
    // project root; fall back to the first ancestor directory that owns a
    // `package.json` (found by walking up from the file being checked).
    let stop_at: Option<std::path::PathBuf> =
        ctx.project.project_root.clone().or_else(|| {
            let mut d = ctx.path.parent();
            loop {
                let Some(dir) = d else { break None };
                if dir.join("package.json").is_file() {
                    break Some(dir.to_path_buf());
                }
                d = dir.parent();
            }
        });

    let mut dir = ctx.path.parent();
    while let Some(d) = dir {
        for name in COMPILER_CONFIG_FILES {
            let cfg = d.join(name);
            if !cfg.is_file() {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&cfg)
                && raw.contains(REACT_COMPILER_DEP)
            {
                return true;
            }
        }
        // Stop at the project root — never walk above it.
        if stop_at.as_deref() == Some(d) {
            break;
        }
        dir = d.parent();
    }
    false
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
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use crate::rules::test_helpers::{run_oxc_tsx, run_oxc_tsx_with_file_ctx};
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
    fn flags_when_no_react_compiler() {
        let dir = TempDir::new().unwrap();
        let d = run_in_project(
            dir.path(),
            "src/comp.tsx",
            "const x = <DataTable data={[row1, row2]} />;",
        );
        assert_eq!(d.len(), 1, "baseline: should flag inline array: {d:?}");
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
            "const x = <AsyncMultiSelect options={[{ value: 'a', label: 'A' }]} />;",
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
            "const x = <DataTable data={[row1, row2]} />;",
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
            "const x = <DataTable data={[row1, row2]} />;",
        );
        assert_eq!(
            d.len(),
            1,
            "vite without react-compiler: rule still applies: {d:?}"
        );
    }
}
