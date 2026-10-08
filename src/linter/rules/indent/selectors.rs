//! Node-type selectors for indentation exclusions.
use oxc::ast::AstKind;

fn node_type(kind: AstKind<'_>) -> String {
    match kind {
        AstKind::IdentifierName(_)
        | AstKind::IdentifierReference(_)
        | AstKind::BindingIdentifier(_)
        | AstKind::LabelIdentifier(_) => "Identifier".into(),
        AstKind::ComputedMemberExpression(_)
        | AstKind::StaticMemberExpression(_)
        | AstKind::PrivateFieldExpression(_) => "MemberExpression".into(),
        AstKind::ObjectProperty(_)
        | AstKind::BindingProperty(_)
        | AstKind::AssignmentTargetPropertyIdentifier(_)
        | AstKind::AssignmentTargetPropertyProperty(_) => "Property".into(),
        AstKind::ArrayAssignmentTarget(_) => "ArrayPattern".into(),
        AstKind::ObjectAssignmentTarget(_) => "ObjectPattern".into(),
        AstKind::BindingRestElement(_) | AstKind::AssignmentTargetRest(_) => "RestElement".into(),
        AstKind::AssignmentTargetWithDefault(_) | AstKind::AssignmentPattern(_) => {
            "AssignmentPattern".into()
        }
        AstKind::Function(n) => if n.is_function_declaration() {
            "FunctionDeclaration"
        } else {
            "FunctionExpression"
        }
        .into(),
        AstKind::Class(n) => if n.is_declaration() {
            "ClassDeclaration"
        } else {
            "ClassExpression"
        }
        .into(),
        AstKind::FunctionBody(_) => "BlockStatement".into(),
        AstKind::BooleanLiteral(_)
        | AstKind::NullLiteral(_)
        | AstKind::NumericLiteral(_)
        | AstKind::StringLiteral(_)
        | AstKind::BigIntLiteral(_)
        | AstKind::RegExpLiteral(_) => "Literal".into(),
        _ => format!("{:?}", kind.ty()),
    }
}
/// Match comma-separated node types with child or descendant relationships.
/// More complex esquery selectors are retained as compatibility debt.
pub(super) fn matches(selector: &str, kind: AstKind<'_>, ancestors: &[AstKind<'_>]) -> bool {
    let name = node_type(kind);
    let ancestors = ancestors
        .iter()
        .copied()
        .filter(|kind| {
            !matches!(
                kind,
                AstKind::FormalParameters(_)
                    | AstKind::FormalParameter(_)
                    | AstKind::CatchParameter(_)
                    | AstKind::ParenthesizedExpression(_)
            )
        })
        .collect::<Vec<_>>();
    selector.split(',').any(|s| {
        let spaced = s.replace('>', " > ");
        let parts = spaced.split_whitespace().collect::<Vec<_>>();
        let Some(last) = parts.last() else {
            return false;
        };
        if *last != "*" && *last != name {
            return false;
        }
        let mut a = 0;
        let mut p = parts.len() - 1;
        while p > 0 {
            p -= 1;
            let immediate = parts[p] == ">";
            if immediate {
                if p == 0 {
                    return false;
                }
                p -= 1;
            }
            if immediate {
                let Some(parent) = ancestors.get(a) else {
                    return false;
                };
                if parts[p] != "*" && parts[p] != node_type(*parent) {
                    return false;
                }
                a += 1;
            } else {
                let Some(index) = ancestors[a..]
                    .iter()
                    .position(|k| parts[p] == "*" || parts[p] == node_type(*k))
                else {
                    return false;
                };
                a += index + 1;
            }
        }
        true
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use oxc::{allocator::Allocator, parser::Parser, semantic::SemanticBuilder, span::SourceType};
    #[test]
    fn node_type_and_ancestry_selectors() {
        let a = Allocator::default();
        let p = Parser::new(&a, "const x = f(g());", SourceType::ts()).parse();
        let s = SemanticBuilder::new().build(&p.program).semantic;
        let n = s
            .nodes()
            .iter()
            .find(|n| matches!(n.kind(),AstKind::CallExpression(c) if c.span.start==12))
            .unwrap();
        let ancestors = s
            .nodes()
            .ancestors(n.id())
            .map(|n| n.kind())
            .collect::<Vec<_>>();
        assert!(matches("CallExpression", n.kind(), &ancestors));
        assert!(matches(
            "VariableDeclarator CallExpression",
            n.kind(),
            &ancestors
        ));
        assert!(matches(
            "CallExpression > CallExpression",
            n.kind(),
            &ancestors
        ));
        assert!(matches(
            "CallExpression>CallExpression",
            n.kind(),
            &ancestors
        ));
        assert!(!matches(
            "VariableDeclarator > CallExpression",
            n.kind(),
            &ancestors
        ));
        let a = Allocator::default();
        let p = Parser::new(&a, "function f(x) {}", SourceType::ts()).parse();
        let s = SemanticBuilder::new().build(&p.program).semantic;
        let n = s
            .nodes()
            .iter()
            .find(|n| matches!(n.kind(), AstKind::BindingIdentifier(id) if id.name == "x"))
            .unwrap();
        let ancestors = s
            .nodes()
            .ancestors(n.id())
            .map(|n| n.kind())
            .collect::<Vec<_>>();
        assert!(matches(
            "FunctionDeclaration>Identifier",
            n.kind(),
            &ancestors
        ));
    }
}
