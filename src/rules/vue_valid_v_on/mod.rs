//! vue-valid-v-on — enforce valid `v-on` directive usage in Vue templates.
//!
//! A `v-on` directive (long form `v-on:click` or shorthand `@click`) must name
//! an event, carry only known modifiers, and provide a handler expression. The
//! long form without an argument is missing its event name; any modifier that is
//! not a known event/key/system modifier is invalid; and a directive with no
//! value expression is missing its handler.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-valid-v-on",
    description: "`v-on` (or its `@` shorthand) must name an event, use only known modifiers, and provide a handler expression.",
    remediation: "Give `v-on` an event name (e.g. `v-on:click`), remove or correct unknown modifiers, and add a handler expression (e.g. `@click=\"onClick\"`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
