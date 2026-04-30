//! rust-box-dyn-error-without-send-sync — `Box<dyn Error>` without `+ Send + Sync`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-box-dyn-error-without-send-sync",
    description: "`Box<dyn Error>` without `+ Send + Sync` bounds.",
    remediation: "A bare `Box<dyn Error>` cannot cross thread boundaries — \
                  it can't be sent into a `tokio::spawn` task or returned \
                  from a function that's awaited on another runtime worker. \
                  Use `Box<dyn Error + Send + Sync + 'static>` (or the \
                  `anyhow::Error` alias).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
