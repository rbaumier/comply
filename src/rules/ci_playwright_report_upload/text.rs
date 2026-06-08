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

crate::ast_check! { on ["block_mapping_pair"] prefilter = ["actions/upload-artifact"] => |node, source, ctx, diagnostics|
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
        path: std::sync::Arc::clone(&ctx.path_arc),
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, ".github/workflows/e2e.yml")
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
