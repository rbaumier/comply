//! Flag `ngOnInit`/`ngAfterViewInit`/etc. methods on a class decorated with
//! `@Injectable()` (with no `@Component`/`@Directive` decorator).

use crate::diagnostic::{Diagnostic, Severity};

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

fn class_decorators_text(class: tree_sitter::Node, source: &[u8]) -> String {
    // Decorators can appear as direct children of the class_declaration
    // (TS grammar) OR as preceding siblings inside an export_statement.
    let mut out = String::new();
    let mut cursor = class.walk();
    for child in class.children(&mut cursor) {
        if child.kind() == "decorator" {
            out.push_str(std::str::from_utf8(&source[child.byte_range()]).unwrap_or(""));
            out.push('\n');
        }
    }
    if let Some(parent) = class.parent() {
        let mut cur = parent.walk();
        for child in parent.children(&mut cur) {
            if child.kind() == "decorator" {
                out.push_str(std::str::from_utf8(&source[child.byte_range()]).unwrap_or(""));
                out.push('\n');
            }
        }
    }
    out
}

crate::ast_check! { on ["method_definition"] prefilter = ["@Injectable"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
    if !COMPONENT_LIFECYCLE_HOOKS.contains(&name) { return; }
    let mut cur = node;
    let class = loop {
        match cur.parent() {
            Some(p) if p.kind() == "class_declaration" => break p,
            Some(p) => cur = p,
            None => return,
        }
    };
    let decos = class_decorators_text(class, source);
    if !decos.contains("@Injectable") { return; }
    if decos.contains("@Component") || decos.contains("@Directive") || decos.contains("@Pipe") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("`{name}` is a component lifecycle hook — it is never invoked on an `@Injectable()` service."),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
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
