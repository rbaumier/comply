mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-mutation-for-client-state",
    description: "`useMutation` is for async server state — don't use it to drive purely local state.",
    remediation: "Use `useState` / `useReducer` / a store (Zustand, Redux) for client-only state. Reserve `useMutation` for network writes.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/mutations"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
