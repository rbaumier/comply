use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Match a dotted-quad IPv4 pattern starting at `start` in `s`.
/// Returns the full IP string if found.
fn find_ipv4(s: &str, start: usize) -> Option<(usize, String)> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = start;

    // Skip to a digit.
    while i < len && !bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i >= len {
        return None;
    }

    let ip_start = i;
    let mut octets = 0;
    let mut octet_val = 0u32;
    let mut octet_len = 0;

    while i < len {
        if bytes[i].is_ascii_digit() {
            octet_val = octet_val * 10 + (bytes[i] - b'0') as u32;
            octet_len += 1;
            if octet_val > 255 || octet_len > 3 {
                return find_ipv4(s, i + 1);
            }
            i += 1;
        } else if bytes[i] == b'.' && octet_len > 0 && octets < 3 {
            octets += 1;
            octet_val = 0;
            octet_len = 0;
            i += 1;
        } else {
            break;
        }
    }

    if octets == 3 && octet_len > 0 {
        let ip = &s[ip_start..i];
        Some((i, ip.to_string()))
    } else {
        find_ipv4(s, i + 1)
    }
}

const ALLOWED: &[&str] = &["255.255.255.255"];

/// Reserved ranges that are never routable production endpoints:
/// `127.0.0.0/8` (loopback) and `0.0.0.0/8` ("this host", RFC 1122).
/// Hardcoding them never points at a remote host, so there is nothing
/// to move to configuration. Private ranges (`10/8`, `192.168/16`,
/// `172.16/12`) ARE real connection targets and still flag.
fn is_reserved_nonroutable(ip: &str) -> bool {
    ip.starts_with("127.") || ip.starts_with("0.")
}

fn is_documentation_ip(ip: &str) -> bool {
    ip.starts_with("192.0.2.")
        || ip.starts_with("198.51.100.")
        || ip.starts_with("203.0.113.")
}

fn is_version_like(line: &str, ip_end: usize, ip_len: usize) -> bool {
    let bytes = line.as_bytes();
    let ip_start = ip_end - ip_len;
    if ip_start > 0 && (bytes[ip_start - 1] == b'v' || bytes[ip_start - 1] == b'V') {
        return true;
    }
    if ip_end < bytes.len() && bytes[ip_end] == b'.' && bytes.get(ip_end + 1) == Some(&b'*') {
        return true;
    }
    if ip_end < bytes.len()
        && bytes[ip_end] == b'-'
        && bytes.get(ip_end + 1).is_some_and(|b| b.is_ascii_alphabetic())
    {
        return true;
    }
    false
}

/// True when the IPv4 token at byte range `[ip_end - ip_len, ip_end)` in
/// `line` sits inside a quoted string literal that also contains
/// whitespace — a multi-word string (JVM banner, log message, prose),
/// not a bare network address. Network endpoints are single tokens
/// (`"10.0.0.5"`, `"10.0.0.5:8080"`, a URL); a dotted-quad embedded in a
/// sentence is a version string (`8.0.222.0`) or prose, not an address.
fn ip_in_multiword_string(line: &str, ip_end: usize, ip_len: usize) -> bool {
    let bytes = line.as_bytes();
    let ip_start = ip_end - ip_len;
    // Nearest quote to the left of the IP = opening delimiter.
    let mut l = ip_start;
    let open = loop {
        if l == 0 {
            return false;
        }
        l -= 1;
        if matches!(bytes[l], b'"' | b'\'' | b'`') {
            break bytes[l];
        }
    };
    // Matching quote to the right = closing delimiter.
    let mut r = ip_end;
    while r < bytes.len() {
        if bytes[r] == open {
            return bytes[l + 1..r].iter().any(|&b| b == b' ' || b == b'\t');
        }
        r += 1;
    }
    false
}

fn is_cidr_notation(line: &str, ip_end: usize) -> bool {
    let bytes = line.as_bytes();
    ip_end < bytes.len()
        && bytes[ip_end] == b'/'
        && bytes.get(ip_end + 1).is_some_and(|b| b.is_ascii_digit())
}

