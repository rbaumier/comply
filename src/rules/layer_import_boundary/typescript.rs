//! layer-import-boundary backend — enforce hexagonal architecture:
//!
//! - `domain/` cannot import from `infrastructure/` or `application/`
//! - `application/` cannot import from `infrastructure/`
//! - `infrastructure/` can import from anything

use crate::diagnostic::{Diagnostic, Severity};

#[derive(Debug, Clone, Copy)]
enum Layer {
    Domain,
    Application,
    Infrastructure,
}

fn detect_layer(path: &str) -> Option<Layer> {
    let normalized = path.replace('\\', "/");
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

crate::ast_check! { on ["import_statement", "export_statement"] => |node, source, ctx, diagnostics|
    // import_statement covers `import x from '...'` and `import '...';`.
    // export_statement covers `export ... from '...'` re-exports.
    let Some(src_node) = node.child_by_field_name("source") else { return };

    let path_str = ctx.path.to_string_lossy();
    let Some(file_layer) = detect_layer(&path_str) else { return };

    let raw = src_node.utf8_text(source).unwrap_or("");
    let spec = raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`');
    let Some(import_layer) = import_source_layer(spec) else { return };

    if !is_forbidden(file_layer, import_layer) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &src_node,
        super::META.id,
        format!(
            "`{}` layer must not import from `{}` layer.",
            layer_name(file_layer),
            layer_name(import_layer),
        ),
        Severity::Warning,
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

    fn run_with_path(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
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
        let diags = run_with_path("src/domain/order.ts", "import { User } from './user';");
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
