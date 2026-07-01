use crate::parser::ast::{Expression, Statement};

pub fn statement_location(statement: &Statement) -> (usize, usize) {
    match statement {
        Statement::Let {
            name_line,
            name_column,
            ..
        } => (*name_line, *name_column),
        Statement::Assignment { value, .. }
        | Statement::Expression(value)
        | Statement::Drop { value }
        | Statement::New {
            value: Some(value), ..
        }
        | Statement::Own {
            value: Some(value), ..
        }
        | Statement::Move { value, .. } => expression_location(value),
        Statement::Return(Some(value)) => expression_location(value),
        Statement::If { condition, .. } | Statement::While { condition, .. } => {
            expression_location(condition)
        }
        Statement::For { iterable, .. } | Statement::Find { iterable, .. } => {
            expression_location(iterable)
        }
        Statement::Match { value, .. } => expression_location(value),
        _ => (1, 1),
    }
}

pub fn expression_location(expression: &Expression) -> (usize, usize) {
    match expression {
        Expression::Identifier(ident) => (ident.line.max(1), ident.column.max(1)),
        Expression::BinaryOp { left, .. }
        | Expression::NamedArg { value: left, .. }
        | Expression::Reference { expr: left, .. }
        | Expression::Dereference { expr: left, .. }
        | Expression::Box { value: left, .. }
        | Expression::Pipeline { input: left, .. }
        | Expression::Try { expr: left, .. }
        | Expression::Ok { value: left, .. }
        | Expression::Err { value: left, .. } => expression_location(left),
        Expression::UnaryOp { operand, .. } => expression_location(operand),
        Expression::Call { args, .. }
        | Expression::List { elements: args, .. }
        | Expression::Tuple { elements: args, .. } => {
            args.first().map(expression_location).unwrap_or((1, 1))
        }
        Expression::Dict { entries, .. } => entries
            .first()
            .map(|(key, _)| expression_location(key))
            .unwrap_or((1, 1)),
        Expression::Index { target, .. } | Expression::MemberAccess { target, .. } => {
            expression_location(target)
        }
        Expression::Closure { body, .. } => body.first().map(statement_location).unwrap_or((1, 1)),
        Expression::Match { value, .. } => expression_location(value),
        Expression::EnumVariant { payloads, .. } => {
            payloads.first().map(expression_location).unwrap_or((1, 1))
        }
        Expression::Literal(_) | Expression::EnumVariantPath { .. } => (1, 1),
    }
}
