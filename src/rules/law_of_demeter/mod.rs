//! law-of-demeter — max one dot deep on dependency chains.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
