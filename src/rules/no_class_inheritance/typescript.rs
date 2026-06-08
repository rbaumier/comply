//! no-class-inheritance backend — flag `class Foo extends Bar`.
//!
//! Exception: extending Error types is allowed (Error, CustomError, TaggedError, etc.)

use crate::diagnostic::{Diagnostic, Severity};

const FRAMEWORK_IMPORT_MARKERS: &[&str] = &["@nestjs/", "@angular/", "typeorm"];
const FRAMEWORK_FILE_SUFFIXES: &[&str] = &[
    ".adapter.ts",
    ".controller.ts",
    ".filter.ts",
    ".guard.ts",
    ".interceptor.ts",
    ".module.ts",
    ".pipe.ts",
    ".resolver.ts",
    ".service.ts",
    ".strategy.ts",
];
const FRAMEWORK_BASE_CLASSES: &[&str] = &[
    "AbstractHttpAdapter",
    "AuthGuard",
    "BaseExceptionFilter",
    "CacheInterceptor",
    "ClientProxy",
    "ClientTCP",
    "ContextCreator",
    "Server",
];

fn inherited_class_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "class_heritage" {
            continue;
        }
        let mut hcursor = child.walk();
        for heritage_child in child.children(&mut hcursor) {
            if heritage_child.kind() != "extends_clause" {
                continue;
            }
            let mut ecursor = heritage_child.walk();
            for extends_child in heritage_child.children(&mut ecursor) {
                if matches!(extends_child.kind(), "identifier" | "member_expression") {
                    return extends_child.utf8_text(source).ok().map(str::to_owned);
                }
            }
        }
    }
    None
}

fn is_framework_extension_path(path: &std::path::Path) -> bool {
    let normalized = path.to_string_lossy().to_lowercase();
    FRAMEWORK_FILE_SUFFIXES
        .iter()
        .any(|suffix| normalized.ends_with(suffix))
}

fn has_framework_import_for(parent_name: &str, source: &str) -> bool {
    source.lines().any(|line| {
        line.trim_start().starts_with("import ")
            && line.contains(parent_name)
            && FRAMEWORK_IMPORT_MARKERS
                .iter()
                .any(|marker| line.contains(marker))
    })
}

fn is_framework_base_class(parent_name: &str) -> bool {
    let short_name = parent_name.rsplit('.').next().unwrap_or(parent_name);
    FRAMEWORK_BASE_CLASSES.contains(&short_name)
}

crate::ast_check! { on ["class_declaration", "class"] prefilter = ["extends"] => |node, source, ctx, diagnostics|
    let Some(parent_name) = inherited_class_name(node, source) else {
        return;
    };

    // Allow extending Error types (Error, CustomError, TaggedError, etc.)
    if parent_name.to_lowercase().contains("error") {
        return;
    }
    if is_framework_extension_path(ctx.path)
        || has_framework_import_for(&parent_name, ctx.source)
        || is_framework_base_class(&parent_name)
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-class-inheritance".into(),
        message: "Class inheritance via `extends` — prefer composition over inheritance.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_class_extends() {
        assert_eq!(run_on("class Dog extends Animal {}").len(), 1);
    }

    #[test]
    fn flags_export_class_extends() {
        assert_eq!(run_on("export class Foo extends Base {}").len(), 1);
    }

    #[test]
    fn allows_class_without_extends() {
        assert!(run_on("class Foo {}").is_empty());
    }

    #[test]
    fn allows_class_expression_without_extends() {
        assert!(run_on("const Foo = class {};").is_empty());
    }

    #[test]
    fn allows_extends_error() {
        assert!(run_on("class MyError extends Error {}").is_empty());
    }

    #[test]
    fn allows_extends_custom_error() {
        assert!(run_on("class ValidationError extends CustomError {}").is_empty());
    }

    #[test]
    fn allows_extends_tagged_error() {
        assert!(run_on("class ApiError extends TaggedError {}").is_empty());
    }

    #[test]
    fn allows_extends_base_error() {
        assert!(run_on("class NotFoundError extends BaseError {}").is_empty());
    }

    #[test]
    fn allows_extends_framework_imported_base_class() {
        let src = "import { ContextCreator } from '@nestjs/core';\nclass C extends ContextCreator {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_extends_framework_base_name() {
        assert!(run_on("class GrpcServer extends Server {}").is_empty());
    }

    #[test]
    fn allows_extends_in_framework_extension_file() {
        let src = "class JwtGuard extends AuthGuard {}";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "auth.guard.ts");
        assert!(d.is_empty());
    }
}
