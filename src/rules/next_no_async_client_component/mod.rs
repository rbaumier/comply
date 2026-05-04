//! next-no-async-client-component

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-async-client-component",
    description: "Client components cannot be `async` — they must be synchronous.",
    remediation: "Drop `async`, fetch via `useEffect`/`useSWR`/`useQuery`, or convert this file to a server component.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/use-client"),
    categories: &["nextjs", "rsc"],
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
