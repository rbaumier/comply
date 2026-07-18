//! no-insecure-jwt backend for Rust.
//!
//! Flags the insecure JWT algorithms `none` and `HS256` written as string
//! literals. JWT context is established by import provenance: the check only
//! runs in a file that imports a known JWT crate (see
//! [`crate::rules::rust_helpers::JWT_CRATES`]), so an unrelated `"none"`/
//! `"HS256"` string — a compression/hashing `*Algorithm` enum, a config value —
//! in a file with no JWT dependency is never flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::file_imports_jwt_crate;

crate::ast_check! { on ["string_literal", "raw_string_literal"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    let lower = text.to_ascii_lowercase();

    let is_none = lower.contains("\"none\"");
    let is_hs256 = lower.contains("hs256");
    if !is_none && !is_hs256 {
        return;
    }

    // JWT context = the file imports a JWT library. Without this gate the
    // literal text alone cannot distinguish a JWT `alg` value from an unrelated
    // `"none"`/`"HS256"` string in a compression/hashing enum.
    if !file_imports_jwt_crate(node, source) {
        return;
    }

    if is_none {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-insecure-jwt",
            "Insecure JWT algorithm `none` — use RS256 or ES256.".into(),
            Severity::Error,
        ));
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-insecure-jwt",
        "HS256 in JWT context — prefer asymmetric algorithms (RS256, ES256).".into(),
        Severity::Error,
    ));
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_algorithm_none() {
        let src = "use jsonwebtoken::Algorithm;\nfn f() { let alg = Algorithm::from(\"none\"); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_hs256_in_jwt_context() {
        let src = "use jsonwebtoken::{Algorithm, Header};\nfn f() { let jwt_alg = \"HS256\"; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_rs256() {
        let src = "use jsonwebtoken::Algorithm;\nfn f() { let jwt_alg = \"RS256\"; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_none_under_jwt_glob_import() {
        let src = "use jwt_simple::prelude::*;\nfn f() { let alg = \"none\"; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_none_under_aliased_jwt_import() {
        let src = "use jsonwebtoken::Algorithm as Alg;\nfn f() { let alg = \"none\"; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_compression_algorithm_none_all_shapes_without_jwt_import() {
        // #7808: a message-compression enum whose `None` variant serializes to
        // "none" (Display, FromStr, Serialize, From<_> for String) in a file with
        // no JWT import must not be misread as a JWT `none` header.
        let src = r#"
use std::fmt::{Display, Formatter};

pub enum CompressionAlgorithm { None, Gzip }

impl Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionAlgorithm::None => write!(f, "none"),
            CompressionAlgorithm::Gzip => write!(f, "gzip"),
        }
    }
}

impl std::str::FromStr for CompressionAlgorithm {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "none" => Ok(CompressionAlgorithm::None),
            _ => Err(()),
        }
    }
}

impl From<CompressionAlgorithm> for String {
    fn from(a: CompressionAlgorithm) -> String {
        match a {
            CompressionAlgorithm::None => "none".to_string(),
            CompressionAlgorithm::Gzip => "gzip".to_string(),
        }
    }
}

fn serialize(a: &CompressionAlgorithm, out: &mut String) {
    if let CompressionAlgorithm::None = a {
        out.push_str("none");
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_hashing_algorithm_none_without_jwt_import() {
        let src = r#"
pub enum HashingAlgorithm { None, Sha256 }
impl HashingAlgorithm {
    fn as_str(&self) -> &str {
        match self {
            HashingAlgorithm::None => "none",
            HashingAlgorithm::Sha256 => "sha256",
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_hs256_without_jwt_import() {
        // An `algorithm` identifier on the line is not JWT context absent a JWT import.
        let src = r#"fn f() { let algorithm = "HS256"; }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_none_with_algorithm_identifier_but_no_jwt_import() {
        // A `compression_algorithm` identifier is not JWT context absent a JWT import.
        let src = r#"fn f() { let compression_algorithm = "none"; }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_none_under_local_jwt_module_import() {
        // A local module named `jwt` (first path segment `crate`) is not the
        // external `jwt` crate, so it grants no JWT provenance.
        let src = "use crate::jwt::Claims;\nfn f() { let alg = \"none\"; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_none_under_bare_crate_jwt_import() {
        let src = "use jsonwebtoken;\nfn f() { let alg = \"none\"; }";
        assert_eq!(run_on(src).len(), 1);
    }
}
