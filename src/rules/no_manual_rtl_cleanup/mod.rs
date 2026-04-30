//! no-manual-rtl-cleanup — `cleanup()` is automatic with Vitest.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-manual-rtl-cleanup",
    description: "Importing `cleanup` from `@testing-library/react` in Vitest causes double cleanup.",
    remediation: "Remove the `cleanup` import and any `afterEach(cleanup)` \
                  call. Vitest with `@testing-library/react` runs cleanup \
                  automatically after each test. Manual cleanup causes \
                  double cleanup which can mask unmount bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
