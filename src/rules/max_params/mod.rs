//! max-params — enforce a maximum number of parameters in function definitions.
//!
//! Native implementation. TS/JS/TSX use the OXC AST so the rule can exempt
//! callbacks whose signatures are dictated by third-party library types
//! (TanStack Query's `useMutation({ onError: (e, v, c, m) => … })` and
//! friends). Rust delegation to clippy's `too_many_arguments` is unchanged.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "max-params",
    description: "Functions should take at most 3 positional arguments.",
    remediation: "If you need more than 3 parameters, pack them into an \
                  options object — named fields carry intent where \
                  positional arguments don't.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::Clippy { lint: "clippy::too_many_arguments" }),
        ],
    }
}
