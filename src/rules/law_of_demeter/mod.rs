//! law-of-demeter — max one dot deep on dependency chains.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
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
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
