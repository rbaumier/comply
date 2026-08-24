//! rust-no-provider-in-domain — the domain never spells a provider's name.
//!
//! In a ports-and-adapters layout the domain owns the vocabulary of the
//! business and nothing else. A declaration named after the SaaS that
//! happens to back it today (`twilio_phone_sid`, `StripeCustomerId`) welds
//! the domain to that vendor: swapping providers then means renaming types
//! and fields across every layer instead of rewriting one adapter.
//!
//! The check is scoped by path (`domain_globs`) and by an explicit vendor
//! list (`providers`), both configurable — see
//! `[rules.rust-no-provider-in-domain]` in `src/config/defaults.toml`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-provider-in-domain",
    description: "A declaration inside the domain names a provider — a port must stay vendor-agnostic.",
    remediation: "Name the declaration after the role it plays in the domain \
                  (`twilio_phone_sid` → `provisioned_number_reference`) and \
                  keep the provider's name in the adapter that talks to it. \
                  Then swapping providers rewrites one adapter instead of \
                  every layer that quotes the vendor.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "architecture"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
