use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "__mocks__"];

fn is_test_path(path: &str) -> bool {
    TEST_MARKERS.iter().any(|m| path.contains(m))
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
        let current_path = ctx.path.to_string_lossy();
        if is_test_path(&current_path) {
            return;
        }
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let module = import.source.value.as_str();
        if !is_test_path(module) {
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
    use crate::rules::test_helpers::run_oxc_ts_with_path;



    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(source, &Check, path)
    }


    #[test]
    fn flags_import_of_test_file_from_prod() {
        let d = run_on_path("import { fixture } from './foo.test.ts';", "src/foo.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("foo.test.ts"));
    }


    #[test]
    fn flags_import_of_spec_file_from_prod() {
        let d = run_on_path("import { stub } from './bar.spec.ts';", "src/bar.ts");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_import_from_tests_folder() {
        let d = run_on_path(
            "import { helper } from './__tests__/helpers';",
            "src/mod.ts",
        );
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_import_from_mocks_folder() {
        let d = run_on_path("import svc from './__mocks__/service';", "src/app.ts");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_test_file_importing_other_test_file() {
        let d = run_on_path(
            "import { fixture } from './util.test.ts';",
            "src/foo.test.ts",
        );
        assert!(d.is_empty());
    }


    #[test]
    fn allows_normal_import() {
        let d = run_on_path("import { foo } from './foo';", "src/bar.ts");
        assert!(d.is_empty());
    }
}
