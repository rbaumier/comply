//! rust-eprintln-in-library backend.
//!
//! Walks `macro_invocation` nodes for `eprintln!` / `eprint!` and
//! flags any invocation that:
//!
//! - is **not** in test context (`#[test]` / `#[cfg(test)]` /
//!   `tests/` integration directory), and
//! - is **not** in a Cargo build script (`build.rs`), and
//! - is **not** in a binary file (`main.rs`, `src/bin/*.rs`), and
//! - is **not** in a file declared as an explicit-path executable target
//!   in the nearest `Cargo.toml` (a `[[bin]]`/`[[example]]`/`[[bench]]`/
//!   `[[test]]` table with `path = "utils/foo.rs"`), and
//! - is **not** in a crate that declares a binary (the nearest
//!   `Cargo.toml` declares a `[[bin]]` target or a `src/main.rs`
//!   exists next to it), and
//! - is **not** in a build-time codegen crate (the nearest `Cargo.toml`
//!   `[package].name` ends with `-build`/`-codegen`/`-bindgen` or the
//!   `_` variants), and
//! - is **not** in an FFI bridge crate (the nearest `Cargo.toml`
//!   `[lib] crate-type` declares `cdylib`/`staticlib` and no `rlib`/`lib`), and
//! - is **not** in the `then` branch of an `if` gated by a
//!   verbose/debug-style flag (`if self.verbose() { eprintln!(...) }`).
//!
//! `eprintln!` is fine in CLI binaries — that's where it belongs.
//! It's a problem in libraries because consumers can't redirect or
//! capture it. A crate that ships a binary is an application: every
//! one of its source files is exempt, not just the entry points —
//! even when it also carries a `[lib]` purely to expose internals to
//! its own integration tests (the `lib.rs` + `main.rs` split).
//!
//! A file declared as an explicit-path executable target — a `[[bin]]`,
//! `[[example]]`, `[[bench]]`, or `[[test]]` table with `path = "utils/foo.rs"`
//! — is itself a standalone binary with its own `fn main()` that Cargo compiles
//! and runs directly. It is application code even when it lives outside the
//! conventional `src/main.rs` / `src/bin/` locations, so its `eprintln!` is
//! exempt regardless of whether the surrounding crate also ships a library.
//!
//! A Cargo build script (`build.rs`) is a separate binary that Cargo
//! compiles and runs at build time, not the crate's runtime library code.
//! Cargo captures and displays its stderr, so `eprintln!` is the idiomatic
//! build-script diagnostic channel — it is exempt.
//!
//! A build-time codegen crate (a `-build`/`-codegen`/`-bindgen` library
//! such as `prost-build` or `tonic-build`) is consumed from a `build.rs`
//! script, where writing to Cargo's build-output stream via `eprintln!` /
//! `println!` is the idiomatic diagnostic channel — tracing/log is
//! unavailable there — so its `eprintln!` is exempt too.
//!
//! An FFI bridge crate (a `[lib] crate-type` of `cdylib`/`staticlib` with no
//! `rlib`/`lib`, such as Python/Java/Swift bindings) is linked into a foreign
//! runtime, not consumed as a Rust library. That runtime never initialises a
//! Rust tracing subscriber, so there is no `tracing::warn!` alternative —
//! `eprintln!` is the only practical way to surface errors at the FFI boundary,
//! so it is exempt too.
//!
//! A logging/tracing infrastructure crate (the nearest `Cargo.toml`
//! `[package].name` is a known logging crate such as `tracing` /
//! `tracing-subscriber` / `env_logger`, or carries a `tracing` / `logger` /
//! `logging` / `slog` segment) implements the
//! `Subscriber` / `Log` machinery itself. It cannot route its own internal
//! failures through `tracing::warn!` / `log::error!` — that is the very system
//! that has failed (a log-file rotation error, a formatter bug, a
//! `RUST_LOG` parse error) or would recurse — so `eprintln!` is its
//! legitimate last-resort fallback output and is exempt. The match is on the
//! crate's own identity, not on whether it depends on a logging crate, so an
//! application that merely uses `tracing` stays flagged.
//!
//! Output gated behind a runtime verbosity flag is opt-in diagnostics,
//! not unconditional library noise: the consumer only sees it after
//! turning the flag on. The guard is recognised when the `if` condition
//! is either:
//!
//! - a *simple* flag reference — a bare identifier, a field access, or a
//!   no-argument method call — whose final segment names a known flag
//!   (`verbose`, `debug`, `quiet`, `trace`, …), or
//! - an environment-variable-presence check — `env::var(KEY).is_ok()` or
//!   `env::var_os(KEY).is_some()` (with or without a `std::` prefix). The
//!   `eprintln!` is unreachable unless the consumer has set the variable,
//!   so it is a runtime opt-in just like a verbosity flag.
//!
//! Negated, compared, or compound conditions stay flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;
use std::path::Path;

