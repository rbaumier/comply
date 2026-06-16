//! use-qwik-loader-location — enforce that Qwik loader/action functions are
//! declared correctly.
//!
//! Qwik's `routeLoader$` and `routeAction$` are tied to a route and only run
//! when declared in a route boundary file (`index`, `layout`, or `plugin`
//! inside `src/routes`). Every loader/action (`routeLoader$`, `routeAction$`,
//! `globalAction$`) must additionally be exported under a `use*` name and
//! receive an inline arrow function rather than a reference, so the optimizer
//! can keep server code out of the client bundle.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-qwik-loader-location",
    description: "Qwik loader/action functions must be in a route boundary file, exported with a `use*` name, and given an inline arrow function.",
    remediation: "Declare route loaders in an `index`/`layout`/`plugin` file under `src/routes`, export them with a `use*` name, and pass an inline arrow function.",
    severity: Severity::Warning,
    doc_url: Some("https://qwik.dev/docs/route-loader/"),
    categories: &["qwik"],

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
