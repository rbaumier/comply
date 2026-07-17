//! nestjs-no-sync-controller-method OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "@Controller")
}

const ROUTE_DECORATORS: &[&str] = &[
    "@Get", "@Post", "@Put", "@Patch", "@Delete", "@All", "@Options", "@Head",
];

fn method_has_route_decorator(
    method: &oxc_ast::ast::MethodDefinition,
    source: &str,
) -> Option<String> {
    for dec in &method.decorators {
        let dec_text = &source[dec.span.start as usize..dec.span.end as usize];
        if ROUTE_DECORATORS.iter().any(|d| dec_text.starts_with(d)) {
            return Some(dec_text.to_string());
        }
    }
    None
}

fn return_type_is_promise(method: &oxc_ast::ast::MethodDefinition, source: &str) -> bool {
    if let Some(ret_type) = &method.value.return_type {
        let text = &source[ret_type.span.start as usize..ret_type.span.end as usize];
        return text.contains("Promise<") || text.contains("Observable<");
    }
    false
}

/// The callee identifier name of a decorator: `Foo` for both the call form
/// `@Foo(...)` and the bare form `@Foo`. `None` for member-expression or other
/// non-identifier decorator heads.
fn decorator_callee_name<'d>(decorator: &'d oxc_ast::ast::Decorator<'_>) -> Option<&'d str> {
    let callee = match &decorator.expression {
        Expression::CallExpression(call) => &call.callee,
        other => other,
    };
    match callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// True when a parameter carries `@Res()`/`@Response()` (or their bare forms) —
/// NestJS's library-specific response mode. In that mode NestJS discards the
/// handler's return value: the handler writes the response itself
/// (`response.send(...)`, `response.redirect(...)`), so there is no returned
/// value to await and the async-uniformity premise does not apply.
fn has_manual_response_param(method: &oxc_ast::ast::MethodDefinition) -> bool {
    method.value.params.items.iter().any(|param| {
        param
            .decorators
            .iter()
            .any(|dec| matches!(decorator_callee_name(dec), Some("Res" | "Response")))
    })
}

/// True when an empty-bodied method carries a method-level `@UseGuards(...)` — a
/// Passport strategy initiator (e.g. `@UseGuards(AuthGuard('google'))`): the
/// guard performs the redirect, so the body is intentionally empty and
/// synchronous with nothing to await.
fn is_empty_guard_initiator(method: &oxc_ast::ast::MethodDefinition) -> bool {
    let has_use_guards = method
        .decorators
        .iter()
        .any(|dec| matches!(decorator_callee_name(dec), Some("UseGuards")));
    if !has_use_guards {
        return false;
    }
    match &method.value.body {
        Some(body) => body.statements.is_empty(),
        None => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_nestjs_file(ctx.source) {
            return;
        }
        let AstKind::MethodDefinition(method) = node.kind() else { return };

        let Some(deco) = method_has_route_decorator(method, ctx.source) else {
            return;
        };

        if method.value.r#async {
            return;
        }
        if return_type_is_promise(method, ctx.source) {
            return;
        }
        if has_manual_response_param(method) {
            return;
        }
        if is_empty_guard_initiator(method) {
            return;
        }

        let name = match &method.key {
            PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
            _ => return,
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, method.key.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Controller method `{name}` ({deco}) should be `async` or return a `Promise`."
            ),
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
        crate::rules::test_helpers::run_rule(&Check, source, "ctrl.ts")
    }

    #[test]
    fn flags_plain_sync_handler() {
        // No @Res param, non-empty body returning a plain value → true positive.
        let src = "@Controller('x') class C { @Get() getUser(): User { return this.svc.find(); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decorated_non_res_param_handler() {
        // A param decorator that is not @Res/@Response (here @Body) does not
        // switch NestJS into manual-response mode, so a sync handler still flags.
        let src = "@Controller('x') class C { @Post() create(@Body() dto: CreateDto): CreateDto { return dto; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_body_without_guards() {
        // Empty body but no @UseGuards → not a guard initiator, still flags.
        let src = "@Controller('x') class C { @Get() ping() {} }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_res_manual_response_void_handler() {
        // Issue #7643: @Res() manual-response handler declaring `: void`.
        let src = "@Controller('x') class C {\n\
            @Get() getWebManifest(@Param('languageCode') languageCode: string, @Res() response: Response): void {\n\
              response.setHeader('Content-Type', 'application/json');\n\
              response.send(webManifest);\n\
            }\n\
        }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_res_manual_response_no_return_type() {
        // getSitemapXml(@Res() response) — manual response, no return annotation.
        let src = "@Controller('x') class C { @Get() getSitemapXml(@Res() response: Response) { response.send(xml); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_res_decorator() {
        // Bare `@Res` (no call parens) also switches to manual-response mode.
        let src = "@Controller('x') class C { @Get() getSitemapXml(@Res response: Response) { response.send(xml); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_response_alias_decorator() {
        // `@Response()` is the alias of `@Res()`.
        let src = "@Controller('x') class C { @Get() m(@Response() response: Response) { response.send(x); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_req_and_res_callback() {
        // googleLoginCallback(@Req() request, @Res() response) — @Res present.
        let src = "@Controller('x') class C { @Get() googleLoginCallback(@Req() request, @Res() response): void { response.redirect(url); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_non_empty_body_with_guards() {
        // @UseGuards but a non-empty body returning a plain value → still flags;
        // the guard-initiator skip is gated on an empty body.
        let src = "@Controller('x') class C { @Get() @UseGuards(AuthGuard('x')) list(): User[] { return this.svc.all(); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_empty_guard_initiator() {
        // googleLogin() — empty body + @UseGuards(AuthGuard('google')) initiator.
        let src = "@Controller('x') class C { @Get() @UseGuards(AuthGuard('google')) googleLogin() {} }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_async_handler() {
        let src = "@Controller('x') class C { @Get() async list(): Promise<User[]> { return []; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_nestjs_files() {
        let src = "class C { @Get() getUser(): User { return x; } }";
        assert!(run(src).is_empty());
    }
}
