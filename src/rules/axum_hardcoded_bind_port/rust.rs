//! axum-hardcoded-bind-port backend.
//!
//! A platform that publishes a container tells the service which port to serve
//! by injecting it into the environment. A service that binds a port written
//! into its source ignores that injection and stays unreachable there, so one
//! image cannot serve both the local run and such a platform.
//!
//! The rule flags one shape: `TcpListener::bind(<addr>)` where `<addr>` is an
//! address the call spells out, on the **unspecified** IP with a **non-zero**
//! port.
//!
//! ## Why the unspecified address
//!
//! `0.0.0.0` and `::` answer true to [`std::net::IpAddr::is_unspecified`]. That
//! is the bind which accepts connections on every interface, and it is what a
//! platform routes traffic to. A loopback bind (`127.0.0.1`, `::1`) answers
//! only from inside the machine. No platform publishes it and none injects a
//! port for it, so a fixed port there is a deliberate local choice.
//!
//! The rule reads an address into a [`std::net::SocketAddr`] and asks
//! `is_unspecified()`, instead of comparing the text against known addresses.
//!
//! ## Why a non-zero port
//!
//! Port `0` asks the operating system for an ephemeral port, so the address is
//! already dynamic and nothing is hardcoded.
//!
//! ## Why the crate must declare `axum`
//!
//! The premise is a platform that injects `PORT` for an HTTP service. It does
//! not carry over to a process that binds an IANA-assigned port by protocol (a
//! DNS or SMTP server) or to a peer-to-peer node. The rule therefore runs only
//! where the nearest `Cargo.toml` lists `axum` among the dependencies linked
//! into the crate's own targets. That is crate provenance read from the
//! manifest, not a module name or a fragment of source text.
//!
//! ## Where the rule stays silent
//!
//! Test code (`#[cfg(test)]`, `#[test]`, a `tests/` directory) and relaxed
//! directories (`examples/`, `benches/`, …) bind fixed ports on purpose.
//!
//! A literal reached only through a branch that an environment read selects is
//! already configurable, so a bind inside such a branch is silent. The read has
//! to stand in the expression that chooses the branch: one bound to a variable
//! first is out of reach of a syntax-only check.
//!
//! The rule is also silent on every address the call does not spell out: a
//! variable, a `format!`, a constant, an `std::env::var` lookup. A syntax-only
//! check cannot read what such an expression holds.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::CheckCtx;
use crate::rules::rust_helpers;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

crate::ast_check! { on ["call_expression"] prefilter = ["TcpListener::bind"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if !is_tcp_listener_bind(func, source) { return; }
    let Some(argument) = sole_argument(node) else { return };
    let Some(address) = socket_address(argument, source) else { return };
    if !is_fixed_port_on_every_interface(address) { return; }
    if rust_helpers::is_in_test_context(node, source) { return; }
    if binds_the_environment_fallback(node, source) { return; }
    if !is_axum_crate(ctx) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "This binds every interface on a port written into the source. On a \
         platform that injects `PORT`, the service is unreachable there. Read \
         the port from the environment and keep the literal as the fallback \
         the local run needs."
            .into(),
        Severity::Error,
    ));
}

/// True when the crate owning this file links `axum` — the provenance that
/// makes the platform-injected `PORT` premise apply. A file with no reachable
/// `Cargo.toml` proves nothing and is left alone.
fn is_axum_crate(ctx: &CheckCtx) -> bool {
    ctx.project
        .nearest_cargo_manifest(ctx.path)
        .is_some_and(|manifest| manifest.depends_on("axum"))
}

/// `TcpListener::bind` — a path named `bind` qualified by the `TcpListener`
/// type, however that type is reached (`TcpListener::bind`,
/// `tokio::net::TcpListener::bind`, `std::net::TcpListener::bind`). A bare
/// `bind(...)` or a `.bind(...)` method on some other receiver is too generic
/// to key on and stays silent.
fn is_tcp_listener_bind(func: tree_sitter::Node, source: &[u8]) -> bool {
    rust_helpers::trailing_path_segment(func, source) == Some("bind")
        && path_qualifier(func, source) == Some("TcpListener")
}

