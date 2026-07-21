//! axum-cors-regex-unanchored

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-cors-regex-unanchored",
    description: "A CORS origin regex used in an `AllowOrigin::predicate` closure without a trailing `$` anchor matches more than intended (e.g. `https://good.example.com.attacker.com`).",
    remediation: "Anchor the origin regex at the end with `$`: `Regex::new(r\"^https://.*\\.example\\.com$\")`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
