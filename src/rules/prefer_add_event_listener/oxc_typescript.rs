//! prefer-add-event-listener oxc backend — flag an `X.onEVENT = handler`
//! assignment as a DOM-handler smell, EXCEPT a self-referential rebind
//! `recv.onX = recv.onX.bind(...)`, which is method binding (e.g. a state
//! class binding its own method so it can be passed as a prop), not DOM.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, StaticMemberExpression};
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

        // `recv.onX = recv.onX.bind(...)` rebinds the SAME member being assigned —
        // method rebinding (e.g. a state-class binding its own method so it can be
        // passed as a prop), never a DOM-handler attachment. The bound member must
        // structurally equal the assignment target (same property AND same receiver).
        if rhs_is_self_bind(member, &assign.right) {
            return;
        }

        // `recv.onX = null` / `= undefined` DETACHES a handler; there is no
        // single-statement `addEventListener` form for removal (its counterpart
        // is `removeEventListener(type, ref)`, needing a stored reference this
        // site never created), so `.onX = null` is the canonical idiom.
        if rhs_is_detach(&assign.right) {
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

/// True when `rhs` is `<recv>.<prop>.bind(...)` whose bound member is the SAME
/// member as the assignment target `lhs` (same property name and structurally
/// equal receiver) — i.e. `this.onclick = this.onclick.bind(this)`.
fn rhs_is_self_bind(lhs: &StaticMemberExpression, rhs: &Expression) -> bool {
    let Expression::CallExpression(call) = rhs else {
        return false;
    };
    let Expression::StaticMemberExpression(bind_member) = &call.callee else {
        return false;
    };
    if bind_member.property.name.as_str() != "bind" {
        return false;
    }
    let Expression::StaticMemberExpression(bound) = &bind_member.object else {
        return false;
    };
    bound.property.name.as_str() == lhs.property.name.as_str()
        && receivers_equal(&bound.object, &lhs.object)
}

/// True when `rhs` ultimately assigns `null` or the `undefined` identifier —
/// a handler removal (detach). Follows chained assignments (`a = b = null`, where
/// the outer RHS is itself an `AssignmentExpression`) down to the terminal value.
fn rhs_is_detach(rhs: &Expression) -> bool {
    let mut terminal = rhs.without_parentheses();
    while let Expression::AssignmentExpression(inner) = terminal {
        terminal = inner.right.without_parentheses();
    }
    matches!(terminal, Expression::NullLiteral(_))
        || matches!(terminal, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

/// Structural equality for member-access receivers, limited to the forms that
/// appear in a self-rebind: `this` and a bare identifier (`el.onX = el.onX...`).
fn receivers_equal(a: &Expression, b: &Expression) -> bool {
    match (a, b) {
        (Expression::ThisExpression(_), Expression::ThisExpression(_)) => true,
        (Expression::Identifier(x), Expression::Identifier(y)) => x.name.as_str() == y.name.as_str(),
        _ => false,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_genuine_dom_handler_assignment() {
        let d = run_on("el.onclick = handler;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-add-event-listener");
    }

    #[test]
    fn allows_this_self_bind() {
        // #3731: `this.onfocus = this.onfocus.bind(this)` rebinds the object's
        // own method (state class), not a DOM-handler attachment.
        let src = "class S { constructor() { this.onfocus = this.onfocus.bind(this); } onfocus(_) {} }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identifier_self_bind() {
        // #3731: same shape with a bare identifier receiver.
        assert!(run_on("el.onclick = el.onclick.bind(this);").is_empty());
    }

    #[test]
    fn flags_non_bind_rhs() {
        // RHS is not a `.bind` of the same member → genuine handler assignment.
        let src = "class S { constructor() { this.onclick = someOtherHandler; } }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_cross_receiver_bind() {
        // The bound receiver `other` ≠ the target receiver `el` → not a self-bind.
        let d = run_on("el.onclick = other.onclick.bind(this);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_null_detach() {
        // #7507: `.onX = null` removes a handler; no `addEventListener` form exists.
        assert!(run_on("document.onmouseup = null;").is_empty());
    }

    #[test]
    fn allows_undefined_detach() {
        // #7507: `.onX = undefined` is likewise a removal, not an attachment.
        assert!(run_on("el.onclick = undefined;").is_empty());
    }

    #[test]
    fn allows_chained_null_detach() {
        // #7507 exact case: both `onmousemove` and `onmouseup` are detached — the
        // outer RHS is itself an assignment whose terminal value is `null`, so both
        // sites must be suppressed (not just the innermost `onmouseup = null`).
        assert!(run_on("document.onmousemove = document.onmouseup = null;").is_empty());
    }

    #[test]
    fn flags_chained_real_handler() {
        // Chained assignment whose terminal is a real handler still attaches at
        // both sites → both flagged.
        let d = run_on("a.onclick = b.onclick = someHandler;");
        assert_eq!(d.len(), 2);
    }
}