/// The type a path is qualified by — the segment before the last
/// (`std::net::Ipv4Addr::UNSPECIFIED` -> `Ipv4Addr`). `None` for an unqualified
/// path (`bind`) and for anything that is not a path (`builder.bind`).
fn path_qualifier<'src>(path: tree_sitter::Node, source: &'src [u8]) -> Option<&'src str> {
    rust_helpers::trailing_path_segment(path.child_by_field_name("path")?, source)
}

/// True when `node` sits in a branch that an environment read selects: an arm
/// of a `match` on `env::var`, a branch of an `if` testing one, or the `else`
/// of a `let` binding one.
///
/// A literal such a branch holds is already configurable — the environment
/// decides whether the program reaches it at all. Only the expression that
/// chooses the branch is read, so an unrelated lookup elsewhere in the same
/// function excuses nothing, and a bind written inside that expression is not
/// one of the branches it chooses. The walk stops at the enclosing function:
/// an arm selects the item it holds, not the binds the item's own body writes.
fn binds_the_environment_fallback(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut child = node;
    while let Some(parent) = child.parent() {
        if parent.kind() == "function_item" {
            return false;
        }
        let selector = match parent.kind() {
            "match_expression" | "let_declaration" => parent.child_by_field_name("value"),
            "if_expression" => parent.child_by_field_name("condition"),
            _ => None,
        };
        if let Some(selector) = selector
            && selector != child
            && rust_helpers::subtree_reads_env_var(selector, source)
        {
            return true;
        }
        child = parent;
    }
    false
}

/// The one argument `TcpListener::bind` takes, with a leading `&` peeled off.
/// A call carrying any other number of arguments is not that function.
fn sole_argument<'tree>(call: tree_sitter::Node<'tree>) -> Option<tree_sitter::Node<'tree>> {
    let [only] = *named_elements(call.child_by_field_name("arguments")?) else {
        return None;
    };
    match only.kind() {
        "reference_expression" => only.child_by_field_name("value"),
        _ => Some(only),
    }
}

/// The expressions a delimited list holds — the arguments of a call, the
/// members of a tuple, the elements of an array — with the comments dropped.
/// tree-sitter-rust makes a comment a named child, so a list read without this
/// filter loses its shape as soon as anyone annotates an element.
fn named_elements(list: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|child| !rust_helpers::is_comment_node(*child))
        .collect()
}

/// The address an expression spells out, or `None` when a syntax-only read
/// cannot say which address it is — including for every address reached
/// through a binding.
///
/// These `ToSocketAddrs` spellings are read:
///
/// - `"0.0.0.0:3000"`, and the raw form `r"0.0.0.0:3000"`
/// - `("0.0.0.0", 3000)`, and the same tuple wrapped in `SocketAddr::from`
/// - `SocketAddr::new(host, 3000)`, and the same for `SocketAddrV4` and
///   `SocketAddrV6`
///
/// See [`spelled_out_host`] for the host spellings the last two accept.
fn socket_address(expr: tree_sitter::Node, source: &[u8]) -> Option<SocketAddr> {
    match expr.kind() {
        "string_literal" | "raw_string_literal" => {
            string_literal_content(expr, source)?.parse().ok()
        }
        "tuple_expression" => {
            let [host, port] = *named_elements(expr) else {
                return None;
            };
            host_and_port(host, port, source)
        }
        "call_expression" => socket_addr_constructor(expr, source),
        _ => None,
    }
}

