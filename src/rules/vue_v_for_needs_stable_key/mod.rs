//! vue-v-for-needs-stable-key — flag `:key="index"` in `v-for` loops.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-v-for-needs-stable-key",
    description: "v-for `:key` must use a stable identifier, not the loop index.",
    remediation: "Replace `:key=\"index\"` / `:key=\"i\"` with a stable id from \
                  the data: `:key=\"item.id\"`. Index keys cause Vue to reuse the \
                  wrong DOM when items reorder, filter, or get inserted.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
