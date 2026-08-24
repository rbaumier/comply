//! react-no-request-state-mirror — a hand-rolled `"idle" | "loading"` union or
//! `useState("idle")` duplicates the `status` TanStack Query already tracks.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-request-state-mirror",
    description: "Request state re-modelled as a hand-rolled union or `useState` — the query result already carries `status`.",
    remediation: "Delete the mirror and switch on the `status` of the `useQuery` / \
                  `useMutation` result (`pending` / `error` / `success`), or on its \
                  `isPending` / `isError` / `isSuccess` discriminants. A parallel copy \
                  drifts out of sync with the request it describes.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/latest/docs/framework/react/guides/queries"),
    categories: &["tanstack", "react"],

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