/// The address a `SocketAddr` constructor spells out: `SocketAddr::from(<tuple>)`
/// forwards to the tuple, and `SocketAddr::new` / `SocketAddrV4::new` /
/// `SocketAddrV6::new` take the host and the port as their first two arguments.
fn socket_addr_constructor(call: tree_sitter::Node, source: &[u8]) -> Option<SocketAddr> {
    let func = call.child_by_field_name("function")?;
    if !matches!(
        path_qualifier(func, source)?,
        "SocketAddr" | "SocketAddrV4" | "SocketAddrV6"
    ) {
        return None;
    }
    let arguments = named_elements(call.child_by_field_name("arguments")?);
    match rust_helpers::trailing_path_segment(func, source)? {
        "from" => match arguments.as_slice() {
            [tuple] => socket_address(*tuple, source),
            _ => None,
        },
        // `SocketAddrV6::new` carries a flow info and a scope id after the port.
        "new" => match arguments.as_slice() {
            [host, port, ..] => host_and_port(*host, *port, source),
            _ => None,
        },
        _ => None,
    }
}

/// The address a host expression and a port expression spell out together.
fn host_and_port(
    host: tree_sitter::Node,
    port: tree_sitter::Node,
    source: &[u8],
) -> Option<SocketAddr> {
    Some(SocketAddr::new(
        spelled_out_host(host, source)?,
        integer_literal_value(port, source)?,
    ))
}

/// The address a host expression spells out, or `None` when a syntax-only read
/// cannot say which address it is.
///
/// Read: a string literal, an octet array, an `Ipv4Addr::new` / `Ipv6Addr::new`
/// call, and the `UNSPECIFIED` associated constant of `Ipv4Addr` / `Ipv6Addr`.
/// Any of the last three may be wrapped in an `IpAddr` constructor
/// (`IpAddr::V4`, `IpAddr::from`).
///
/// The address returned is whichever one the expression names, loopback
/// included, so the caller decides which addresses it reports.
fn spelled_out_host(host: tree_sitter::Node, source: &[u8]) -> Option<IpAddr> {
    match host.kind() {
        "string_literal" | "raw_string_literal" => {
            string_literal_content(host, source)?.parse().ok()
        }
        "array_expression" => octet_address(&named_elements(host), source),
        "scoped_identifier" => match (
            path_qualifier(host, source)?,
            rust_helpers::trailing_path_segment(host, source)?,
        ) {
            ("Ipv4Addr", "UNSPECIFIED") => Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            ("Ipv6Addr", "UNSPECIFIED") => Some(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            _ => None,
        },
        "call_expression" => ip_constructor_address(host, source),
        _ => None,
    }
}

/// The address an IP constructor spells out: `IpAddr::V4` / `IpAddr::V6` /
/// `IpAddr::from` wrap another host expression, `Ipv4Addr::new` takes four
/// octets and `Ipv6Addr::new` eight segments.
fn ip_constructor_address(call: tree_sitter::Node, source: &[u8]) -> Option<IpAddr> {
    let func = call.child_by_field_name("function")?;
    let arguments = named_elements(call.child_by_field_name("arguments")?);
    match (
        path_qualifier(func, source)?,
        rust_helpers::trailing_path_segment(func, source)?,
    ) {
        ("IpAddr", "V4" | "V6" | "from") => match arguments.as_slice() {
            [inner] => spelled_out_host(*inner, source),
            _ => None,
        },
        ("Ipv4Addr", "new") => <[u8; 4]>::try_from(octets(&arguments, source)?.as_slice())
            .ok()
            .map(|v4| IpAddr::V4(Ipv4Addr::from(v4))),
        ("Ipv6Addr", "new") => {
            let segments = integer_literal_values(&arguments, source)?;
            <[u16; 8]>::try_from(segments.as_slice())
                .ok()
                .map(|v6| IpAddr::V6(Ipv6Addr::from(v6)))
        }
        _ => None,
    }
}

/// The address a list of octets holds: four make an IPv4 address, sixteen make
/// an IPv6 one. `None` for any other length.
fn octet_address(elements: &[tree_sitter::Node], source: &[u8]) -> Option<IpAddr> {
    let octets = octets(elements, source)?;
    match <[u8; 4]>::try_from(octets.as_slice()) {
        Ok(v4) => Some(IpAddr::V4(Ipv4Addr::from(v4))),
        Err(_) => <[u8; 16]>::try_from(octets.as_slice())
            .ok()
            .map(|v6| IpAddr::V6(Ipv6Addr::from(v6))),
    }
}

/// The octets a list of decimal integer literals holds, or `None` as soon as
/// one element is not such a literal or does not fit an octet.
fn octets(elements: &[tree_sitter::Node], source: &[u8]) -> Option<Vec<u8>> {
    integer_literal_values(elements, source)?
        .into_iter()
        .map(|value| u8::try_from(value).ok())
        .collect()
}

/// The values a list of decimal integer literals holds, or `None` as soon as
/// one element is not such a literal.
fn integer_literal_values(elements: &[tree_sitter::Node], source: &[u8]) -> Option<Vec<u16>> {
    elements
        .iter()
        .map(|element| integer_literal_value(*element, source))
        .collect()
}

/// True when `address` accepts connections on every interface at a port that
/// cannot move. A zero port is the ephemeral one the operating system picks,
/// and a loopback host answers only from inside the machine.
fn is_fixed_port_on_every_interface(address: SocketAddr) -> bool {
    address.ip().is_unspecified() && address.port() != 0
}

/// The text a string literal holds, or `None` when it holds an escape sequence
/// — an address literal never needs one, so anything escaped is another kind of
/// string. A raw literal (`r"0.0.0.0:3000"`) carries the same single
/// `string_content` child and reads the same way.
fn string_literal_content<'src>(
    literal: tree_sitter::Node,
    source: &'src [u8],
) -> Option<&'src str> {
    let mut cursor = literal.walk();
    let parts: Vec<_> = literal.named_children(&mut cursor).collect();
    let [content] = parts.as_slice() else {
        return None;
    };
    if content.kind() != "string_content" {
        return None;
    }
    content.utf8_text(source).ok()
}

