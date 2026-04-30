//! shadcn-avatar-requires-fallback — every `<Avatar>` must render an
//! `<AvatarFallback>` so the UI never renders a broken image.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-avatar-requires-fallback",
    description: "`<Avatar>` must contain an `<AvatarFallback>` so broken images degrade gracefully.",
    remediation: "Add an `<AvatarFallback>` child (initials, icon, or empty circle) alongside `<AvatarImage>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
