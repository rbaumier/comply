//! prefer-add-event-listener oxc backend — flag `element.onclick = handler` style assignments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::AssignmentTarget;
use std::sync::Arc;

const ON_EVENTS: &[&str] = &[
    "onclick",
    "ondblclick",
    "onmousedown",
    "onmouseup",
    "onmouseover",
    "onmouseout",
    "onmousemove",
    "onmouseenter",
    "onmouseleave",
    "onkeydown",
    "onkeyup",
    "onkeypress",
    "onfocus",
    "onblur",
    "onchange",
    "oninput",
    "onsubmit",
    "onreset",
    "onscroll",
    "onresize",
    "onload",
    "onunload",
    "onbeforeunload",
    "onerror",
    "onabort",
    "ondrag",
    "ondragstart",
    "ondragend",
    "ondragover",
    "ondragenter",
    "ondragleave",
    "ondrop",
    "ontouchstart",
    "ontouchend",
    "ontouchmove",
    "ontouchcancel",
    "onpointerdown",
    "onpointerup",
    "onpointermove",
    "onpointerover",
    "onpointerout",
    "onpointerenter",
    "onpointerleave",
    "oncontextmenu",
    "onwheel",
    "onanimationstart",
    "onanimationend",
    "onanimationiteration",
    "ontransitionend",
    "onmessage",
    "onclose",
    "onopen",
    "onhashchange",
    "onpopstate",
    "onstorage",
    "onselect",
    "oncopy",
    "oncut",
    "onpaste",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return;
        };

        let prop_name = member.property.name.as_str();

        if !ON_EVENTS.contains(&prop_name) {
            return;
        }

        let event_name = &prop_name[2..]; // strip "on"
        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `addEventListener('{event_name}', handler)` over `.{prop_name} = handler`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
