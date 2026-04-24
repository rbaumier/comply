//! Flag workflows that run Playwright but never upload the `playwright-report/` folder.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};

fn is_playwright_run(cmd: &str) -> bool {
    let lower = cmd.to_ascii_lowercase();
    lower.contains("playwright test")
        || lower.contains("playwright install")
        || lower.contains("@playwright/test")
}

/// Scan the YAML tree for a step that uses `actions/upload-artifact` with a
/// `path:` or `name:` referencing `playwright-report`. A minimal source-level
/// check is sufficient here and cheaper than a second AST walk — tree-sitter
/// already validated the file parses.
fn uploads_playwright_report(source: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !line.contains("actions/upload-artifact") {
            continue;
        }
        for next in lines.iter().skip(i + 1).take(15) {
            if next.contains("playwright-report") {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "block_mapping_pair" { return; }
    if pair_key_text(node, source).as_deref() != Some("run") { return; }
    let Some(cmd) = pair_scalar_value(node, source) else { return; };
    if !is_playwright_run(&cmd) { return; }
    // Only emit the diagnostic on the first Playwright `run:` we encounter.
    // Subsequent matches would produce duplicate file-level warnings.
    if !diagnostics.is_empty() { return; }
    if uploads_playwright_report(ctx.source) { return; }

    let Some(value_node) = node.named_child(1) else { return; };
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: 1,
        rule_id: "ci-playwright-report-upload".into(),
        message: "Playwright runs without uploading `playwright-report/` — add an \
                  `actions/upload-artifact@v4` step with `if: failure()` to preserve \
                  traces and screenshots."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_yaml_with_path;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml_with_path(source, &Check, ".github/workflows/e2e.yml")
    }

    #[test]
    fn flags_missing_upload() {
        let yaml = "\
on: push
jobs:
  e2e:
    steps:
      - run: npx playwright test
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_when_upload_present() {
        let yaml = "\
on: push
jobs:
  e2e:
    steps:
      - run: npx playwright test
      - uses: actions/upload-artifact@v4
        if: failure()
        with:
          name: playwright-report
          path: playwright-report/
";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_workflows_without_playwright() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - run: npm ci";
        assert!(run(yaml).is_empty());
    }
}
