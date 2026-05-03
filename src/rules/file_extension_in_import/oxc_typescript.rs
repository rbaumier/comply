//! file-extension-in-import OXC backend — flag relative imports missing a file extension.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const KNOWN_EXTENSIONS: &[&str] = &[
    ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json", ".css", ".scss",
    ".less", ".svg", ".png", ".vue", ".svelte",
];

const BUNDLER_DEPS: &[&str] = &[
    "vite",
    "webpack",
    "next",
    "esbuild",
    "parcel",
    "rollup",
    "@parcel/core",
    "@rspack/core",
    "rspack",
    "turbopack",
    "metro",
    "bun",
    "@swc/core",
    "tsup",
];

const BUNDLER_CONFIG_FILES: &[&str] = &[
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mts",
    "vite.config.mjs",
    "vite.config.cts",
    "vite.config.cjs",
    "webpack.config.ts",
    "webpack.config.js",
    "webpack.config.mts",
    "webpack.config.mjs",
    "webpack.config.cts",
    "webpack.config.cjs",
    "next.config.ts",
    "next.config.js",
    "next.config.mjs",
    "next.config.cjs",
    "turbopack.config.ts",
    "turbopack.config.js",
];

fn has_known_extension(spec: &str) -> bool {
    KNOWN_EXTENSIONS.iter().any(|ext| spec.ends_with(ext))
}

fn is_directory_import(spec: &str) -> bool {
    spec.ends_with('/') || spec.ends_with("/index")
}

fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

fn project_uses_bundler(ctx: &CheckCtx) -> bool {
    if let Some(pkg) = ctx.project.nearest_package_json(ctx.path)
        && (BUNDLER_DEPS.iter().any(|dep| pkg.has_dep_or_engine(dep))
            || pkg.all_deps().any(|dep| dep.starts_with("@vitejs/")))
    {
        return true;
    }
    has_bundler_config(ctx.path)
}

fn has_bundler_config(path: &std::path::Path) -> bool {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if BUNDLER_CONFIG_FILES
            .iter()
            .any(|name| d.join(name).is_file())
        {
            return true;
        }
        dir = d.parent();
    }
    false
}

fn tsconfig_uses_bundler_resolution(ctx: &CheckCtx) -> bool {
    let Some(ts) = ctx.project.nearest_tsconfig(ctx.path) else {
        return false;
    };
    ts.module_resolution
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("bundler"))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[] // full-program analysis
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if project_uses_bundler(ctx) {
            return Vec::new();
        }
        if tsconfig_uses_bundler_resolution(ctx) {
            return Vec::new();
        }

        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::ImportDeclaration(import) => {
                    let spec = import.source.value.as_str();
                    check_specifier(spec, import.source.span.start, ctx, &mut diagnostics);
                }
                AstKind::ExportNamedDeclaration(export) => {
                    if let Some(source) = &export.source {
                        let spec = source.value.as_str();
                        check_specifier(spec, source.span.start, ctx, &mut diagnostics);
                    }
                }
                AstKind::ExportAllDeclaration(export) => {
                    let spec = export.source.value.as_str();
                    check_specifier(spec, export.source.span.start, ctx, &mut diagnostics);
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn check_specifier(
    spec: &str,
    span_start: u32,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_relative(spec) {
        return;
    }
    if has_known_extension(spec) {
        return;
    }
    if is_directory_import(spec) {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Relative import `{spec}` is missing a file extension. Add an explicit extension (e.g. `.js`, `.ts`) for ESM compatibility.",
        ),
        severity: Severity::Warning,
        span: None,
    });
}
