//! rust-orphan-module — a `.rs` file under `src/` that no `mod` declaration loads.
//!
//! rustc never compiles such a file: its code and tests are dead while grep
//! still finds them. The usual cause is a file created without its `mod foo;`.

mod text;

pub(crate) use text::{declaring_dir, module_name};

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-orphan-module",
    description: "Rust source file that no `mod` declaration loads.",
    remediation: "Declare it from the parent module (`mod foo;`) or delete the file.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::Text(Box::new(text::Check)))],
    }
}
