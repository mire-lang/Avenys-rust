use super::*;
use super::rename::{ModuleRenamer, match_pattern_bindings};

impl<'a> ModuleRenamer<'a> {

    pub(crate) fn rename_expression(
        &self,
        expression: Expression,
        scope_stack: &[HashSet<String>],
    ) -> Expression {
        match expression {
            Expression::Identifier(Identifier {
                name,
                data_type,
                line,
                column,
            }) => Expression::Identifier(Identifier {
                name: self.rename_type_name(name, scope_stack),
                data_type: self.rename_data_type(data_type, scope_stack),
                line,
                column,
            }),
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
            } => Expression::BinaryOp {
                operator,
                left: Box::new(self.rename_expression(*left, scope_stack)),
                right: Box::new(self.rename_expression(*right, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::UnaryOp {
                operator,
                operand,
                data_type,
            } => Expression::UnaryOp {
                operator,
                operand: Box::new(self.rename_expression(*operand, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::NamedArg {
                name,
                value,
                data_type,
            } => Expression::NamedArg {
                name,
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Call {
                name,
                args,
                type_args,
                name_line,
                name_column,
                data_type,
            } => {
                let name = self.rename_type_name(name, scope_stack);
                Expression::Call {
                    name,
                    args: args
                        .into_iter()
                        .map(|arg| self.rename_expression(arg, scope_stack))
                        .collect(),
                    type_args: type_args
                        .into_iter()
                        .map(|data_type| self.rename_data_type(data_type, scope_stack))
                        .collect(),
                    name_line,
                    name_column,
                    data_type: self.rename_data_type(data_type, scope_stack),
                }
            }
            Expression::List {
                elements,
                element_type,
                data_type,
            } => Expression::List {
                elements: elements
                    .into_iter()
                    .map(|element| self.rename_expression(element, scope_stack))
                    .collect(),
                element_type: self.rename_data_type(element_type, scope_stack),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Dict {
                entries,
                key_type,
                value_type,
                data_type,
            } => Expression::Dict {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            self.rename_expression(key, scope_stack),
                            self.rename_expression(value, scope_stack),
                        )
                    })
                    .collect(),
                key_type: self.rename_data_type(key_type, scope_stack),
                value_type: self.rename_data_type(value_type, scope_stack),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Tuple {
                elements,
                data_type,
            } => Expression::Tuple {
                elements: elements
                    .into_iter()
                    .map(|element| self.rename_expression(element, scope_stack))
                    .collect(),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Index {
                target,
                index,
                data_type,
            } => Expression::Index {
                target: Box::new(self.rename_expression(*target, scope_stack)),
                index: Box::new(self.rename_expression(*index, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::MemberAccess {
                target,
                member,
                data_type,
            } => Expression::MemberAccess {
                target: Box::new(self.rename_expression(*target, scope_stack)),
                member,
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Closure {
                params,
                body,
                return_type,
                capture,
            } => {
                let mut body_scope = scope_stack.to_vec();
                if let Some(scope) = body_scope.last_mut() {
                    scope.extend(params.iter().map(|(name, _)| name.clone()));
                }
                Expression::Closure {
                    params: params
                        .into_iter()
                        .map(|(name, data_type)| {
                            (name, self.rename_data_type(data_type, scope_stack))
                        })
                        .collect(),
                    body: self.rename_statement_block(body, &mut body_scope),
                    return_type: self.rename_data_type(return_type, scope_stack),
                    capture,
                }
            }
            Expression::Reference {
                expr,
                is_mutable,
                data_type,
                referenced_type,
            } => Expression::Reference {
                expr: Box::new(self.rename_expression(*expr, scope_stack)),
                is_mutable,
                data_type: self.rename_data_type(data_type, scope_stack),
                referenced_type: self.rename_data_type(referenced_type, scope_stack),
            },
            Expression::Dereference { expr, data_type } => Expression::Dereference {
                expr: Box::new(self.rename_expression(*expr, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Box { value, data_type } => Expression::Box {
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Pipeline {
                input,
                stage,
                safe,
                data_type,
            } => Expression::Pipeline {
                input: Box::new(self.rename_expression(*input, scope_stack)),
                stage: Box::new(self.rename_expression(*stage, scope_stack)),
                safe,
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Try { expr, data_type } => Expression::Try {
                expr: Box::new(self.rename_expression(*expr, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Ok { value, data_type } => Expression::Ok {
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Err { value, data_type } => Expression::Err {
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Some { value, data_type } => Expression::Some {
                value: Box::new(self.rename_expression(*value, scope_stack)),
                data_type: self.rename_data_type(data_type, scope_stack),
            },
            Expression::Match {
                value,
                cases,
                default,
                data_type,
            } => {
                let value = self.rename_expression(*value, scope_stack);
                let cases = cases
                    .into_iter()
                    .map(|(pattern, body)| {
                        let pattern = self.rename_match_pattern(pattern, scope_stack);
                        let mut case_scope = scope_stack.to_vec();
                        if let Some(scope) = case_scope.last_mut() {
                            scope.extend(match_pattern_bindings(&pattern));
                        }
                        (pattern, self.rename_expression(body, &case_scope))
                    })
                    .collect();
                let default = Box::new(self.rename_expression(*default, scope_stack));
                Expression::Match {
                    value: Box::new(value),
                    cases,
                    default,
                    data_type: self.rename_data_type(data_type, scope_stack),
                }
            }
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
                ..
            } => Expression::EnumVariantPath {
                enum_name: self.rename_type_name(enum_name, scope_stack),
                variant_name,
                data_type: self.rename_data_type(data_type, scope_stack),
                line: 0,
                column: 0,
            },
            Expression::EnumVariant {
                enum_name,
                variant_name,
                payloads,
                data_type,
                ..
            } => Expression::EnumVariant {
                enum_name: self.rename_type_name(enum_name, scope_stack),
                variant_name,
                payloads: payloads
                    .into_iter()
                    .map(|payload| self.rename_expression(payload, scope_stack))
                    .collect(),
                data_type: self.rename_data_type(data_type, scope_stack),
                line: 0,
                column: 0,
            },
            Expression::Ascription { .. } => expression,
            Expression::UseMacro { .. } => expression,
            Expression::Literal { lit: literal, line, column } => Expression::Literal { lit: match literal {
                Literal::List(elements) => Literal::List(
                    elements
                        .into_iter()
                        .map(|element| self.rename_expression(element, scope_stack))
                        .collect(),
                ),
                Literal::Dict(entries) => Literal::Dict(
                    entries
                        .into_iter()
                        .map(|((key, value), data_type)| {
                            (
                                (
                                    self.rename_expression(key, scope_stack),
                                    self.rename_expression(value, scope_stack),
                                ),
                                self.rename_data_type(data_type, scope_stack),
                            )
                        })
                        .collect(),
                ),
                Literal::Tuple(elements) => Literal::Tuple(
                    elements
                        .into_iter()
                        .map(|element| self.rename_expression(element, scope_stack))
                        .collect(),
                ),
                other => other,
            }, line, column },
        }
    }

    pub(super) fn rename_query_op(
        &self,
        op: crate::parser::ast::QueryOp,
        scope_stack: &[HashSet<String>],
    ) -> crate::parser::ast::QueryOp {
        match op {
            crate::parser::ast::QueryOp::Insert { assigns } => {
                crate::parser::ast::QueryOp::Insert {
                    assigns: assigns
                        .into_iter()
                        .map(|(name, expr)| (name, self.rename_expression(expr, scope_stack)))
                        .collect(),
                }
            }
            crate::parser::ast::QueryOp::Update { condition, assigns } => {
                crate::parser::ast::QueryOp::Update {
                    condition: self.rename_expression(condition, scope_stack),
                    assigns: assigns
                        .into_iter()
                        .map(|(name, expr)| (name, self.rename_expression(expr, scope_stack)))
                        .collect(),
                }
            }
            crate::parser::ast::QueryOp::Delete { condition } => {
                crate::parser::ast::QueryOp::Delete {
                    condition: self.rename_expression(condition, scope_stack),
                }
            }
            crate::parser::ast::QueryOp::Get(mut get) => {
                get.condition = self.rename_expression(get.condition, scope_stack);
                get.body = self.rename_statement_block(get.body, &mut scope_stack.to_vec());
                crate::parser::ast::QueryOp::Get(get)
            }
            other => other,
        }
    }

    pub(super) fn rename_enum_variant(
        &self,
        mut variant: EnumVariantDef,
        enum_name: &str,
        scope_stack: &[HashSet<String>],
    ) -> EnumVariantDef {
        variant.enum_name = enum_name.to_string();
        variant.data_types = variant
            .data_types
            .into_iter()
            .map(|data_type| self.rename_data_type(data_type, scope_stack))
            .collect();
        variant
    }
}
