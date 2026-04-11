//! law-of-demeter — max one dot deep on dependency chains.
//!
//! TypeScript-only on purpose. The rule's chain-depth heuristic was
//! designed for OO codebases with getter conventions
//! (`order.getCustomer().getAddress().getCity()`). In idiomatic Rust the
//! same syntactic pattern is overwhelmingly:
//!
//! - Builder pattern: `Command::new().arg().output()`
//! - Result/Option monad: `result.map().and_then().unwrap_or_else()`
//! - Iterator pipeline: `vec.iter().filter().map().collect()`
//! - PathBuf composition: `path.parent()?.join("x").canonicalize()`
//! - tree-sitter navigation: `node.child_by_field_name("body")?.kind()`
//!
//! None of those are object-graph reach-through. Real Demeter violations
//! in Rust would be field-access chains (`order.customer.address.city`),
//! which are rare because Rust doesn't have implicit getters — code uses
//! pattern matching, destructuring, and `&self` references instead. A
//! Rust backend that flags chain depth would be 99% noise; a backend
//! that flags field chains specifically would have almost nothing to
//! catch. Either way, the rule's premise doesn't transfer.
//!
//! Decoupling discipline in Rust is enforced by the type system, the
//! borrow checker, and trait design — not by counting dots.

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
};pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