/// The value of a decimal integer literal, digit separators dropped and a type
/// suffix ignored (`3_000u16` -> `3000`). `None` when the value overflows a
/// `u16`, when the node is not an integer literal, or when the literal is
/// written in another radix.
///
/// A radix literal opens with the digit `0` (`0x7f`, `0o177`, `0b0111_1111`),
/// so reading its decimal prefix would answer `0` — a valid port and a valid
/// octet, both wrong. Such a literal has no decimal value to report.
fn integer_literal_value(literal: tree_sitter::Node, source: &[u8]) -> Option<u16> {
    if literal.kind() != "integer_literal" {
        return None;
    }
    let text = literal.utf8_text(source).ok()?;
    let end_of_digits = text
        .find(|c: char| !c.is_ascii_digit() && c != '_')
        .unwrap_or(text.len());
    let (digits, suffix) = text.split_at(end_of_digits);
    // What follows the digits is either a type suffix (`u16`, `i32`) or the
    // rest of a radix literal (`0x7f` -> `x7f`), whose value this cannot read.
    if !suffix.is_empty() && !suffix.starts_with(['u', 'i']) {
        return None;
    }
    digits.replace('_', "").parse().ok()
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
    use crate::files::Language;
    use crate::rules::file_ctx::FileCtx;
    use crate::rules::test_helpers::run_rule_with_cargo;
    use std::path::Path;

    const AXUM_CARGO_TOML: &str = r#"
[package]
name = "my-service"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
"#;

    /// Run on `src/main.rs` of a crate whose manifest is `cargo_toml`.
    fn run_with_cargo(cargo_toml: &str, source: &str) -> Vec<Diagnostic> {
        run_rule_with_cargo(&Check, cargo_toml, source, "src/main.rs")
    }

    /// Run on `src/main.rs` of an axum crate — the rule's target context.
    fn run(source: &str) -> Vec<Diagnostic> {
        run_with_cargo(AXUM_CARGO_TOML, source)
    }

    /// Whether the engine offers a file at `relative_path` to this rule, i.e.
    /// what the `RuleMeta` directory skips decide.
    fn applies_to(relative_path: &str) -> bool {
        let path = Path::new("/crate").join(relative_path);
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(&path, "", Language::Rust, project);
        super::super::META.applies_to_file(&file)
    }

    // ── Positive: an address the call spells out on every interface ────────

    #[test]
    fn flags_string_literal_bind() {
        let src = r#"async fn main() { let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap(); }"#;
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule_id.as_ref(), "axum-hardcoded-bind-port");
        // Points at `TcpListener::bind(...)`, not at the enclosing statement.
        assert_eq!(diagnostics[0].column, 34);
    }

    #[test]
    fn flags_fully_qualified_tokio_bind() {
        let src = r#"async fn main() { tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ipv6_unspecified_bind() {
        let src = r#"async fn main() { TcpListener::bind("[::]:3000").await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_host_port_tuple_bind() {
        let src = r#"async fn main() { TcpListener::bind(("0.0.0.0", 3000)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_reference_to_string_literal_bind() {
        let src = r#"async fn main() { TcpListener::bind(&"0.0.0.0:3000").await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_separated_and_suffixed_port_literal() {
        let src = r#"async fn main() { TcpListener::bind(("0.0.0.0", 3_000u16)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_annotated_arguments() {
        // A comment among the arguments must not hide the address.
        let src = r#"async fn main() { TcpListener::bind(/* public */ "0.0.0.0:3000").await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Positive: the address goes through a `SocketAddr` constructor ──────

    #[test]
    fn flags_socket_addr_from_octet_array() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 3000))).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_socket_addr_from_unspecified_constant() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 3000))).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_socket_addr_new_with_wrapped_constant() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 3000)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_socket_addr_v4_new() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 3000)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unspecified_constant_tuple() {
        let src = r#"async fn main() { TcpListener::bind((Ipv6Addr::UNSPECIFIED, 3000)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_socket_addr_v6_new_with_flow_info_and_scope() {
        // `SocketAddrV6::new` carries two more arguments after the port; the
        // host and the port still sit in front of them.
        let src = r#"async fn main() { TcpListener::bind(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 3000, 0, 0)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_sixteen_octet_unspecified_array() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::new(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), 3000)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ipv4_addr_new_host() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3000)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ipv6_addr_new_host() {
        // Eight `u16` segments, not sixteen octets.
        let src = r#"async fn main() { TcpListener::bind(SocketAddrV6::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 3000, 0, 0)).await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_raw_string_address() {
        // A raw literal is as fixed as a quoted one and holds the same
        // `string_content` child.
        let src = r#"async fn main() { TcpListener::bind(r"0.0.0.0:3000").await.unwrap(); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: the address is not spelled out ───────────────────────────

    #[test]
    fn allows_port_read_from_the_environment() {
        let src = r#"
async fn main() {
    let port = std::env::var("PORT").unwrap().parse::<u16>().unwrap();
    TcpListener::bind(("0.0.0.0", port)).await.unwrap();
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_formatted_address() {
        let src = r#"
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_owned());
    TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_address_held_by_a_variable() {
        let src = r#"async fn main() { TcpListener::bind(addr).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_port_behind_a_constant() {
        let src = r#"
const PORT: u16 = 3000;
async fn main() { TcpListener::bind(("0.0.0.0", PORT)).await.unwrap(); }
"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: the literal is the fallback of an environment read ───────

    #[test]
    fn allows_literal_in_the_failed_arm_of_an_environment_match() {
        // This is the remediation the rule asks for, written with `match`
        // instead of `unwrap_or_else`.
        let src = r#"
async fn main() {
    let listener = match std::env::var("PORT") {
        Ok(port) => TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap(),
        Err(_) => TcpListener::bind("0.0.0.0:3000").await.unwrap(),
    };
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_literal_in_the_failed_branch_of_an_environment_if() {
        let src = r#"
async fn main() {
    let listener = if let Ok(port) = std::env::var("PORT") {
        TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap()
    } else {
        TcpListener::bind("0.0.0.0:3000").await.unwrap()
    };
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_literal_in_the_else_of_an_environment_let() {
        let src = r#"
async fn main() {
    let Ok(port) = std::env::var("PORT") else {
        TcpListener::bind("0.0.0.0:3000").await.unwrap();
        return;
    };
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bind_in_a_nested_function_inside_an_environment_arm() {
        // The walk stops at the enclosing function: the arm selects the item,
        // not the binds the item's own body writes.
        let src = r#"
async fn main() {
    match std::env::var("PORT") {
        Ok(_) => {}
        Err(_) => { async fn serve() { TcpListener::bind("0.0.0.0:3000").await.unwrap(); } }
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bind_written_inside_the_expression_that_selects() {
        // The scrutinee is not one of the branches it chooses, so reading the
        // environment there excuses nothing written alongside it.
        let src = r#"
async fn main() {
    match std::env::var("PORT").map(|_| TcpListener::bind("0.0.0.0:3000")) {
        Ok(_) => {}
        Err(_) => {}
    }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bind_beside_an_environment_read_that_selects_another_value() {
        // Negative space for the three tests above: only the expression that
        // chooses the branch counts. A lookup elsewhere in the same function
        // leaves the address as fixed as it was.
        let src = r#"
async fn main() {
    let level = match std::env::var("RUST_LOG") {
        Ok(level) => level,
        Err(_) => "info".to_owned(),
    };
    TcpListener::bind("0.0.0.0:3000").await.unwrap();
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: the address is spelled out but is not a deployed bind ────

    #[test]
    fn allows_loopback_bind() {
        let src = r#"async fn main() { TcpListener::bind("127.0.0.1:3000").await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ipv6_loopback_bind() {
        let src = r#"async fn main() { TcpListener::bind("[::1]:3000").await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_loopback_host_tuple() {
        let src = r#"async fn main() { TcpListener::bind(("127.0.0.1", 3000)).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_loopback_octet_array() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 3000))).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_localhost_constant() {
        // `LOCALHOST` is not one of the constants the host reader knows, so it
        // names no address at all rather than naming the loopback one.
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000)).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ephemeral_port() {
        let src = r#"async fn main() { TcpListener::bind("0.0.0.0:0").await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ephemeral_port_in_tuple() {
        let src = r#"async fn main() { TcpListener::bind(("0.0.0.0", 0)).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_port_written_in_another_radix() {
        // Reading the decimal prefix of `0x0bb8` would answer `0` and let the
        // ephemeral-port test swallow a real port. The literal has no decimal
        // value, so the reader reports none.
        let src = r#"async fn main() { TcpListener::bind(("0.0.0.0", 0x0bb8)).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_octets_written_in_another_radix() {
        // `[0x7f, 0x00, 0x00, 0x01]` is loopback. Reading the decimal prefix of
        // each octet would answer `[0, 0, 0, 0]` and flag it as every
        // interface, which is the opposite of what the array says.
        for octets in ["[0x7f, 0x00, 0x00, 0x01]", "[0o177, 0, 0, 1]"] {
            let src = format!(
                r#"async fn main() {{ TcpListener::bind(SocketAddr::from(({octets}, 3000))).await.unwrap(); }}"#
            );
            assert!(run(&src).is_empty(), "must not read {octets} as unspecified");
        }
    }

    #[test]
    fn allows_octet_array_of_the_wrong_length() {
        let src = r#"async fn main() { TcpListener::bind(SocketAddr::from(([0, 0, 0], 3000))).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unspecified_constant_of_another_type() {
        // A user type that also names a `UNSPECIFIED` constant is not an
        // address the rule can read.
        let src = r#"async fn main() { TcpListener::bind((MyAddr::UNSPECIFIED, 3000)).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_broadcast_constant() {
        let src = r#"async fn main() { TcpListener::bind((Ipv4Addr::BROADCAST, 3000)).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_port_beyond_u16() {
        let src = r#"async fn main() { TcpListener::bind(("0.0.0.0", 70000)).await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: the call is another function ─────────────────────────────

    #[test]
    fn allows_bare_bind_call() {
        let src = r#"async fn main() { bind("0.0.0.0:3000").await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bind_method_on_another_receiver() {
        let src = r#"async fn main() { builder.bind("0.0.0.0:3000").await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bind_on_another_socket_type() {
        let src = r#"async fn main() { UdpSocket::bind("0.0.0.0:3000").await.unwrap(); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_address_literal_outside_a_bind_call() {
        // svix-bridge's serde default: the literal is the value configuration
        // overrides, not the address the service commits to.
        let src = r#"
fn default_http_listen_address() -> SocketAddr {
    "0.0.0.0:5000".parse().expect("default http listen address")
}
"#;
        assert!(run(src).is_empty());
    }

    // ── Negative: the crate is not an axum service ─────────────────────────

    #[test]
    fn allows_bind_in_a_crate_without_axum() {
        const DNS_CARGO_TOML: &str = r#"
[package]
name = "resolver"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
"#;
        let src = r#"async fn main() { TcpListener::bind("0.0.0.0:53").await.unwrap(); }"#;
        assert!(run_with_cargo(DNS_CARGO_TOML, src).is_empty());
    }

    #[test]
    fn allows_bind_when_axum_is_a_dev_dependency_only() {
        const DEV_ONLY_CARGO_TOML: &str = r#"
[package]
name = "protocol-server"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }

[dev-dependencies]
axum = "0.8"
"#;
        let src = r#"async fn main() { TcpListener::bind("0.0.0.0:9000").await.unwrap(); }"#;
        assert!(run_with_cargo(DEV_ONLY_CARGO_TOML, src).is_empty());
    }

    #[test]
    fn flags_bind_when_axum_is_renamed() {
        const RENAMED_CARGO_TOML: &str = r#"
[package]
name = "my-service"
version = "0.1.0"
edition = "2021"

[dependencies]
web = { package = "axum", version = "0.8" }
tokio = { version = "1", features = ["full"] }
"#;
        let src = r#"async fn main() { TcpListener::bind("0.0.0.0:3000").await.unwrap(); }"#;
        assert_eq!(run_with_cargo(RENAMED_CARGO_TOML, src).len(), 1);
    }

    #[test]
    fn allows_bind_without_a_manifest() {
        // No `Cargo.toml` means no provenance, so the axum premise is unproven.
        let src = r#"async fn main() { TcpListener::bind("0.0.0.0:3000").await.unwrap(); }"#;
        let diagnostics =
            crate::rules::test_helpers::run_rule(&Check, src, "/nonexistent_crate/src/main.rs");
        assert!(diagnostics.is_empty());
    }

    // ── Negative: test code binds fixed ports on purpose ───────────────────

    #[test]
    fn allows_bind_inside_a_cfg_test_module() {
        let src = r#"
#[cfg(test)]
mod tests {
    async fn spawn() { TcpListener::bind("0.0.0.0:3000").await.unwrap(); }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bind_in_a_test_function() {
        let src = r#"
#[tokio::test]
async fn serves() { TcpListener::bind("0.0.0.0:3000").await.unwrap(); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_the_tests_directory() {
        assert!(!applies_to("tests/api.rs"));
    }

    #[test]
    fn skips_relaxed_directories() {
        assert!(!applies_to("examples/hello/main.rs"));
        assert!(!applies_to("benches/serve.rs"));
    }

    #[test]
    fn applies_to_crate_sources() {
        // Negative space for the two skips above: they must not swallow the
        // crate's own sources, which is where the rule earns its findings.
        assert!(applies_to("src/main.rs"));
    }
}
