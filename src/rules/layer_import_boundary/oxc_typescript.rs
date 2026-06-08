//! layer-import-boundary oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ImportDeclaration,
            AstType::ExportNamedDeclaration,
            AstType::ExportAllDeclaration,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (source_value, source_span) = match node.kind() {
            AstKind::ImportDeclaration(import) => {
                (import.source.value.as_str(), import.source.span)
            }
            AstKind::ExportNamedDeclaration(export) => {
                let Some(src) = &export.source else { return };
                (src.value.as_str(), src.span)
            }
            AstKind::ExportAllDeclaration(export) => {
                (export.source.value.as_str(), export.source.span)
            }
            _ => return,
        };

        let path_str = ctx.path.to_string_lossy();
        let Some(file_layer) = detect_layer(&path_str) else {
            return;
        };
        let Some(import_layer) = import_source_layer(source_value) else {
            return;
        };
        if !is_forbidden(file_layer, import_layer) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, source_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` layer must not import from `{}` layer.",
                layer_name(file_layer),
                layer_name(import_layer),
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_with_path(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
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
