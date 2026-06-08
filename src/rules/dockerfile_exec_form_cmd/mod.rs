//! dockerfile-exec-form-cmd — CMD/ENTRYPOINT shell form loses signal
//! forwarding; use exec form `["bin","arg"]`.

mod typescript;

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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Dockerfile,
            Backend::TreeSitter(Box::new(typescript::Check)),
        )],
    }
}
