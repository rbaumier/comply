//! nuxt-no-manual-fetch-in-setup

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-manual-fetch-in-setup",
    description: "Raw `fetch()` in setup duplicates the request between SSR and hydration.",
    remediation: "Use `useFetch()` or `useAsyncData()` so the response is serialized into the payload.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/getting-started/data-fetching"),
    categories: &["nuxt", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
