//! no-manual-rtl-cleanup — `cleanup()` is automatic with Vitest.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-manual-rtl-cleanup",
    description: "Importing `cleanup` from `@testing-library/react` in Vitest causes double cleanup.",
    remediation: "Remove the `cleanup` import and any `afterEach(cleanup)` \
                  call. Vitest with `@testing-library/react` runs cleanup \
                  automatically after each test. Manual cleanup causes \
                  double cleanup which can mask unmount bugs.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
