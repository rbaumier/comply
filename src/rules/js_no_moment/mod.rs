//! js-no-moment — moment.js is 300kB+; prefer `date-fns`, `dayjs`, or
//! the native `Temporal` API.
//!
//! The harm is moment.js bloating the production bundle, so only runtime/source
//! adoption is flagged. Test files (`skip_in_test_dir`) never ship to production
//! — moment imported there (e.g. as a parity oracle in a date-library's own test
//! suite) is out of scope and not flagged.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "js-no-moment",
    description: "moment.js is 300kB+ — use `date-fns`, `dayjs`, or `Temporal` instead.",
    remediation: "Replace `moment` with a smaller library (`date-fns`, `dayjs`) or the \
                  native `Temporal` API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["bundle-size"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
