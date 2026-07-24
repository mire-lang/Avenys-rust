use super::*;

pub(super) fn hash_expression(expr: &Expression, hasher: &mut FxHasher) {
    match expr {
        Expression::Ascription { expr: inner, .. } => {
            hasher.write_u8(99);
            hash_expression(inner, hasher);
        }
        Expression::Literal(lit) => {
            hasher.write_u8(0);
            hash_literal(lit, hasher);
        }
        Expression::Identifier(ident) => {
            hasher.write_u8(1);
            hash_identifier(ident, hasher);
        }
        Expression::BinaryOp {
            operator,
            left,
            right,
            data_type,
        } => {
            hasher.write_u8(2);
            operator.hash(hasher);
            hash_expression(left, hasher);
            hash_expression(right, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::UnaryOp {
            operator,
            operand,
            data_type,
        } => {
            hasher.write_u8(3);
            operator.hash(hasher);
            hash_expression(operand, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::NamedArg {
            name,
            value,
            data_type,
        } => {
            hasher.write_u8(4);
            name.hash(hasher);
            hash_expression(value, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Call {
            name,
            args,
            type_args,
            name_line: _,
            name_column: _,
            data_type,
        } => {
            hasher.write_u8(5);
            name.hash(hasher);
            hash_expressions(args, hasher);
            for arg in type_args {
                hash_data_type(arg, hasher);
            }
            hash_data_type(data_type, hasher);
        }
        Expression::List {
            elements,
            element_type,
            data_type,
        } => {
            hasher.write_u8(6);
            hash_expressions(elements, hasher);
            hash_data_type(element_type, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Dict {
            entries,
            key_type,
            value_type,
            data_type,
        } => {
            hasher.write_u8(7);
            entries.len().hash(hasher);
            for (k, v) in entries {
                hash_expression(k, hasher);
                hash_expression(v, hasher);
            }
            hash_data_type(key_type, hasher);
            hash_data_type(value_type, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Tuple {
            elements,
            data_type,
        } => {
            hasher.write_u8(8);
            hash_expressions(elements, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Index {
            target,
            index,
            data_type,
        } => {
            hasher.write_u8(9);
            hash_expression(target, hasher);
            hash_expression(index, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::MemberAccess {
            target,
            member,
            data_type,
        } => {
            hasher.write_u8(10);
            hash_expression(target, hasher);
            member.hash(hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Closure {
            params,
            body,
            return_type,
            capture,
        } => {
            hasher.write_u8(11);
            hash_params(params, hasher);
            hash_statements(body, hasher);
            hash_data_type(return_type, hasher);
            capture.len().hash(hasher);
            for (name, data_type) in capture {
                name.hash(hasher);
                hash_data_type(data_type, hasher);
            }
        }
        Expression::Reference {
            expr,
            is_mutable,
            data_type,
            referenced_type,
        } => {
            hasher.write_u8(12);
            hash_expression(expr, hasher);
            is_mutable.hash(hasher);
            hash_data_type(data_type, hasher);
            hash_data_type(referenced_type, hasher);
        }
        Expression::Dereference { expr, data_type } => {
            hasher.write_u8(13);
            hash_expression(expr, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Box { value, data_type } => {
            hasher.write_u8(14);
            hash_expression(value, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Pipeline {
            input,
            stage,
            safe,
            data_type,
        } => {
            hasher.write_u8(15);
            hash_expression(input, hasher);
            hash_expression(stage, hasher);
            safe.hash(hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Match {
            value,
            cases,
            default,
            data_type,
        } => {
            hasher.write_u8(16);
            hash_expression(value, hasher);
            cases.len().hash(hasher);
            for (a, b) in cases {
                hash_expression(a, hasher);
                hash_expression(b, hasher);
            }
            hash_expression(default, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::EnumVariantPath {
            enum_name,
            variant_name,
            data_type,
        } => {
            hasher.write_u8(17);
            enum_name.hash(hasher);
            variant_name.hash(hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::EnumVariant {
            enum_name,
            variant_name,
            payloads,
            data_type,
        } => {
            hasher.write_u8(18);
            enum_name.hash(hasher);
            variant_name.hash(hasher);
            hash_expressions(payloads, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Try { expr, data_type } => {
            hasher.write_u8(19);
            hash_expression(expr, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Ok { value, data_type } => {
            hasher.write_u8(20);
            hash_expression(value, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Err { value, data_type } => {
            hasher.write_u8(21);
            hash_expression(value, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::Some { value, data_type } => {
            hasher.write_u8(42);
            hash_expression(value, hasher);
            hash_data_type(data_type, hasher);
        }
        Expression::UseMacro { inner } => {
            hasher.write_u8(22);
            hash_expression(inner, hasher);
        }
    }
}

pub(super) fn hash_expressions(expressions: &[Expression], hasher: &mut FxHasher) {
    expressions.len().hash(hasher);
    for expr in expressions {
        hash_expression(expr, hasher);
    }
}

pub(super) fn hash_option_expr(expr: &Option<Expression>, hasher: &mut FxHasher) {
    match expr {
        Some(e) => {
            hasher.write_u8(1);
            hash_expression(e, hasher);
        }
        None => hasher.write_u8(0),
    }
}

pub(super) fn hash_query_bindings(bindings: &[QueryBinding], hasher: &mut FxHasher) {
    bindings.len().hash(hasher);
    for binding in bindings {
        binding.target.hash(hasher);
        binding.alias.hash(hasher);
        binding.column.hash(hasher);
    }
}

pub(super) fn hash_query_ops(ops: &[QueryOp], hasher: &mut FxHasher) {
    ops.len().hash(hasher);
    for op in ops {
        match op {
            QueryOp::Insert { assigns } => {
                hasher.write_u8(0);
                assigns.len().hash(hasher);
                for (name, expr) in assigns {
                    name.hash(hasher);
                    hash_expression(expr, hasher);
                }
            }
            QueryOp::Update { condition, assigns } => {
                hasher.write_u8(1);
                hash_expression(condition, hasher);
                assigns.len().hash(hasher);
                for (name, expr) in assigns {
                    name.hash(hasher);
                    hash_expression(expr, hasher);
                }
            }
            QueryOp::Delete { condition } => {
                hasher.write_u8(2);
                hash_expression(condition, hasher);
            }
            QueryOp::Get(get) => {
                hasher.write_u8(3);
                get.target.hash(hasher);
                hash_expression(&get.condition, hasher);
                hash_statements(&get.body, hasher);
            }
            QueryOp::Export { path } => {
                hasher.write_u8(4);
                path.hash(hasher);
            }
            QueryOp::Import { path } => {
                hasher.write_u8(5);
                path.hash(hasher);
            }
        }
    }
}

pub(super) fn hash_query_joins(joins: &[QueryJoin], hasher: &mut FxHasher) {
    joins.len().hash(hasher);
    for join in joins {
        join.right_table.hash(hasher);
        join.left_column.hash(hasher);
        join.right_column.hash(hasher);
    }
}

pub(super) fn hash_query_group(group: Option<&QueryGroup>, hasher: &mut FxHasher) {
    match group {
        Some(g) => {
            hasher.write_u8(1);
            g.column.hash(hasher);
        }
        None => hasher.write_u8(0),
    }
}
