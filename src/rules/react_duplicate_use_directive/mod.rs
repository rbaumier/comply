//! A file with both `"use client"` and `"use server"` at the top is
//! contradictory — only the first directive is honoured, the rest are
//! silently ignored. The resulting behaviour depends on source ordering, not
//! intent.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-duplicate-use-directive",
    description: "A file can have `\"use client\"` or `\"use server\"`, not both.",
    remediation: "Pick one: server modules get `\"use server\"`, client \
                  components get `\"use client\"`. Delete the other directive.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/directives"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
