mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "mutex-over-atomic",
    description: "`Mutex` wrapping a primitive type — prefer an atomic type.",
    remediation: "Use `AtomicBool`, `AtomicUsize`, etc. instead of `Mutex<bool>`, `Mutex<usize>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
