#![allow(dead_code)] // Rules consumed by engine::run_custom_rules (task 12).

//! Custom lint rules — each rule implements the Rule trait and is registered
//! in `all_rules()`. The engine calls every rule on every file whose language
//! matches.

pub mod max_file_lines;

use crate::diagnostic::Diagnostic;
use crate::files::Language;
use std::path::Path;

/// A lint rule that operates on source code.
pub trait Rule {
    /// Unique rule identifier (e.g., "max-file-lines").
    fn id(&self) -> &'static str;

    /// Which languages this rule applies to.
    fn languages(&self) -> &[Language];

    /// Run the rule on source code and return any violations.
    fn check(&self, path: &Path, source: &str, language: Language) -> Vec<Diagnostic>;
}

/// All registered custom rules. Add new rules here.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![Box::new(max_file_lines::MaxFileLines)]
}
