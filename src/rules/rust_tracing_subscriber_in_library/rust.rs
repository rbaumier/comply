//! rust-tracing-subscriber-in-library backend.
//!
//! Walks `call_expression` nodes and flags the ones that install a
//! process-global collector from library code. Two call shapes are recognised:
//!
//! - a **path call** whose final segment is `init` / `try_init` and whose path
//!   carries a subscriber-builder root (`tracing_subscriber::fmt::init()`,
//!   `env_logger::try_init()`, `fmt::init()`), or whose final segment is
//!   `set_global_default` / `set_logger` / `set_boxed_logger` whatever its
//!   qualification (`tracing::subscriber::set_global_default(s)`, a bare
//!   `set_boxed_logger(b)` after a `use`);
//! - a **builder chain** ending in `.init()` / `.try_init()` whose root receiver
//!   is a subscriber-builder entry point — `tracing_subscriber::fmt()`,
//!   `tracing_subscriber::registry()`, `Registry::default()`,
//!   `env_logger::Builder::new()` — however many `.with(…)` layers sit between.
//!
//! `set_global_default` / `set_logger` / `set_boxed_logger` fire only as a path
//! call, never as a method: `config.set_logger(sink)` configures one object,
//! whereas `log::set_logger(&LOGGER)` claims the process-wide slot.
//!
//! A logging/tracing infrastructure crate is exempt too: `tracing-subscriber`'s
//! own `SubscriberInitExt::try_init` calls `dispatcher::set_global_default`
//! because installing the subscriber IS what that crate ships. The match is on
//! the crate's own package name, so an application that merely depends on
//! `tracing` is not covered by it.
//!
//! Everything outside a Rust library target is exempt through
//! [`is_library_code`]: a `#[test]` / `#[tokio::test]` function, a `#[cfg(test)]`
//! scope or a `tests/` file (installing a subscriber per test is the normal way
//! to see test output), `main.rs` / `src/bin/*.rs`, any file of a crate that
//! declares a binary, a build script, a proc-macro or codegen crate, and a
//! cdylib/staticlib FFI bridge — the last because a foreign runtime loads it
//! directly, so it *is* the top of the process and nobody above it can install
//! the subscriber.
//!
//! A library may still legitimately *offer* setup, as long as the consumer opts
//! in by calling it. So a call inside a function named `init_tracing`,
//! `init_logging`, `setup_tracing` or `setup_logging` is exempt — but only when
//! that function is `pub`. A `pub` one is an opt-in helper the binary invokes
//! deliberately; a private or `pub(crate)` one with the same name is the library
//! installing the subscriber for itself behind the consumer's back, which is
//! exactly the bug, so it stays flagged.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{enclosing_fn, is_library_code, is_pub, rust_path_segments};

/// Path segments that identify a subscriber/logger builder entry point. A
/// `.init()` is only an installation when the chain it terminates starts at one
/// of these — otherwise `.init()` is just a common method name.
const SUBSCRIBER_ROOTS: &[&str] = &[
    "tracing_subscriber",
    "env_logger",
    // The bare forms left by `use tracing_subscriber::{fmt, registry, Registry};`.
    "fmt",
    "registry",
    "Registry",
];

/// Final path segments that install a process-global collector outright, with
/// no builder chain: `tracing`'s `set_global_default` and `log`'s two setters.
const GLOBAL_SETTERS: &[&str] = &["set_global_default", "set_logger", "set_boxed_logger"];

/// The two terminal methods of a subscriber-builder chain. `try_init` returns a
/// `Result` instead of panicking, but claims the same process-global slot.
const INIT_METHODS: &[&str] = &["init", "try_init"];

/// Names of the opt-in setup helper a library may expose for its consumer to
/// call. Exempt only when the function carrying the name is `pub`.
const OPT_IN_INIT_FN_NAMES: &[&str] = &[
    "init_tracing",
    "init_logging",
    "setup_tracing",
    "setup_logging",
];

