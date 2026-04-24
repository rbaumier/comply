//! vue-ref-value-in-script — flag reading a `ref()` in `<script>` without `.value`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-ref-value-in-script",
    description: "Reading a `ref()` in `<script>` without `.value` compares the Ref object, not its value.",
    remediation: "Access `.value` on refs inside `<script>`: `if (count.value > 0)`. Auto-unwrapping only happens in templates.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
