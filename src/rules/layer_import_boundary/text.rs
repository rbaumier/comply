//! layer-import-boundary backend — enforce hexagonal architecture:
//!
//! - `domain/` cannot import from `infrastructure/` or `application/`
//! - `application/` cannot import from `infrastructure/`
//! - `infrastructure/` can import from anything

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

#[derive(Debug, Clone, Copy)]
enum Layer {
    Domain,
    Application,
    Infrastructure,
}

fn detect_layer(path: &str) -> Option<Layer> {
    // Normalize separators.
    let normalized = path.replace('\\', "/");
    // Check path segments.
    for segment in normalized.split('/') {
        match segment {
            "domain" => return Some(Layer::Domain),
            "application" => return Some(Layer::Application),
            "infrastructure" => return Some(Layer::Infrastructure),
            _ => {}
        }
    }
    None
}

fn import_source_layer(import_path: &str) -> Option<Layer> {
    let normalized = import_path.replace('\\', "/");
    if normalized.contains("infrastructure/") || normalized.contains("/infrastructure") {
        return Some(Layer::Infrastructure);
    }
    if normalized.contains("application/") || normalized.contains("/application") {
        return Some(Layer::Application);
    }
    if normalized.contains("domain/") || normalized.contains("/domain") {
        return Some(Layer::Domain);
    }
    None
}

fn is_forbidden(file_layer: Layer, import_layer: Layer) -> bool {
    matches!(
        (file_layer, import_layer),
        (Layer::Domain, Layer::Infrastructure)
            | (Layer::Domain, Layer::Application)
            | (Layer::Application, Layer::Infrastructure)
    )
}

fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Domain => "domain",
        Layer::Application => "application",
        Layer::Infrastructure => "infrastructure",
    }
}

/// Extract the import/require source string from a line.
fn extract_import_source(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    // import ... from "..."
    if (trimmed.starts_with("import") || trimmed.starts_with("export"))
        && let Some(from_idx) = trimmed.find("from")
    {
        let after = trimmed[from_idx + 4..].trim();
        return extract_string_literal(after);
    }
    // require("...")
    if let Some(req_idx) = trimmed.find("require(") {
        let after = trimmed[req_idx + 8..].trim();
        return extract_string_literal(after);
    }
    None
}

fn extract_string_literal(s: &str) -> Option<&str> {
    let quote = s.as_bytes().first()?;
    if !matches!(quote, b'"' | b'\'' | b'`') {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find(*quote as char)?;
    Some(&rest[..end])
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path_str = ctx.path.to_string_lossy();
        let Some(file_layer) = detect_layer(&path_str) else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let Some(source) = extract_import_source(line) else {
                continue;
            };
            let Some(import_layer) = import_source_layer(source) else {
                continue;
            };
            if is_forbidden(file_layer, import_layer) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "layer-import-boundary".into(),
                    message: format!(
                        "`{}` layer must not import from `{}` layer.",
                        layer_name(file_layer),
                        layer_name(import_layer),
                    ),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_with_path(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_domain_importing_infrastructure() {
        let diags = run_with_path(
            "src/domain/user.ts",
            "import { db } from '../infrastructure/database';",
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("domain"));
        assert!(diags[0].message.contains("infrastructure"));
    }

    #[test]
    fn flags_domain_importing_application() {
        let diags = run_with_path(
            "src/domain/user.ts",
            "import { handler } from '../application/userHandler';",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_application_importing_infrastructure() {
        let diags = run_with_path(
            "src/application/userService.ts",
            "import { pg } from '../infrastructure/pg';",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_infrastructure_importing_domain() {
        let diags = run_with_path(
            "src/infrastructure/repo.ts",
            "import { User } from '../domain/user';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_domain_importing_domain() {
        let diags = run_with_path(
            "src/domain/order.ts",
            "import { User } from './user';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_files_outside_layers() {
        let diags = run_with_path(
            "src/utils/helper.ts",
            "import { db } from '../infrastructure/database';",
        );
        assert!(diags.is_empty());
    }
}
