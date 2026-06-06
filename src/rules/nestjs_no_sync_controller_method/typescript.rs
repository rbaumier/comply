//! Flag controller methods (decorated `@Get`/`@Post`/...) without `async`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "@Controller")
}

const ROUTE_DECORATORS: &[&str] = &["@Get", "@Post", "@Put", "@Patch", "@Delete", "@All", "@Options", "@Head"];

fn method_has_route_decorator(method: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let parent = match method.parent() { Some(p) => p, None => return None };
    let mut cursor = parent.walk();
    let mut last_decorator_text: Option<String> = None;
    let target_start = method.start_byte();
    for child in parent.children(&mut cursor) {
        if child.kind() == "decorator" && child.start_byte() < target_start {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if ROUTE_DECORATORS.iter().any(|d| text.starts_with(d)) {
                last_decorator_text = Some(text.to_string());
            }
        }
    }
    last_decorator_text
}

fn method_is_async(method: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = method.walk();
    for child in method.children(&mut cursor) {
        let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
        if text == "async" { return true; }
    }
    false
}

fn return_type_is_promise(method: tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(rt) = method.child_by_field_name("return_type") {
        let text = std::str::from_utf8(&source[rt.byte_range()]).unwrap_or("");
        return text.contains("Promise<") || text.contains("Observable<");
    }
    false
}

crate::ast_check! { on ["method_definition"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    let Some(deco) = method_has_route_decorator(node, source) else { return; };
    if method_is_async(node, source) { return; }
    if return_type_is_promise(node, source) { return; }
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Controller method `{name}` ({deco}) should be `async` or return a `Promise`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_sync_get_handler() {
        let src = "import { Controller, Get } from '@nestjs/common';\n@Controller() class C { @Get() find() { return []; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_handler() {
        let src = "import { Controller, Get } from '@nestjs/common';\n@Controller() class C { @Get() async find() { return []; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_return_type() {
        let src = "import { Controller, Get } from '@nestjs/common';\n@Controller() class C { @Get() find(): Promise<any> { return Promise.resolve([]); } }";
        assert!(run(src).is_empty());
    }
}
