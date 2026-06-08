use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["extends"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };

        let Some(super_class) = &class.super_class else {
            return;
        };

        let parent_name = match super_class {
            Expression::Identifier(ident) => ident.name.to_string(),
            Expression::StaticMemberExpression(member) => {
                let span = member.span;
                ctx.source[span.start as usize..span.end as usize].to_string()
            }
            _ => return,
        };

        // Allow extending Error types
        if parent_name.to_lowercase().contains("error") {
            return;
        }
        if is_framework_extension_path(ctx.path)
            || has_framework_import_for(&parent_name, ctx.source)
            || is_framework_base_class(&parent_name)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Class inheritance via `extends` — prefer composition over inheritance.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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
    fn allows_extends_error() {
        assert!(run_on("class MyError extends Error {}").is_empty());
    }

    #[test]
    fn allows_extends_custom_error() {
        assert!(run_on("class ValidationError extends CustomError {}").is_empty());
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