const KINDS: &[&str] = &["macro_invocation"];

/// Final-segment names that mark an `if` condition as a runtime
/// verbose/debug flag. Kept deliberately small: only output gated by a
/// recognised diagnostics flag is opt-in, everything else stays flagged.
const VERBOSE_FLAG_NAMES: &[&str] = &[
    "verbose",
    "debug",
    "quiet",
    "trace",
    "is_verbose",
    "is_debug",
    "is_quiet",
    "is_trace",
    "debug_mode",
    "verbose_mode",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_name) = node.child_by_field_name("macro") else {
            return;
        };
        let name = macro_name.utf8_text(source_bytes).unwrap_or("");
        let bare = name.rsplit("::").next().unwrap_or(name);
        if bare != "eprintln" && bare != "eprint" {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        if crate::rules::path_utils::is_rust_build_script(ctx.path) {
            return;
        }
        if is_binary_file(ctx.path) {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.declares_binary() || m.declares_executable_at(ctx.path))
        {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_build_codegen_crate())
        {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_ffi_bridge_crate())
        {
            return;
        }
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_logging_infra_crate())
        {
            return;
        }
        if is_under_verbose_flag_guard(node, source_bytes) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-eprintln-in-library",
            format!(
                "`{bare}!` writes to stderr directly — library consumers \
                 can't redirect, configure, or capture it. Use \
                 `tracing::warn!` / `tracing::error!` instead."
            ),
            Severity::Warning,
        ));
    }
}

fn is_under_tests_dir(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "tests")
}

/// True if `path` is a binary entry point: `main.rs` at any directory
/// level, or any file under a `bin/` directory.
fn is_binary_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && name == "main.rs"
    {
        return true;
    }
    path.components().any(|c| c.as_os_str() == "bin")
}

/// True when `node` sits in the `then` branch of an enclosing `if`
/// whose condition is a simple verbose/debug-flag reference. Walks
/// every ancestor `if_expression` (so a flag guard several blocks up
/// still exempts), requiring the node to be inside the `consequence`
/// (not the `else`) of at least one of them.
fn is_under_verbose_flag_guard(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "if_expression"
            && let Some(consequence) = parent.child_by_field_name("consequence")
            && is_descendant_of(node, consequence)
            && parent
                .child_by_field_name("condition")
                .is_some_and(|cond| is_verbose_flag_condition(cond, source))
        {
            return true;
        }
        current = parent;
    }
    false
}

/// True if `node` is `ancestor` or nested anywhere inside it.
fn is_descendant_of(node: tree_sitter::Node, ancestor: tree_sitter::Node) -> bool {
    let mut current = Some(node);
    while let Some(n) = current {
        if n == ancestor {
            return true;
        }
        current = n.parent();
    }
    false
}

/// True when `cond` is a recognised runtime opt-in guard: either a
/// *simple* flag reference (a bare identifier, a field access, or a
/// no-argument method call) whose final path segment is a known
/// verbose/debug flag name, or an environment-variable-presence check
/// (`env::var(KEY).is_ok()` / `env::var_os(KEY).is_some()`). Negation
/// (`!self.verbose()`), comparison, or any other compound expression
/// returns false — those are not plain "is the flag on" guards and stay
/// flagged.
fn is_verbose_flag_condition(cond: tree_sitter::Node, source: &[u8]) -> bool {
    flag_segment(cond, source).is_some_and(|seg| VERBOSE_FLAG_NAMES.contains(&seg))
        || is_env_var_presence_condition(cond, source)
}