crate::ast_check! {
    on ["call_expression"]
    prefilter = ["tracing_subscriber", "env_logger", "set_global_default", "set_logger", "set_boxed_logger"]
    => |node, source, ctx, diagnostics|

    let Some(function) = node.child_by_field_name("function") else { return; };
    let Some(installer) = installer_name(function, source) else { return; };
    if !is_library_code(node, source, ctx) { return; }
    // A logging/tracing crate (`tracing-subscriber`, `env_logger`, …) IS the
    // install machinery: `SubscriberInitExt::try_init` calling
    // `dispatcher::set_global_default` is the feature, not a hijack.
    if ctx
        .project
        .nearest_cargo_manifest(ctx.path)
        .is_some_and(|manifest| manifest.is_logging_infra_crate())
    {
        return;
    }
    if is_in_public_opt_in_initializer(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{installer}` installs a process-global subscriber from library code — \
             it is install-once, so it silently overrides (or is overridden by) the \
             application's own setup. Emit `tracing::info!` / `tracing::error!` here \
             and let the consumer's `main` install the subscriber."
        ),
        Severity::Error,
    ));
}

/// The name to quote in the diagnostic when `function` (a `call_expression`'s
/// callee) installs a process-global collector, or `None` when it does not.
fn installer_name(function: Node, source: &[u8]) -> Option<String> {
    match function.kind() {
        // `tracing_subscriber::fmt().with(…).init()` — a method call, whose
        // callee the grammar spells as a `field_expression`.
        "field_expression" => {
            let method = function
                .child_by_field_name("field")?
                .utf8_text(source)
                .ok()?;
            if !INIT_METHODS.contains(&method) {
                return None;
            }
            let receiver = function.child_by_field_name("value")?;
            let root = rust_path_segments(chain_root(receiver), source);
            root.iter()
                .any(|segment| SUBSCRIBER_ROOTS.contains(&segment.as_str()))
                .then(|| format!("{}(…).{method}()", root.join("::")))
        }
        _ => {
            let segments = rust_path_segments(function, source);
            let last = segments.last()?.as_str();
            // A global setter is unmistakable by name alone, whatever its
            // qualification — including the bare form left by a `use`.
            if GLOBAL_SETTERS.contains(&last) {
                return Some(format!("{}()", segments.join("::")));
            }
            // `init` / `try_init` on its own is far too common a name, so the
            // path must also name a subscriber crate or builder module.
            if INIT_METHODS.contains(&last)
                && segments
                    .iter()
                    .any(|segment| SUBSCRIBER_ROOTS.contains(&segment.as_str()))
            {
                return Some(format!("{}()", segments.join("::")));
            }
            None
        }
    }
}

/// The receiver a method chain ultimately starts from: peel `.method()` calls,
/// field accesses and `?` until nothing is left to unwrap. For
/// `tracing_subscriber::fmt().with_env_filter(f).init()` the chain root is the
/// `tracing_subscriber::fmt` path, which is what tells the chain apart from an
/// unrelated `.init()`.
fn chain_root(node: Node) -> Node {
    let mut current = node;
    loop {
        let next = match current.kind() {
            "call_expression" | "generic_function" => current.child_by_field_name("function"),
            "field_expression" => current.child_by_field_name("value"),
            "try_expression" | "parenthesized_expression" | "reference_expression" => {
                current.named_child(0)
            }
            _ => None,
        };
        match next {
            Some(inner) => current = inner,
            None => return current,
        }
    }
}

/// True when the call sits in a `pub` setup helper the consumer opts into by
/// calling it (`pub fn init_tracing()`). The `pub` requirement is the whole
/// point: a private helper of the same name installs the subscriber on the
/// consumer's behalf without being asked, which is the bug this rule reports.
fn is_in_public_opt_in_initializer(node: Node, source: &[u8]) -> bool {
    let Some(function) = enclosing_fn(node) else {
        return false;
    };
    let named_as_initializer = function
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .is_some_and(|name| OPT_IN_INIT_FN_NAMES.contains(&name));
    named_as_initializer && is_pub(function, source)
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const LIB_CARGO_TOML: &str = "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n\
        edition = \"2021\"\n\n[lib]\nname = \"mylib\"\npath = \"src/lib.rs\"\n";

    const BIN_CARGO_TOML: &str = "[package]\nname = \"mytool\"\nversion = \"0.1.0\"\n\
        edition = \"2021\"\n\n[[bin]]\nname = \"mytool\"\npath = \"src/main.rs\"\n";

    const CDYLIB_CARGO_TOML: &str = "[package]\nname = \"pybridge\"\nversion = \"0.1.0\"\n\
        edition = \"2021\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n";

    /// Run on `rel_path` inside a temp crate carrying `cargo_toml`, so the
    /// library/application classification resolves against a controlled
    /// manifest instead of comply's own (binary) `Cargo.toml`.
    fn run_in_crate(cargo_toml: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();
        let src_path = dir.path().join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    fn run_in_lib(source: &str) -> Vec<Diagnostic> {
        run_in_crate(LIB_CARGO_TOML, "src/telemetry.rs", source)
    }

    #[test]
    fn flags_tracing_subscriber_fmt_init_chain() {
        let source = "pub fn start() { tracing_subscriber::fmt().init(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn flags_tracing_subscriber_fmt_init_path_call() {
        let source = "pub fn start() { tracing_subscriber::fmt::init(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn flags_try_init_on_builder_chain() {
        let source = "use tracing_subscriber::fmt;\n\
             pub fn start() { fmt().with_target(false).try_init().ok(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn flags_registry_chain_init() {
        let source = "pub fn start() { tracing_subscriber::registry().with(layer).init(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn flags_set_global_default() {
        let source = "use tracing_subscriber::Registry;\n\
             pub fn start(s: Registry) { tracing::subscriber::set_global_default(s).unwrap(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    /// A bare `set_global_default(…)` left by `use tracing::subscriber::*;` is
    /// the same call — the name alone identifies it.
    #[test]
    fn flags_bare_set_global_default() {
        let source = "use tracing_subscriber::Registry;\n\
             use tracing::subscriber::set_global_default;\n\
             pub fn start(s: Registry) { let _ = set_global_default(s); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn flags_env_logger_init() {
        assert_eq!(
            run_in_lib("pub fn start() { env_logger::init(); }").len(),
            1
        );
    }

    #[test]
    fn flags_log_set_boxed_logger() {
        let source = "use env_logger::Logger;\n\
             pub fn start(l: Box<Logger>) { log::set_boxed_logger(l).unwrap(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn flags_log_set_logger() {
        let source = "use env_logger::Logger;\n\
             pub fn start(l: &'static Logger) { log::set_logger(l).unwrap(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    /// A private helper named like an opt-in initializer still installs the
    /// subscriber behind the consumer's back.
    #[test]
    fn flags_private_init_tracing_helper() {
        let source = "fn init_tracing() { tracing_subscriber::fmt().init(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    /// `pub(crate)` is not an opt-in surface either: no consumer can call it.
    #[test]
    fn flags_pub_crate_init_tracing_helper() {
        let source = "pub(crate) fn init_tracing() { tracing_subscriber::fmt().init(); }";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    /// The documented escape hatch: a `pub` setup helper the binary calls.
    #[test]
    fn allows_pub_init_tracing_helper() {
        let source = "pub fn init_tracing() { tracing_subscriber::fmt().init(); }";
        assert!(run_in_lib(source).is_empty());
    }

    #[test]
    fn allows_pub_setup_logging_helper() {
        let source = "pub fn setup_logging() { env_logger::init(); }";
        assert!(run_in_lib(source).is_empty());
    }

    /// Emitting events is what a library is supposed to do.
    #[test]
    fn allows_emitting_tracing_events() {
        let source = "use tracing_subscriber::Registry;\n\
             pub fn work() { tracing::info!(\"done\"); }";
        assert!(run_in_lib(source).is_empty());
    }

    /// `.init()` on an unrelated builder is a plain method name, not an install.
    #[test]
    fn allows_unrelated_init_method() {
        let source = "use tracing_subscriber::Registry;\n\
             pub fn start(app: App) { app.builder().init(); }";
        assert!(run_in_lib(source).is_empty());
    }

    /// A `set_logger` *method* configures one object; only the free function
    /// claims the process-wide slot.
    #[test]
    fn allows_set_logger_method_on_a_value() {
        let source = "use env_logger::Logger;\n\
             pub fn configure(cfg: &mut Config, l: Logger) { cfg.set_logger(l); }";
        assert!(run_in_lib(source).is_empty());
    }

    /// A crate-local `init()` that merely names the crate elsewhere in the file
    /// must not be mistaken for a subscriber installation.
    #[test]
    fn allows_crate_local_init_call() {
        let source = "use tracing_subscriber::Registry;\n\
             pub fn start() { crate::state::init(); }";
        assert!(run_in_lib(source).is_empty());
    }

    #[test]
    fn allows_subscriber_init_in_test_function() {
        let source = "#[test]\nfn t() { tracing_subscriber::fmt().init(); }";
        assert!(run_in_lib(source).is_empty());
    }

    #[test]
    fn allows_subscriber_init_in_tokio_test_function() {
        let source = "#[tokio::test]\nasync fn t() { tracing_subscriber::fmt().init(); }";
        assert!(run_in_lib(source).is_empty());
    }

    #[test]
    fn allows_subscriber_init_in_main() {
        let source = "fn main() { tracing_subscriber::fmt().init(); }";
        assert!(run_in_crate(BIN_CARGO_TOML, "src/main.rs", source).is_empty());
    }

    /// Every file of a crate that ships a binary belongs to an application,
    /// even one that also carries a `[lib]`.
    #[test]
    fn allows_subscriber_init_in_binary_crate_module() {
        let source = "pub fn start() { tracing_subscriber::fmt().init(); }";
        assert!(run_in_crate(BIN_CARGO_TOML, "src/telemetry.rs", source).is_empty());
    }

    /// A cdylib bridge is loaded by a foreign runtime: nobody above it can
    /// install the subscriber, so it must do it itself.
    #[test]
    fn allows_subscriber_init_in_ffi_bridge_crate() {
        let source = "pub fn start() { tracing_subscriber::fmt().init(); }";
        assert!(run_in_crate(CDYLIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// `tracing-subscriber` installing the global subscriber is the feature it
    /// ships, not a library hijacking the application's setup.
    #[test]
    fn allows_global_default_in_a_logging_infrastructure_crate() {
        let cargo_toml = "[package]\nname = \"tracing-subscriber\"\nversion = \"0.3.20\"\n\
            edition = \"2021\"\n";
        let source = "pub fn try_init(s: Registry) { \
             tracing::subscriber::set_global_default(s).unwrap(); }";
        assert!(run_in_crate(cargo_toml, "src/util.rs", source).is_empty());
    }

    #[test]
    fn allows_subscriber_init_in_integration_test_file() {
        let source = "fn setup() { tracing_subscriber::fmt().init(); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "tests/it.rs", source).is_empty());
    }
}
