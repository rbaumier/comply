//! OxcCheck backend for no-static-only-class.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ClassElement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        // Skip classes that extend a superclass
        if class.super_class.is_some() {
            return;
        }

        // Skip decorated classes. A class decorator (e.g. NestJS `@Module`,
        // Angular `@Injectable`) attaches runtime metadata to the class
        // identity and is read by a framework's DI/IoC container, so the
        // class form is load-bearing and cannot be replaced by a plain
        // object or functions.
        if !class.decorators.is_empty() {
            return;
        }

        let mut member_count = 0u32;
        let mut all_static = true;

        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(m) => {
                    member_count += 1;
                    if !m.r#static {
                        all_static = false;
                        break;
                    }
                }
                ClassElement::PropertyDefinition(p) => {
                    member_count += 1;
                    if !p.r#static {
                        all_static = false;
                        break;
                    }
                }
                _ => continue,
            }
        }

        if member_count == 0 || !all_static {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use an object or plain functions instead of a class with only static members."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_static_only_methods() {
        let d = run("class Foo { static bar() {} static baz() {} }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-static-only-class");
    }

    #[test]
    fn allows_class_with_instance_member() {
        assert!(run("class Foo { static bar() {} baz() {} }").is_empty());
    }

    #[test]
    fn allows_class_extending_superclass() {
        assert!(run("class Foo extends Base { static bar() {} }").is_empty());
    }

    #[test]
    fn allows_decorated_nest_dynamic_module() {
        // NestJS dynamic module: `@Module({})`-decorated class exposing only a
        // static `forRoot()` factory. The class form is required by Nest's IoC
        // container, so it must not be flagged.
        let src = "@Module({})\n\
                   class DatabaseModule {\n\
                     static forRoot(): DynamicModule {\n\
                       return { module: DatabaseModule, providers: [] };\n\
                     }\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_decorated_class_with_only_static_members() {
        assert!(run("@Injectable()\nclass Foo { static bar() {} }").is_empty());
    }
}
