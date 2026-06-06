//! react-jsx-no-new-object-as-prop OxcCheck backend.
//!
//! Flags `jsx_attribute` nodes whose value is an inline object literal.
//!
//! Skipped when the project is configured with `babel-plugin-react-compiler`
//! (auto-memoising inline objects/arrays), detected via a dep entry in
//! `package.json` or a reference inside `vite.config.*`, `next.config.*`,
//! or `babel.config.*` walking up from the file.

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
    crate::oxc_helpers::cached_file_bool(
        ctx.source,
        crate::oxc_helpers::SLOT_REACT_COMPILER,
        || compute_uses_react_compiler(ctx),
    )
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
            "const x = <Comp style={{ color: 'red' }} />;",
        );
        assert_eq!(d.len(), 1, "baseline: should flag inline object: {d:?}");
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
            "const x = <Comp style={{ color: 'red' }} />;",
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
            "const x = <Comp config={{ a: 1 }} />;",
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
            "const x = <Comp style={{ color: 'red' }} />;",
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
            "const x = <Comp style={{ color: 'red' }} />;",
        );
        assert_eq!(
            d.len(),
            1,
            "vite without react-compiler: rule still applies: {d:?}"
        );
    }
}
