//! law-of-demeter — max one dot deep on dependency chains.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "law-of-demeter",
    description: "Chained member access couples the caller to the entire object graph.",
    remediation: "Add a direct accessor on the immediate dependency. \
                  `order.getCustomer().getAddress().getCity()` → expose \
                  `order.shippingCity()`. The caller shouldn't know how \
                  Customer and Address are structured.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
