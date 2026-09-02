//! rust-no-print-macros — `println!` bypasses the log pipeline.
//!
//! Doc-only marker rule. Equivalent to `clippy::print_stdout`
//! (restriction group, allow by default — binding it here is what turns
//! it on). It writes straight to the process stream: no level, no
//! target, no span, nothing a subscriber can filter or redirect, and an
//! unconditional lock on every call. The lint fires everywhere, binaries
//! included, so a CLI that legitimately owns its stdout needs the escape
//! hatch spelled out in the remediation. `eprintln!` is deliberately not
//! bound: `rust-eprintln-in-library` owns it with the build-script,
//! proc-macro and binary exemptions a clippy binding cannot express. comply registers the rule for documentation
//! parity with the rest of the Rust catalog but defers enforcement to
//! clippy.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-print-macros",
    description: "`println!` writes past the logging pipeline.",
    remediation: "Outside a binary's own output path, replace `println!` with a `tracing` macro (`tracing::info!`, \
                  `tracing::warn!`, `tracing::error!`) so the consumer's \
                  subscriber decides the level, the target and the sink. \
                  In a CLI, where stdout is the product, write through \
                  `std::io::stdout().lock()` + `writeln!` — it is buffered \
                  and the lint does not fire on it — or put \
                  `#[expect(clippy::print_stdout, reason = \"CLI output\")]` \
                  on `main`. Enforced by `clippy::print_stdout`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy {
                lint: "clippy::print_stdout",
            },
        )],
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Severity;
    use crate::rules::test_helpers::assert_clippy_rule;

    use super::*;

    #[test]
    fn registers_print_stdout_only() {
        assert_clippy_rule(
            register(),
            "rust-no-print-macros",
            Severity::Error,
            &["clippy::print_stdout"],
        );
    }

    #[test]
    fn remediation_names_the_lint_and_the_cli_escape_hatch() {
        assert!(META.remediation.contains("clippy::print_stdout"));
        assert!(META.remediation.contains("std::io::stdout().lock()"));
        assert!(META.remediation.contains("#[expect(clippy::print_stdout"));
        assert_eq!(META.categories, &["rust"]);
    }
}
