//! import-no-unresolved OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();
        if index.is_empty() {
            return Vec::new();
        }

        let canon = index.canonical(ctx.path);
        let mut seen: HashSet<(String, usize)> = HashSet::new();
        let mut diagnostics = Vec::new();

        for imp in index.get_imports(&canon) {
            let is_relative = imp.specifier.starts_with("./") || imp.specifier.starts_with("../");
            if !is_relative {
                continue;
            }
            if imp.source_path.is_some() {
                continue;
            }
            // Skip gitignored build-time generated files (e.g. TanStack
            // Router's `routeTree.gen.ts`): often absent at lint time, always
            // present at build/dev time.
            if is_generated_specifier(&imp.specifier) {
                continue;
            }
            if is_generated_dir_specifier(&imp.specifier) {
                continue;
            }
            if is_build_output_specifier(&imp.specifier) {
                continue;
            }
            // CSS, CSS Modules, SVG, and other static assets are imported via
            // build-tool support (Webpack, Vite, Next.js) and never enter the
            // TS/JS index. When such a non-source file exists on disk next to
            // the importer, the import is resolved — don't flag it.
            if is_existing_asset_import(ctx.path, &imp.specifier) {
                continue;
            }
            if !seen.insert((imp.specifier.clone(), imp.line)) {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: imp.line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unable to resolve import path `{}` — file does not exist.",
                    imp.specifier
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// True for specifiers pointing at a build-time generated file whose final
/// segment ends in `.gen` (e.g. `./routeTree.gen`) or carries a `.gen.`
/// extension stem (e.g. `./routeTree.gen.ts`). Such files are gitignored and
/// often absent at lint time, yet always present at build/dev time.
fn is_generated_specifier(spec: &str) -> bool {
    let last = spec.rsplit('/').next().unwrap_or(spec);
    last.ends_with(".gen") || last.contains(".gen.")
}

/// True for specifiers that traverse into a conventional build-output
/// directory (`dist`, `build`, `out`). Integration tests deliberately import
/// the compiled artifact (e.g. `../../dist/cjs/index.js`); these directories
/// are gitignored and absent in a clean checkout, so an unresolved import
/// there is expected, not a defect.
fn is_build_output_specifier(spec: &str) -> bool {
    spec.split('/').any(|seg| matches!(seg, "dist" | "build" | "out"))
}

/// True for specifiers that traverse into a directory holding code-generated
/// output (`generated`/`__generated__` from Prisma, GraphQL/Relay codegen, the
/// `.prisma` client cache) or into `node_modules`. These artifacts are produced
/// by a build step (`prisma generate`, `graphql-codegen`, `npm install`), are
/// gitignored, and are absent in a clean checkout — so an unresolved import
/// there is expected, not a defect.
fn is_generated_dir_specifier(spec: &str) -> bool {
    spec.split('/')
        .any(|seg| matches!(seg, "generated" | "__generated__" | ".prisma" | "node_modules"))
}

/// Source extensions resolved through the TS/JS import index. A specifier
/// carrying one of these is a real source import — if it stayed unresolved,
/// the target is genuinely missing and must be flagged.
const SOURCE_EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "mjs", "cts", "cjs", "vue"];

/// True for a relative specifier that names a non-source file (a `.css`,
/// `.svg`, `.png`, … asset) present on disk next to the importer. These never
/// enter the TS/JS index, so `source_path` is always `None`, yet the import is
/// resolved at build time.
pub(super) fn is_existing_asset_import(importer: &Path, specifier: &str) -> bool {
    let Some(ext) = Path::new(specifier).extension().and_then(|e| e.to_str()) else {
        return false;
    };
    if SOURCE_EXTS.contains(&ext) {
        return false;
    }
    let Some(base_dir) = importer.parent() else {
        return false;
    };
    base_dir.join(specifier).is_file()
}

#[cfg(test)]
mod oxc_tests {
    use super::{is_build_output_specifier, is_generated_dir_specifier, is_generated_specifier};

    #[test]
    fn detects_generated_specifiers_issue_487() {
        assert!(is_generated_specifier("./routeTree.gen"));
        assert!(is_generated_specifier("./routeTree.gen.ts"));
        assert!(is_generated_specifier("../app/routeTree.gen"));
        assert!(!is_generated_specifier("./routeTree"));
        assert!(!is_generated_specifier("./generated"));
    }

    #[test]
    fn detects_build_output_specifiers_issue_1005() {
        // reproducers from the issue
        assert!(is_build_output_specifier("../../../dist/cjs/index.js"));
        assert!(is_build_output_specifier("../../dist/esm/index.js"));
        assert!(is_build_output_specifier("../build/index.js"));
        assert!(is_build_output_specifier("./out/index.js"));
        // still flagged — real source / not an exact build segment
        assert!(!is_build_output_specifier("./src/index.js"));
        assert!(!is_build_output_specifier("../distance/index.js"));
        assert!(!is_build_output_specifier("./distribution/x"));
        assert!(!is_build_output_specifier("./lib/util.js")); // lib intentionally NOT skipped
    }

    #[test]
    fn detects_generated_dir_specifiers_issue_1420() {
        // reproducers from the issue (Prisma / GraphQL codegen output)
        assert!(is_generated_dir_specifier("./generated/prisma/client"));
        assert!(is_generated_dir_specifier("./generated/client"));
        assert!(is_generated_dir_specifier("./node_modules/@prisma/client"));
        assert!(is_generated_dir_specifier("../src/__generated__/graphql"));
        assert!(is_generated_dir_specifier("./.prisma/client"));
        // still flagged — a genuinely broken relative import has no codegen segment
        assert!(!is_generated_dir_specifier("./does-not-exist"));
        assert!(!is_generated_dir_specifier("../utils/helper"));
        assert!(!is_generated_dir_specifier("./generated-things")); // substring, not a segment
    }
}
