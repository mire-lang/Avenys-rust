use crate::compiler::location;
use crate::error::Result;
use crate::parser::ast::Expression;

use super::{BindingState, BorrowChecker};
impl BorrowChecker<'_> {
    pub(super) fn check_expression(&mut self, expression: &Expression) -> Result<()> {
        let loc = Self::expression_location(expression);
        self.current_span = loc;
        match expression {
            Expression::Ascription { expr, .. } => {
                self.check_expression(expr)?;
            }
            Expression::Literal(_) => {}
            Expression::Identifier(ident) => {
                self.ensure_binding_available(&ident.name)?;
            }
            Expression::BinaryOp { left, right, .. } => {
                self.check_expression(left)?;
                self.check_expression(right)?;
            }
            Expression::UnaryOp { operand, .. } => {
                self.check_expression(operand)?;
            }
            Expression::NamedArg { value, .. } => {
                self.check_expression(value)?;
            }
            Expression::Call { name, args, .. } => {
                if name == "move::" {
                    if let Some(first) = args.first() {
                        self.check_expression(first)?;
                        if let Some(source) = Self::identifier_name(first) {
                            self.ensure_can_move(&source)?;
                            if let Some(state) = self.lookup_binding_mut(&source) {
                                state.is_moved = true;
                            }
                        }
                    }
                    return Ok(());
                }

                if name == "drop::" {
                    for arg in args {
                        self.check_expression(arg)?;
                        if let Some(source) = Self::identifier_name(arg) {
                            self.ensure_can_drop(&source)?;
                            if let Some(state) = self.lookup_binding_mut(&source) {
                                state.is_moved = true;
                            }
                        }
                    }
                    return Ok(());
                }

                for (index, arg) in args.iter().enumerate() {
                    self.check_expression(arg)?;
                    self.check_call_argument(name, index, arg)?;
                }
            }
            Expression::List { elements: args, .. } | Expression::Tuple { elements: args, .. } => {
                for arg in args {
                    self.check_expression(arg)?;
                }
            }
            Expression::Dict { entries, .. } => {
                for (key, value) in entries {
                    self.check_expression(key)?;
                    self.check_expression(value)?;
                }
            }
            Expression::Index { target, index, .. } => {
                self.check_expression(target)?;
                self.check_expression(index)?;
            }
            Expression::MemberAccess { target, .. } => {
                self.check_expression(target)?;
            }
            Expression::Closure { body, .. } => {
                if let Expression::Closure {
                    params, capture, ..
                } = expression
                {
                    for (name, _) in capture {
                        self.register_borrow(name, false)?;
                    }
                    self.push_scope();
                    for (name, _) in capture {
                        self.insert_binding(name.clone(), BindingState::default());
                    }
                    for (name, _) in params {
                        self.insert_binding(name.clone(), BindingState::default());
                    }
                } else {
                    self.push_scope();
                }
                self.check_statements(body)?;
                self.pop_scope();
            }
            Expression::Reference {
                expr, is_mutable, ..
            } => {
                self.check_expression(expr)?;
                if let Some(name) = Self::identifier_name(expr) {
                    self.ensure_borrow_allowed(&name, *is_mutable)?;
                }
            }
            Expression::Dereference { expr, .. } | Expression::Box { value: expr, .. } => {
                self.check_expression(expr)?;
            }
            Expression::Try { expr, .. } => {
                self.check_expression(expr)?;
                if let Some(name) = Self::identifier_name(expr) {
                    self.mark_moved_if_non_copy(&name);
                }
            }
            Expression::Ok { value, .. } | Expression::Err { value, .. } | Expression::Some { value, .. } => {
                self.check_expression(value)?;
                if let Some(name) = Self::identifier_name(value) {
                    self.mark_moved_if_non_copy(&name);
                }
            }
            Expression::Pipeline { input, stage, .. } => {
                self.check_expression(input)?;
                self.check_expression(stage)?;
            }
            Expression::Match {
                value,
                cases,
                default,
                ..
            } => {
                self.check_expression(value)?;
                let scopes_before = self.scopes.clone();
                let mut branch_scopes = Vec::new();
                for (pattern, result) in cases {
                    self.scopes = scopes_before.clone();
                    self.push_scope();
                    self.insert_match_pattern_bindings(pattern);
                    self.check_expression(result)?;
                    self.pop_scope();
                    branch_scopes.push(self.scopes.clone());
                }
                self.scopes = scopes_before.clone();
                self.push_scope();
                self.check_expression(default)?;
                self.pop_scope();
                branch_scopes.push(self.scopes.clone());
                self.scopes = Self::merge_multiple_scopes(&scopes_before, &branch_scopes);
            }
            Expression::EnumVariantPath { .. } => {}
            Expression::EnumVariant { payloads, .. } => {
                for payload in payloads {
                    self.check_expression(payload)?;
                }
            }
            Expression::UseMacro { inner } => {
                self.check_expression(inner)?;
            }
        }
        Ok(())
    }

    pub(super) fn expression_location(expression: &Expression) -> crate::error::Span {
        location::expression_location(expression)
    }
}
