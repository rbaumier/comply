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
            // Router's `routeTree.gen.ts`) and imports into build-output /
            // codegen directories (dist/build/out, generated/__generated__/
            // .prisma/prisma/gen, node_modules): often absent at lint time,
            // always present at build/dev time.
            if crate::rules::path_utils::is_generated_file_specifier(&imp.specifier)
                || crate::rules::path_utils::is_build_output_specifier(&imp.specifier)
            {
                continue;
            }
            // CSS, CSS Modules, SVG, and other static assets are imported via
            // build-tool support (Webpack, Vite, Next.js) and never enter the
            // TS/JS index. When such a non-source file exists on disk next to
            // the importer, the import is resolved — don't flag it.
            if is_existing_asset_import(ctx.path, &imp.specifier) {
                continue;
            }
            // A relative import whose target source file exists on disk but lives
            // in a directory excluded from the scan (e.g. vendored code under
            // `vendor/`) is absent from the import index, so `source_path` is
            // `None` — yet the import is genuinely resolvable. Don't flag it.
            if is_existing_source_import(ctx.path, &imp.specifier) {
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

/// True for a relative specifier that resolves to a real source file on disk,
/// even when that file is absent from the import index because it lives in a
/// directory excluded from the scan (e.g. vendored code under `vendor/`).
/// Mirrors the import index's resolution order — bare path, each source
/// extension, then `index.<ext>` — but checks the filesystem directly instead
/// of the in-memory `known` set. A specifier with no matching file on disk
/// (e.g. `./does-not-exist`) returns `false` and is still flagged.
pub(super) fn is_existing_source_import(importer: &Path, specifier: &str) -> bool {
    let Some(base_dir) = importer.parent() else {
        return false;
    };
    let raw = base_dir.join(specifier);

    if Path::new(specifier)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| SOURCE_EXTS.contains(&ext))
        && raw.is_file()
    {
        return true;
    }
    if SOURCE_EXTS.iter().any(|ext| raw.with_extension(ext).is_file()) {
        return true;
    }
    SOURCE_EXTS
        .iter()
        .any(|ext| raw.join(format!("index.{ext}")).is_file())
}

#[cfg(test)]
mod oxc_tests {
    use crate::rules::path_utils::{is_build_output_specifier, is_generated_file_specifier};

    #[test]
    fn detects_generated_specifiers_issue_487() {
        assert!(is_generated_file_specifier("./routeTree.gen"));
        assert!(is_generated_file_specifier("./routeTree.gen.ts"));
        assert!(is_generated_file_specifier("../app/routeTree.gen"));
        assert!(!is_generated_file_specifier("./routeTree"));
        assert!(!is_generated_file_specifier("./generated"));
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
        // reproducers from the issue (Prisma / GraphQL codegen output); the
        // generated-dir set is now part of `is_build_output_specifier`.
        assert!(is_build_output_specifier("./generated/prisma/client"));
        assert!(is_build_output_specifier("./generated/client"));
        assert!(is_build_output_specifier("./node_modules/@prisma/client"));
        assert!(is_build_output_specifier("../src/__generated__/graphql"));
        assert!(is_build_output_specifier("./.prisma/client"));
        // still flagged — a genuinely broken relative import has no codegen segment
        assert!(!is_build_output_specifier("./does-not-exist"));
        assert!(!is_build_output_specifier("../utils/helper"));
        assert!(!is_build_output_specifier("./generated-things")); // substring, not a segment
    }
}