/// True when `cond` is an environment-variable-presence check:
/// `env::var(KEY).is_ok()` or `env::var_os(KEY).is_some()`, with or
/// without a `std::` prefix. The shape is a `.is_ok()` / `.is_some()`
/// method call whose receiver is a call to `env::var` / `env::var_os`.
/// Such an `eprintln!` only runs when the consumer has set the variable,
/// so it is a runtime opt-in like a verbosity flag.
fn is_env_var_presence_condition(cond: tree_sitter::Node, source: &[u8]) -> bool {
    // `<receiver>.is_ok()` / `<receiver>.is_some()`
    if cond.kind() != "call_expression" {
        return false;
    }
    let Some(func) = cond.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let presence_ok = func
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|m| m == "is_ok" || m == "is_some");
    if !presence_ok {
        return false;
    }
    // The receiver must be a call to `env::var` / `env::var_os`.
    func.child_by_field_name("value")
        .is_some_and(|recv| is_env_var_call(recv, source))
}

/// True when `node` is a call whose callee path ends in `env::var` or
/// `env::var_os` — i.e. the final segment is `var`/`var_os` and the
/// segment before it is `env` (matches `std::env::var_os`, `env::var`, …).
fn is_env_var_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "scoped_identifier" {
        return false;
    }
    let Some(name) = func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
    else {
        return false;
    };
    if name != "var" && name != "var_os" {
        return false;
    }
    // The qualifier directly before `var`/`var_os` must be `env`.
    func.child_by_field_name("path")
        .is_some_and(|path| trailing_path_segment(path, source) == Some("env"))
}

/// The final segment of a path: the `name` of a `scoped_identifier`
/// (`std::env` → `env`) or the text of a bare `identifier` (`env` → `env`).
fn trailing_path_segment<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok(),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok()),
        _ => None,
    }
}

/// Extract the final segment of a simple flag reference, or `None` if
/// `cond` is not one of the accepted simple shapes.
fn flag_segment<'a>(cond: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match cond.kind() {
        // `verbose`
        "identifier" => cond.utf8_text(source).ok(),
        // `self.verbose`, `opts.debug`
        "field_expression" => cond
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok()),
        // `self.verbose()` — only the no-argument call form.
        "call_expression" => {
            let args = cond.child_by_field_name("arguments")?;
            if args.named_child_count() != 0 {
                return None;
            }
            let func = cond.child_by_field_name("function")?;
            flag_segment(func, source)
        }
        _ => None,
    }
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

    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    /// Run on `rel_path` inside a temp crate with the given `Cargo.toml`,
    /// so the crate-shape check resolves against a controlled manifest
    /// instead of comply's own (binary-only) `Cargo.toml`.
    fn run_in_crate(cargo_toml_contents: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        let src_path = dir.path().join(rel_path);
        if let Some(parent) = src_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "mylib"
version = "0.1.0"
edition = "2021"

[lib]
name = "mylib"
path = "src/lib.rs"
"#;

    const BIN_ONLY_CARGO_TOML: &str = r#"
[package]
name = "mytool"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "mytool"
path = "src/main.rs"
"#;

    /// starship shape: a CLI binary that carries a `[lib]` table (and a
    /// `src/lib.rs`) purely to expose internals to its own integration tests,
    /// alongside a `[[bin]]` target that is the real entry point.
    /// `is_binary_only()` is false here, yet the crate still owns its stderr.
    const CLI_WITH_LIB_CARGO_TOML: &str = r#"
[package]
name = "starship"
version = "0.1.0"
edition = "2021"

[lib]
name = "starship"
path = "src/lib.rs"

[[bin]]
name = "starship"
path = "src/main.rs"
"#;

    /// A build-time codegen library: its `[package].name` ends in `-build`, so
    /// it is consumed from a consumer's `build.rs`, where `eprintln!` to Cargo's
    /// build-output stream is the idiomatic diagnostic channel.
    const BUILD_CODEGEN_CARGO_TOML: &str = r#"
[package]
name = "grpc-protobuf-build"
version = "0.1.0"
edition = "2021"

[lib]
name = "grpc_protobuf_build"
path = "src/lib.rs"
"#;

    const CODEGEN_CARGO_TOML: &str = r#"
[package]
name = "something-codegen"
version = "0.1.0"
edition = "2021"

[lib]
name = "something_codegen"
path = "src/lib.rs"
"#;

    /// An FFI bridge crate built as a C dynamic library (Python/Java bindings):
    /// `[lib] crate-type = ["cdylib"]`. Linked by a foreign runtime, never a Rust
    /// library consumer — `eprintln!` is the only error channel at the boundary.
    const CDYLIB_FFI_CARGO_TOML: &str = r#"
[package]
name = "cozo-lib-python"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
"#;

    /// An FFI bridge crate built as a static library (Swift bindings):
    /// `[lib] crate-type = ["staticlib"]`.
    const STATICLIB_FFI_CARGO_TOML: &str = r#"
[package]
name = "cozo-lib-swift"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["staticlib"]
"#;

    /// A crate-type that mixes a Rust library target (`rlib`) with `cdylib` is
    /// still consumed as a Rust library, so it is NOT an FFI-only bridge and
    /// stays flagged.
    const CDYLIB_PLUS_RLIB_CARGO_TOML: &str = r#"
[package]
name = "mixed"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]
"#;

    /// A logging/tracing infrastructure crate: `[package].name` carries a
    /// `tracing` segment (`tracing-subscriber`). It implements the subscriber
    /// machinery itself, so it cannot route its own failures through tracing.
    const TRACING_SUBSCRIBER_CARGO_TOML: &str = r#"
