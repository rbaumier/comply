//! dockerfile-no-secrets-in-copy tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["copy_instruction"] => |node, source, ctx, diagnostics|
    // Collect all `path` children — last is destination, all before are sources.
    let mut paths: Vec<&str> = Vec::new();
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "path" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            paths.push(text);
        }
    }
    if paths.len() < 2 { return; }
    let sources = &paths[..paths.len() - 1];
    for src in sources {
        if let Some(reason) = matches_secret(src) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "COPY source `{src}` looks like a credential file ({reason}); add it to `.dockerignore`."
                ),
                severity: Severity::Error,
                span: Some((node.byte_range().start, node.byte_range().len())),
            });
            break;
        }
    }
}

fn matches_secret(src: &str) -> Option<&'static str> {
    let s = src.trim_matches('"').trim_matches('\'');
    let basename = s.rsplit('/').next().unwrap_or(s);
    if basename == ".env" || basename.starts_with(".env.") {
        return Some(".env file");
    }
    if basename.ends_with(".pem") || basename.ends_with(".key") {
        return Some("private key");
    }
    if basename == "id_rsa" || basename == "id_dsa" || basename == "id_ecdsa" || basename == "id_ed25519" {
        return Some("SSH private key");
    }
    if basename == ".npmrc" || basename == ".yarnrc" || basename == ".netrc" {
        return Some("package manager credential file");
    }
    if basename == ".aws" || s.contains("/.aws/") {
        return Some("AWS credentials");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_copy_env_file() {
        assert_eq!(run("COPY .env /app/.env\n").len(), 1);
    }

    #[test]
    fn flags_copy_pem() {
        assert_eq!(run("COPY server.pem /etc/ssl/server.pem\n").len(), 1);
    }

    #[test]
    fn flags_copy_npmrc() {
        assert_eq!(run("COPY .npmrc /root/.npmrc\n").len(), 1);
    }

    #[test]
    fn allows_regular_copy() {
        assert!(run("COPY package.json ./\n").is_empty());
    }
}
