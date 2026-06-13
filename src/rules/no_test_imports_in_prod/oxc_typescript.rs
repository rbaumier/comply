use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "__mocks__"];

fn is_test_path(path: &str) -> bool {
    TEST_MARKERS.iter().any(|m| path.contains(m))
}

/// In `*-perf-tests` packages (e.g. Azure SDK's `@azure-tools/test-perf`
/// framework) benchmark classes live in `.spec.ts` files by convention and are
/// imported by the package's perf runner. The `.spec.` suffix is a framework
/// naming convention there, not a test-suite marker, so importing them from the
/// perf runner is not a test-into-prod leak.
fn in_perf_tests_package(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|seg| seg.ends_with("-perf-tests"))
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Dual-read: the unit-test harness injects an empty default FileCtx, so
        // `in_test_dir` is false in tests — fall back to the local marker scan.
        let current_path = ctx.path.to_string_lossy();
        if ctx.file.path_segments.in_test_dir || is_test_path(&current_path) {
            return;
        }
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let module = import.source.value.as_str();
        if !is_test_path(module) {
            return;
        }
        // `.spec.` benchmark classes in `*-perf-tests` packages are a framework
        // naming convention, not test suites — importing them from the perf
        // runner is legitimate.
        if module.contains(".spec.") && in_perf_tests_package(ctx.path) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        let range_start = import.span.start as usize;
        let range_len = (import.span.end - import.span.start) as usize;
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Production file imports test/mock module `{module}` \u{2014} move shared helpers out of the test file."
            ),
            severity: Severity::Warning,
            span: Some((range_start, range_len)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::OxcCheck;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::path::Path;

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        let path = Path::new(path);
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test(path, source);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if Check.interested_kinds().contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    // Regression for issue #1072: `.spec.` benchmark imports in a `*-perf-tests`
    // package are a framework convention, not test suites.
    #[test]
    fn allows_spec_import_in_perf_tests_package() {
        let d = run_on_path(
            "import { SerializeTest } from \"./serialize.spec.js\";",
            "sdk/schemaregistry/schema-registry-avro-perf-tests/src/index.ts",
        );
        assert!(d.is_empty(), "perf-tests spec import must not be flagged, got {d:?}");
    }

    #[test]
    fn flags_spec_import_outside_perf_tests_package() {
        let d = run_on_path("import { stub } from \"./bar.spec.ts\";", "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    // The perf-tests carve-out is `.spec.`-only: genuine `.test.` imports still leak.
    #[test]
    fn flags_test_import_in_perf_tests_package() {
        let d = run_on_path(
            "import { helper } from \"./util.test.js\";",
            "sdk/foo/foo-perf-tests/src/index.ts",
        );
        assert_eq!(d.len(), 1);
    }
}