[package]
name = "tracing-subscriber"
version = "0.1.0"
edition = "2021"

[lib]
name = "tracing_subscriber"
path = "src/lib.rs"
"#;

    /// An ordinary application/library that merely *depends on* `tracing` keeps
    /// a normal package name — it is not logging infrastructure and stays
    /// flagged.
    const TRACING_DEPENDENT_CARGO_TOML: &str = r#"
[package]
name = "myapp"
version = "0.1.0"
edition = "2021"

[lib]
name = "myapp"
path = "src/lib.rs"

[dependencies]
tracing = "0.1"
"#;

    #[test]
    fn flags_eprintln_in_library_file() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #4465: `grpc-protobuf-build` (like `prost-build` /
    /// `tonic-build` / `bindgen`) is a build-time codegen library called from a
    /// consumer's `build.rs`. Its `eprintln!` forwards `protoc`'s stderr to
    /// Cargo's build output — the idiomatic build-script diagnostic channel.
    #[test]
    fn allows_eprintln_in_build_codegen_crate() {
        let source = "fn f() { eprintln!(\"{}\", msg); }";
        assert!(run_in_crate(BUILD_CODEGEN_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A `-codegen`-suffixed crate is the same category of build-time codegen
    /// library and is exempt as well.
    #[test]
    fn allows_eprintln_in_codegen_crate() {
        let source = "fn f() { eprintln!(\"{}\", msg); }";
        assert!(run_in_crate(CODEGEN_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    #[test]
    fn flags_eprint_in_library_file() {
        let source = "fn f() { eprint!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #4749 (cozodb/cozo `cozo-lib-python`): a `cdylib` FFI
    /// bridge crate is linked into a Python/Java runtime that never initialises
    /// a Rust tracing subscriber. `eprintln!` is the only way to surface errors
    /// at the FFI boundary, so it is exempt.
    #[test]
    fn allows_eprintln_in_cdylib_ffi_crate() {
        let source = "fn f() { eprintln!(\"{}\", err); }";
        assert!(run_in_crate(CDYLIB_FFI_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Regression for #4749 (cozodb/cozo `cozo-lib-swift`): a `staticlib` FFI
    /// bridge crate is the same case — exempt.
    #[test]
    fn allows_eprintln_in_staticlib_ffi_crate() {
        let source = "fn f() { eprintln!(\"{err}\"); }";
        assert!(run_in_crate(STATICLIB_FFI_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A crate that declares both `cdylib` and `rlib` is still consumed as a
    /// Rust library by other crates, so its `eprintln!` stays flagged.
    #[test]
    fn flags_eprintln_in_cdylib_plus_rlib_crate() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(
            run_in_crate(CDYLIB_PLUS_RLIB_CARGO_TOML, "src/lib.rs", source).len(),
            1
        );
    }

    /// Regression for #4994 (tokio-rs/tracing `tracing-subscriber`): a
    /// logging/tracing infrastructure crate implements the subscriber/formatter
    /// machinery itself and cannot route its own internal failures through
    /// `tracing` (that is what has failed or would recurse). `eprintln!` is its
    /// last-resort fallback — e.g. when the formatter fails or `RUST_LOG`
    /// can't be parsed — and is exempt.
    #[test]
    fn allows_eprintln_in_logging_infra_crate() {
        let source =
            "fn f() { eprintln!(\"[tracing-subscriber] Unable to format event: {:?}\", attrs); }";
        assert!(run_in_crate(TRACING_SUBSCRIBER_CARGO_TOML, "src/fmt/fmt_layer.rs", source).is_empty());
    }

    /// The logging-infra exemption keys off the crate's *own* package name, not
    /// on a `tracing` dependency: an ordinary crate that merely depends on
    /// `tracing` is not logging infrastructure and stays flagged.
    #[test]
    fn flags_eprintln_in_crate_that_merely_depends_on_tracing() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(
            run_in_crate(TRACING_DEPENDENT_CARGO_TOML, "src/lib.rs", source).len(),
            1
        );
    }

    /// Regression for #981: a module of a binary-only crate (no `[lib]`,
    /// no `src/lib.rs`) has no library consumers — `eprintln!` is fine
    /// even outside `main.rs` / `bin/`.
    #[test]
    fn allows_eprintln_in_binary_only_crate_module() {
        let source = "fn print_help() { eprintln!(\"usage\"); }";
        assert!(run_in_crate(BIN_ONLY_CARGO_TOML, "src/session.rs", source).is_empty());
    }

    #[test]
    fn flags_eprintln_in_library_crate_module() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/util.rs", source).len(), 1);
    }

    /// A library crate (it has `src/lib.rs`) that also declares an executable
    /// target by explicit `path` in a non-standard directory. The `path` field
    /// can name a `[[bin]]`, `[[example]]`, `[[bench]]`, or `[[test]]` target —
    /// all standalone executables with their own `fn main()`.
    const LIB_WITH_EXPLICIT_TARGET_CARGO_TOML: &str = r#"
[package]
name = "smoltcp"
version = "0.1.0"
edition = "2021"

[lib]
name = "smoltcp"
path = "src/lib.rs"

[[example]]
name = "packet2pcap"
path = "utils/packet2pcap.rs"
required-features = ["std"]
"#;

    /// Regression for #4728 (smoltcp `utils/packet2pcap.rs:47`): the file is a
    /// standalone executable declared via an explicit `path` in a target table
    /// (`[[example]]`/`[[bin]]`), with its own `fn main()`. It is application
    /// code even though it lives in `utils/` (not `src/main.rs` / `src/bin/`)
    /// and the crate also ships a library — `eprintln!` belongs there.
    #[test]
    fn allows_eprintln_in_explicit_path_executable_target() {
        let source = "fn main() { eprintln!(\"{e}\"); }";
        assert!(
            run_in_crate(
                LIB_WITH_EXPLICIT_TARGET_CARGO_TOML,
                "utils/packet2pcap.rs",
                source,
            )
            .is_empty()
        );
    }

    /// The explicit-target exemption is path-scoped: a genuine library module
    /// in the same crate — not named by any target `path` — stays flagged.
    #[test]
    fn flags_eprintln_in_library_module_of_crate_with_explicit_target() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(
            run_in_crate(LIB_WITH_EXPLICIT_TARGET_CARGO_TOML, "src/wire.rs", source).len(),
            1
        );
    }

    /// Regression for #1312: starship declares a `[[bin]]` target (the
    /// `starship` CLI) alongside a `[lib]` used only to expose internals to
    /// integration tests. `eprintln!` in setup/logger code is controlled CLI
    /// error output — not a library writing to a consumer's stderr.
    #[test]
    fn allows_eprintln_in_cli_crate_with_internal_lib() {
        let source = "fn init_logger() { eprintln!(\"Unable to create log dir\"); }";
        assert!(run_in_crate(CLI_WITH_LIB_CARGO_TOML, "src/logger.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_in_main_rs() {
        let source = "fn main() { eprintln!(\"oops\"); }";
        assert!(run_on(source, "src/main.rs").is_empty());
    }

    #[test]
    fn allows_eprintln_in_bin_dir() {
        let source = "fn main() { eprintln!(\"oops\"); }";
        assert!(run_on(source, "src/bin/tool.rs").is_empty());
    }

    /// Regression for #1310: `eprintln!` gated behind `if self.verbose() { … }`
    /// is opt-in diagnostics (polars sets the flag via `POLARS_VERBOSE`),
    /// not unconditional library noise.
    #[test]
    fn allows_eprintln_under_self_verbose_method_guard() {
        let source = "fn f(&self) { if self.verbose() { eprintln!(\"CACHE SET: {id}\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_under_bare_debug_ident_guard() {
        let source = "fn f() { if debug { eprintln!(\"trace\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_under_field_verbose_guard() {
        let source = "fn f(&self) { if self.verbose { eprintln!(\"trace\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// Regression for #3941 (uv `uv-resolver/src/error.rs:789`): an
    /// `eprintln!` gated behind `std::env::var_os(KEY).is_some()` only runs
    /// when the consumer sets the variable — opt-in diagnostics, not noise.
    #[test]
    fn allows_eprintln_under_env_var_os_is_some_guard() {
        let source =
            "pub fn f() { if std::env::var_os(\"KEY\").is_some() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_under_env_var_is_ok_guard() {
        let source = "pub fn f() { if std::env::var(\"KEY\").is_ok() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The `std::` prefix is optional — `env::var_os(..)` is the same gate.
    #[test]
    fn allows_eprintln_under_bare_env_var_os_guard() {
        let source = "pub fn f() { if env::var_os(\"KEY\").is_some() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The issue's exact key shape: an associated const as the key arg.
    #[test]
    fn allows_eprintln_under_env_var_os_const_key_guard() {
        let source = "pub fn f() { if std::env::var_os(EnvVars::UV_INTERNAL__SHOW_DERIVATION_TREE).is_some() { eprintln!(\"x\"); } }";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// A non-`env::var` presence check is not a runtime opt-in: a plain
    /// `.is_some()` on some other call stays flagged.
    #[test]
    fn flags_eprintln_under_non_env_is_some_guard() {
        let source = "pub fn f(o: Option<u8>) { if o.is_some() { eprintln!(\"x\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Bare, un-gated `eprintln!` in library code stays flagged even when a
    /// flag-guarded sibling exists in the same function.
    #[test]
    fn flags_ungated_eprintln_alongside_guarded_one() {
        let source = "fn f(&self) { eprintln!(\"oops\"); if self.verbose() { eprintln!(\"dbg\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// An `if` whose condition is not a verbose-style flag does not exempt.
    #[test]
    fn flags_eprintln_under_non_flag_guard() {
        let source = "fn f(items: Vec<u8>) { if items.is_empty() { eprintln!(\"oops\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// A negated flag guard is the inverse case — output when the flag is
    /// *off* is exactly the unconditional noise the rule targets.
    #[test]
    fn flags_eprintln_under_negated_verbose_guard() {
        let source = "fn f(&self) { if !self.verbose() { eprintln!(\"oops\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// The `else` branch of a flag guard is not the gated path.
    #[test]
    fn flags_eprintln_in_else_of_verbose_guard() {
        let source =
            "fn f(&self) { if self.verbose() { let _ = 1; } else { eprintln!(\"oops\"); } }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #4474 (anyhow `build.rs`): a Cargo build script is a
    /// separate binary run at build time, not library code. `eprintln!` there
    /// writes to Cargo's build-output stream — the idiomatic diagnostic channel.
    /// Run inside a library-only crate so the build-script exemption (not a
    /// `[[bin]]`/codegen manifest) is the only thing that can clear it.
    #[test]
    fn allows_eprintln_in_build_script() {
        let source = "fn main() { eprintln!(\"Failed: {}\", err); }";
        assert!(run_in_crate(LIB_CARGO_TOML, "build.rs", source).is_empty());
    }

    #[test]
    fn allows_eprintln_in_test_function() {
        let source = "#[test]\nfn t() { eprintln!(\"trace\"); }";
        assert!(run_on(source, "src/lib.rs").is_empty());
    }

    #[test]
    fn allows_eprintln_in_tests_dir() {
        let source = "fn f() { eprintln!(\"trace\"); }";
        assert!(run_on(source, "tests/it.rs").is_empty());
    }
}