/// Names of string-introspection methods that consume a literal for its textual
/// properties rather than treating it as a value. An IP literal here is a static
/// fact about IPv4 formatting (e.g. `"101.102.103.104".len()` is the maximum
/// IPv4 string length, used as a buffer-size constant), not a network endpoint.
const INTROSPECTION_METHODS: &[&str] = &[".len()", ".as_bytes()", ".bytes()", ".chars()"];

/// True when the IPv4 token ending at byte offset `ip_end` is the receiver of a
/// string-introspection method call: the closing quote of its enclosing string
/// literal is immediately followed by `.len()`/`.as_bytes()`/`.bytes()`/`.chars()`.
fn is_string_introspection_receiver(line: &str, ip_end: usize) -> bool {
    let bytes = line.as_bytes();
    // The IP must reach the closing quote with no other content in between, so
    // the literal is exactly the IP (e.g. `"101.102.103.104"`). Find that quote.
    let after = match bytes.get(ip_end) {
        Some(b'"' | b'\'' | b'`') => ip_end + 1,
        _ => return false,
    };
    INTROSPECTION_METHODS
        .iter()
        .any(|method| line[after..].starts_with(method))
}

fn is_svg_path_data(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("d=\"")
        || trimmed.starts_with("d='")
        || trimmed.contains(" d=\"")
        || trimmed.contains(" d='")
}

