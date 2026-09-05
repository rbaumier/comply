//! expression-complexity — flag an expression that joins its operands with
//! too many logical/conditional operators.
//!
//! Both backends count operator *nodes*, never source bytes, so operator
//! characters inside a literal, a comment or a macro token tree contribute
//! nothing. What one diagnostic covers differs per language: the Rust backend
//! reports one chain — the outermost `&&` / `||` expression — while the oxc
//! backend groups the operators of a source line.

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "expression-complexity",
    description: "Overly complex expression with too many logical/conditional operators.",
    remediation: "Extract parts of the expression into named intermediate variables. Expressions with 4+ logical/conditional operators are hard to read and reason about.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
