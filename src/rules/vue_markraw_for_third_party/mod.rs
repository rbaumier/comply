//! vue-markraw-for-third-party — wrap third-party instances in `markRaw()`
//! so Vue does not deeply reactify them.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-markraw-for-third-party",
    description: "Wrap third-party instances (Chart.js, maps, editors, ...) in `markRaw()`.",
    remediation: "Storing a third-party object in a `ref()` or `reactive()` makes Vue \
                  walk its entire internal state with Proxies — that breaks libraries \
                  and murders performance. Wrap the instance: \
                  `chart.value = markRaw(new Chart(...))`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
