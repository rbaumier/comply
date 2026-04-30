//! prefer-spy-on — prefer `vi.spyOn`/`jest.spyOn` over reassigning methods
//! with `vi.fn()`/`jest.fn()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-spy-on",
    description: "Reassigning `obj.method = vi.fn()`/`jest.fn()` replaces the original implementation \
         and is harder to restore than a spy.",
    remediation: "Use vi.spyOn(obj, 'method') instead of reassigning to vi.fn()",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
