//! api-deprecation-headers — a route handler marked `@deprecated` must
//! emit the RFC 9745 / RFC 8594 `Deprecation` and `Sunset` response
//! headers.
//!
//! Without these headers, clients have no programmatic way to learn that
//! an endpoint is on the way out. The JSDoc `@deprecated` tag only helps
//! the person reading the source; every live integration keeps hitting
//! the route until the day it 410s. Surfacing the deprecation on the
//! wire gives SDKs and monitoring a chance to react early.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-deprecation-headers",
    description: "Route handlers marked `@deprecated` must set `Deprecation` and `Sunset` response headers.",
    remediation: "Add `Deprecation: true` and `Sunset: <date>` to the handler response so clients can detect the deprecation at runtime (RFC 9745 / RFC 8594).",
    severity: Severity::Warning,
    doc_url: Some("https://datatracker.ietf.org/doc/html/rfc9745"),
    categories: &["api"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
