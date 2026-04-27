//! elysia-after-response-mutation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-after-response-mutation",
    description: "`onAfterResponse` handler tries to mutate the response after it has already been sent.",
    remediation: "Move header / status changes to `onBeforeHandle`, `mapResponse`, or `transform`. `onAfterResponse` runs after the response is flushed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
