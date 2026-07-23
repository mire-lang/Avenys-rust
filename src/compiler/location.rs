use crate::error::Span;
use crate::parser::ast::{Expression, Statement};

/// Compatibility alias — prefer `Span::unknown()` in new code.
pub const NO_POSITION: Span = Span { line: 0, column: 0 };

pub fn statement_location(statement: &Statement) -> Span {
    match statement {
        Statement::Let {
            name_line,
            name_column,
            ..
        } => Span::new(*name_line, *name_column),
        Statement::Assignment { line, column, .. } => Span::new(*line, *column),
        Statement::Expression(value)
        | Statement::Drop { value }
        | Statement::New {
            value: Some(value), ..
        }
        | Statement::Own {
            value: Some(value), ..
        }
        | Statement::Move { value, .. } => expression_location(value),
        Statement::Function { name_line, name_column, .. } => Span::new(*name_line, *name_column),
        Statement::Return(Some(value)) => expression_location(value),
        Statement::If { condition, .. } | Statement::While { condition, .. } => {
            expression_location(condition)
        }
        Statement::For { iterable, .. } | Statement::Find { iterable, .. } => {
            expression_location(iterable)
        }
        Statement::Match { value, .. } => expression_location(value),
        Statement::Unsafe { line, column, .. } => Span::new(*line, *column),
        _ => Span::unknown(),
    }
}

pub fn expression_location(expression: &Expression) -> Span {
    match expression {
        Expression::Identifier(ident) => Span::new(ident.line, ident.column),
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
        Expression::Call {
            name_line,
            name_column,
            args,
            ..
        } => {
            if *name_line > 0 {
                Span::new(*name_line, *name_column)
            } else {
                args.first()
                    .map(expression_location)
                    .unwrap_or(Span::unknown())
            }
        }
        | Expression::List { elements: args, .. }
        | Expression::Tuple { elements: args, .. } => {
            args.first()
                .map(expression_location)
                .unwrap_or(Span::unknown())
        }
        Expression::Dict { entries, .. } => entries
            .first()
            .map(|(key, _)| expression_location(key))
            .unwrap_or(Span::unknown()),
        Expression::Index { target, .. } | Expression::MemberAccess { target, .. } => {
            expression_location(target)
        }
        Expression::Closure { body, .. } => body
            .first()
            .map(statement_location)
            .unwrap_or(Span::unknown()),
        Expression::Match { value, .. } => expression_location(value),
        Expression::EnumVariant { payloads, .. } => payloads
            .first()
            .map(expression_location)
            .unwrap_or(Span::unknown()),
        Expression::Ascription { expr, .. } => expression_location(expr),
        Expression::UseMacro { inner } => expression_location(inner),
        Expression::Literal(_) | Expression::EnumVariantPath { .. } => Span::unknown(),
    }
}
