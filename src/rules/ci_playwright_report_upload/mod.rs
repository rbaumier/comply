//! ci-playwright-report-upload

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ci-playwright-report-upload",
    description: "A workflow that runs Playwright but never uploads `playwright-report/` \
                  gives no way to debug failed E2E runs — traces, screenshots and \
                  videos are lost with the runner.",
    remediation: "Add a step using `actions/upload-artifact@v4` that uploads \
                  `playwright-report/` with `if: failure()` (or `if: always()`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ci-cd"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
