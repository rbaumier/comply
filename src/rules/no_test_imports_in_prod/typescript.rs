//! no-test-imports-in-prod backend — flag imports from test/mock modules
//! when the *current* file is a production file.
//!
//! A path is considered test-flavoured if it contains `.test.`, `.spec.`,
//! `__tests__`, or `__mocks__`. The rule only fires when the importer
//! itself is NOT test-flavoured — production code importing tests leaks
//! fixtures into the shipped bundle.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "__mocks__"];

fn is_test_path(path: &str) -> bool {
    TEST_MARKERS.iter().any(|m| path.contains(m))
}

fn strip_quotes(s: &str) -> &str {
    s.trim_matches(|c| c == '\'' || c == '"' || c == '`')
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    // Skip if the current file is itself a test file.
    let current_path = ctx.path.to_string_lossy();
    if is_test_path(&current_path) {
        return;
    }
    let Some(src_node) = node.child_by_field_name("source") else { return };
    let raw = src_node.utf8_text(source).unwrap_or("");
    let module = strip_quotes(raw);
    if !is_test_path(module) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-test-imports-in-prod".into(),
        message: format!(
            "Production file imports test/mock module `{module}` — move shared helpers out of the test file."
        ),
        severity: Severity::Warning,
        span: Some((node.start_byte(), node.end_byte() - node.start_byte())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_import_of_test_file_from_prod() {
        let d = run_on_path(
            "import { fixture } from './foo.test.ts';",
            "src/foo.ts",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("foo.test.ts"));
    }

    #[test]
    fn flags_import_of_spec_file_from_prod() {
        let d = run_on_path(
            "import { stub } from './bar.spec.ts';",
            "src/bar.ts",
        );
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
        let d = run_on_path(
            "import svc from './__mocks__/service';",
            "src/app.ts",
        );
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
        let d = run_on_path(
            "import { foo } from './foo';",
            "src/bar.ts",
        );
        assert!(d.is_empty());
    }
}
