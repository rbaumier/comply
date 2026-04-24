//! tanstack-start-route-protection-beforeload — protected routes should
//! gate with `beforeLoad` + `throw redirect()`, not `useEffect` + `navigate`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-route-protection-beforeload",
    description: "Protect routes with `beforeLoad` + `throw redirect()`, not \
                  `useEffect` + `navigate`.",
    remediation: "Move the auth check to `beforeLoad` and `throw redirect({ to: '/login' })`. \
                  This runs before render and avoids the protected UI flashing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
