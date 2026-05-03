//! no-catch-without-use — flag `catch (e)` where `e` is never referenced.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-catch-without-use",
    description: "`catch (e)` binding is unused — drop the parameter or use it.",
    remediation: "If you don't need the error, use bare `catch { ... }` (ES2019). \
                  If you do need it, log/rethrow/return it so the binding pays \
                  rent. An unused catch binding hides error information.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],
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