fn is_in_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//")
        || trimmed.starts_with("///")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("*\t")
        || trimmed.starts_with("/**")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !(line.contains('"') || line.contains('\'') || line.contains('`')) {
                continue;
            }
            if is_svg_path_data(line) {
                continue;
            }
            let mut pos = 0;
            while let Some((next, ip)) = find_ipv4(line, pos) {
                pos = next;
                if ALLOWED.contains(&ip.as_str())
                    || is_reserved_nonroutable(&ip)
                    || is_documentation_ip(&ip)
                {
                    continue;
                }
                if is_version_like(line, next, ip.len()) {
                    continue;
                }
                if is_cidr_notation(line, next) {
                    continue;
                }
                if is_string_introspection_receiver(line, next) {
                    continue;
                }
                if ip_in_multiword_string(line, next, ip.len()) {
                    continue;
                }
                if is_in_comment(line) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-hardcoded-ip".into(),
                    message: format!("Hardcoded IP address `{ip}` — move to configuration."),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
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
    ) -> Vec<Diagnostic> {
        self.check(&CheckCtx::for_test_full(path, src, project, file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_hardcoded_ip() {
        assert_eq!(run(r#"const host = "192.168.1.1";"#).len(), 1);
    }

    #[test]
    fn allows_localhost() {
        assert!(run(r#"const host = "127.0.0.1";"#).is_empty());
    }

    #[test]
    fn allows_zero_addr() {
        assert!(run(r#"const host = "0.0.0.0";"#).is_empty());
    }

    #[test]
    fn ignores_non_string() {
        assert!(run("// 10.0.0.1 in a comment without quotes").is_empty());
    }

    #[test]
    fn allows_cidr_notation() {
        assert!(run(r#"const range = "173.245.48.0/20";"#).is_empty());
    }

    #[test]
    fn allows_svg_path_data() {
        assert!(run(r#"d="M54.25 50.974 192.168.1.1""#).is_empty());
    }

    #[test]
    fn allows_doc_comment_example() {
        assert!(run(r#"/// e.g. "192.168.1.1" is a local IP"#).is_empty());
    }

    #[test]
    fn allows_rfc5737_documentation_ips() {
        assert!(run(r#"let h = "192.0.2.60";"#).is_empty());
        assert!(run(r#"let h = "198.51.100.1";"#).is_empty());
        assert!(run(r#"let h = "203.0.113.43";"#).is_empty());
    }

    #[test]
    fn allows_broadcast() {
        assert!(run(r#"const mask = "255.255.255.255";"#).is_empty());
    }

    #[test]
    fn allows_version_with_v_prefix() {
        assert!(run(r#"let ver = "v3.1.2.0";"#).is_empty());
    }

    #[test]
    fn allows_version_with_dash_suffix() {
        assert!(run(r#"let s = "Zulu 8.40.0.25-CA-linux64";"#).is_empty());
    }

    #[test]
    fn allows_loopback_range() {
        // Issue #975: tantivy IP range-query test data.
        assert!(run(r#"let lower = Ipv4Addr::from_str("127.0.0.10").unwrap().into_ipv6_addr();"#)
            .is_empty());
        assert!(run(r#"let upper = "127.0.0.20";"#).is_empty());
    }

    #[test]
    fn allows_zero_slash_eight_range() {
        // Issue #975: tantivy query-grammar IPv6 literal embedding 0/8 quads.
        assert!(run(r#"let res1 = literal("ip:[::0.0.0.50 TO ::0.0.0.52}").expect("parse").1;"#)
            .is_empty());
    }

    #[test]
    fn flags_private_range_ips() {
        assert_eq!(run(r#"const host = "10.0.0.5";"#).len(), 1);
        assert_eq!(run(r#"const dns = "8.8.8.8";"#).len(), 1);
    }

    #[test]
    fn allows_java_version_in_banner_issue_1001() {
        let src = r#"let java_8 = "Eclipse OpenJ9 OpenJDK 64-bit Server VM (1.8.0_222-b10) from linux-amd64 JRE ... 8.0.222.0, built on Jul 17 2019";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
        assert!(run(r#"let java_11 = "Build 11.0.4.0, built on Jul 17 2019";"#).is_empty());
        assert!(run(r#"let v = "Picked up 19.2.0.1 from the environment";"#).is_empty());
    }

    #[test]
    fn flags_ip_in_single_token_string_despite_spaces_on_line() {
        assert_eq!(run(r#"const host = "192.168.1.1";"#).len(), 1);
        assert_eq!(run(r#"let endpoint = "10.0.0.5:8080";"#).len(), 1);
        assert_eq!(run(r#"let dns = "8.8.8.8";"#).len(), 1);
    }

    #[test]
    fn allows_version_with_wildcard_suffix() {
        assert!(run(r#"let cases = ["3.9.0.0.*", "3.9.1.0.*"];"#).is_empty());
        assert!(run(r#"const version = "1.2.3.4.*";"#).is_empty());
    }

    #[test]
    fn allows_ip_literal_as_string_length_constant_issue_1295() {
        // Issue #1295: the longest IPv4 string is used as a `.len()` constant to
        // document a buffer size, not as a network endpoint.
        assert!(run(r#"debug_assert_eq!(MAX_LEN, "101.102.103.104".len());"#).is_empty());
        assert!(run(r#"let n = "101.102.103.104".as_bytes().len();"#).is_empty());
        assert!(run(r#"for b in "101.102.103.104".bytes() {}"#).is_empty());
        assert!(run(r#"let c = "101.102.103.104".chars().count();"#).is_empty());
    }

    #[test]
    fn flags_ip_literal_not_used_for_introspection_issue_1295() {
        // Negative space: a bare IP literal assigned or passed to a connection
        // call must still flag — only the introspection-method receiver is exempt.
        assert_eq!(run(r#"let addr = "192.168.1.1";"#).len(), 1);
        assert_eq!(run(r#"conn.connect("10.0.0.5").await?;"#).len(), 1);
        assert_eq!(run(r#"let s = "10.0.0.5".to_string();"#).len(), 1);
    }

    #[test]
    fn allows_ip_literals_in_test_files_issue_1270() {
        // Issue #1270: literal IPs in test code are fixtures (testing IP-field
        // parsing/handling), not deployment config. `skip_in_test_dir` gates the
        // rule off for test paths — exercise that gate via `run_rule_gated`.
        use crate::rules::test_helpers::run_rule_gated;
        let src = r#"let host = IpAddr::from_str("192.168.1.1").unwrap();"#;
        assert!(run_rule_gated(&Check, src, "src/query/term_query/mod_test.rs").is_empty());
        assert!(run_rule_gated(&Check, src, "tests/ip_query.rs").is_empty());
    }

    #[test]
    fn flags_ip_literals_outside_test_files_issue_1270() {
        // Negative space: the same literal IP in production source still flags.
        use crate::rules::test_helpers::run_rule_gated;
        let src = r#"let host = IpAddr::from_str("192.168.1.1").unwrap();"#;
        assert_eq!(run_rule_gated(&Check, src, "src/server.rs").len(), 1);
    }
}
