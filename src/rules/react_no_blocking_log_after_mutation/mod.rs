//! react-no-blocking-log-after-mutation — `await log()`/`await track()` after a main `await`.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-blocking-log-after-mutation",
    description: "Awaiting a telemetry/log call after a main mutation in a server action \
                  delays the response for every request.",
    remediation: "Fire-and-forget the telemetry call (drop the `await`) or schedule it \
                  via `after()` / `waitUntil()` so the response ships first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: false,
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
