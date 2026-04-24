//! dockerfile-exec-form-cmd — CMD/ENTRYPOINT shell form loses signal
//! forwarding; use exec form `["bin","arg"]`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-exec-form-cmd",
    description: "CMD/ENTRYPOINT must use exec form `[\"bin\",\"arg\"]`, not shell form.",
    remediation: "Rewrite `CMD bin arg` as `CMD [\"bin\", \"arg\"]` so the container receives SIGTERM directly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Dockerfile, Backend::Text(Box::new(text::Check)))],
    }
}
