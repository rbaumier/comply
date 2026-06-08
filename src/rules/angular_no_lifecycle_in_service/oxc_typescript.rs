//! angular-no-lifecycle-in-service OXC backend — flag component lifecycle hooks
//! on `@Injectable()` classes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const COMPONENT_LIFECYCLE_HOOKS: &[&str] = &[
    "ngOnInit",
    "ngAfterViewInit",
    "ngAfterViewChecked",
    "ngAfterContentInit",
    "ngAfterContentChecked",
    "ngOnChanges",
    "ngDoCheck",
];

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/") || crate::oxc_helpers::source_contains(source, "@Injectable")
}

fn decorator_text_contains(source: &str, decorators: &oxc_allocator::Vec<'_, oxc_ast::ast::Decorator<'_>>, needle: &str) -> bool {
    for dec in decorators {
        let text = &source[dec.span.start as usize..dec.span.end as usize];
        if text.contains(needle) {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Injectable"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };
        if !is_angular_file(ctx.source) {
            return;
        }

        // Must have @Injectable, must NOT have @Component/@Directive/@Pipe
        if !decorator_text_contains(ctx.source, &class.decorators, "@Injectable") {
            return;
        }
        if decorator_text_contains(ctx.source, &class.decorators, "@Component")
            || decorator_text_contains(ctx.source, &class.decorators, "@Directive")
            || decorator_text_contains(ctx.source, &class.decorators, "@Pipe")
        {
            return;
        }

        // Check methods for component lifecycle hooks
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else { continue };
            let name = match &method.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                _ => continue,
            };
            if !COMPONENT_LIFECYCLE_HOOKS.contains(&name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, method.key.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}` is a component lifecycle hook — it is never invoked on an `@Injectable()` service."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_ng_on_init_in_injectable() {
        let src = "import { Injectable } from '@angular/core';\n@Injectable() class S { ngOnInit() {} }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_ng_on_init_in_component() {
        let src = "import { Component } from '@angular/core';\n@Component({}) class C { ngOnInit() {} }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_ng_on_destroy_in_service() {
        let src = "import { Injectable } from '@angular/core';\n@Injectable() class S { ngOnDestroy() {} }";
        assert!(run(src).is_empty());
    }
}
