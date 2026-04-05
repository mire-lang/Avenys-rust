use std::collections::HashMap;

use crate::parser::ast::{DataType, Expression, Program, Statement};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionInfo {
    pub params: Vec<DataType>,
    pub return_type: DataType,
    pub is_method: bool,
    pub scope_id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingKind {
    Value,
    SharedRef,
    MutableRef,
    Boxed,
    Parameter,
    QueryBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingInfo {
    pub name: String,
    pub data_type: DataType,
    pub scope_id: usize,
    pub scope_depth: usize,
    pub kind: BindingKind,
    pub reference_target: Option<String>,
    pub declared_in_unsafe: bool,
    pub is_constant: bool,
    pub is_static: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeInfo {
    pub id: usize,
    pub parent_id: Option<usize>,
    pub depth: usize,
    pub is_unsafe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorrowKind {
    Shared,
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorrowFact {
    pub owner: String,
    pub borrower: String,
    pub kind: BorrowKind,
    pub scope_id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveFact {
    pub target: String,
    pub source: Option<String>,
    pub scope_id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticModel {
    pub functions: HashMap<String, FunctionInfo>,
    pub bindings: Vec<BindingInfo>,
    pub bindings_by_name: HashMap<String, Vec<usize>>,
    pub scopes: Vec<ScopeInfo>,
    pub borrow_facts: Vec<BorrowFact>,
    pub move_facts: Vec<MoveFact>,
    pub drop_facts: Vec<(String, usize)>,
    pub unsafe_blocks: usize,
    pub move_statements: usize,
    pub drop_statements: usize,
}

pub fn analyze_program(program: &Program) -> SemanticModel {
    let mut builder = SemanticModelBuilder::new();
    builder.visit_statements(&program.statements);
    builder.model
}

struct SemanticModelBuilder {
    model: SemanticModel,
    scope_depth: usize,
    scope_stack: Vec<usize>,
    unsafe_depth: usize,
    next_scope_id: usize,
}

impl SemanticModelBuilder {
    fn new() -> Self {
        Self {
            model: SemanticModel {
                scopes: vec![ScopeInfo {
                    id: 0,
                    parent_id: None,
                    depth: 0,
                    is_unsafe: false,
                }],
                ..SemanticModel::default()
            },
            scope_depth: 0,
            scope_stack: vec![0],
            unsafe_depth: 0,
            next_scope_id: 1,
        }
    }
}

impl SemanticModelBuilder {
    fn visit_statements(&mut self, statements: &[Statement]) {
        for statement in statements {
            self.visit_statement(statement);
        }
    }

    fn visit_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Let {
                name,
                data_type,
                value,
                is_constant,
                is_static,
                ..
            } => {
                self.register_binding(
                    name.clone(),
                    data_type.clone(),
                    Self::binding_kind(data_type, value.as_ref(), false),
                    Self::reference_target(value.as_ref()),
                    *is_constant,
                    *is_static,
                );
                if let Some(expr) = value {
                    self.visit_expression(expr);
                }
            }
            Statement::Assignment { target, value, .. } => {
                if let Some((owner, kind)) = Self::reference_details(value) {
                    self.model.borrow_facts.push(BorrowFact {
                        owner,
                        borrower: target.clone(),
                        kind,
                        scope_id: self.current_scope_id(),
                    });
                }
                self.visit_expression(value)
            }
            Statement::Function {
                name,
                params,
                body,
                return_type,
                is_method,
                ..
            } => {
                self.model.functions.insert(
                    name.clone(),
                    FunctionInfo {
                        params: params.iter().map(|(_, ty)| ty.clone()).collect(),
                        return_type: return_type.clone(),
                        is_method: *is_method,
                        scope_id: self.next_scope_id,
                    },
                );
                self.with_scope(|builder| {
                    for (param_name, param_type) in params {
                        builder.register_binding(
                            param_name.clone(),
                            param_type.clone(),
                            Self::binding_kind(param_type, None, true),
                            None,
                            false,
                            false,
                        );
                    }
                    builder.visit_statements(body);
                });
            }
            Statement::Return(expr) => {
                if let Some(expr) = expr {
                    self.visit_expression(expr);
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_expression(condition);
                self.with_scope(|builder| builder.visit_statements(then_branch));
                if let Some(else_branch) = else_branch {
                    self.with_scope(|builder| builder.visit_statements(else_branch));
                }
            }
            Statement::While { condition, body } => {
                self.visit_expression(condition);
                self.with_scope(|builder| builder.visit_statements(body));
            }
            Statement::For {
                variable,
                iterable,
                body,
                ..
            }
            | Statement::Find {
                variable,
                iterable,
                body,
                ..
            } => {
                self.visit_expression(iterable);
                self.with_scope(|builder| {
                    builder.register_binding(
                        variable.clone(),
                        DataType::Anything,
                        BindingKind::Value,
                        None,
                        false,
                        false,
                    );
                    builder.visit_statements(body)
                });
            }
            Statement::Expression(expr) => self.visit_expression(expr),
            Statement::Match {
                value,
                cases,
                default,
            } => {
                self.visit_expression(value);
                for (case_expr, body) in cases {
                    self.visit_expression(case_expr);
                    self.with_scope(|builder| builder.visit_statements(body));
                }
                self.with_scope(|builder| builder.visit_statements(default));
            }
            Statement::Class { .. }
            | Statement::Impl { .. }
            | Statement::Type { .. }
            | Statement::Code { .. }
            | Statement::Skill { .. }
            | Statement::Unsafe { .. }
            | Statement::Asm { .. }
            | Statement::Module { .. }
            | Statement::DmireTable { .. }
            | Statement::DmireColumn { .. }
            | Statement::DmireDlist { .. }
            | Statement::Query { .. }
            | Statement::Drop { .. }
            | Statement::Move { .. } => {}
            Statement::Break
            | Statement::Continue
            | Statement::Trait { .. }
            | Statement::ExternLib { .. }
            | Statement::ExternFunction { .. }
            | Statement::AddLib { .. }
            | Statement::Use { .. }
            | Statement::Enum { .. } => {}
        }
    }

    fn visit_expression(&mut self, expression: &Expression) {
        match expression {
            Expression::BinaryOp { left, right, .. } => {
                self.visit_expression(left);
                self.visit_expression(right);
            }
            Expression::UnaryOp { operand, .. } => self.visit_expression(operand),
            Expression::NamedArg { value, .. } => self.visit_expression(value),
            Expression::Call { args, .. }
            | Expression::Tuple { elements: args, .. }
            | Expression::List { elements: args, .. } => {
                for arg in args {
                    self.visit_expression(arg);
                }
            }
            Expression::Dict { entries, .. } => {
                for (key, value) in entries {
                    self.visit_expression(key);
                    self.visit_expression(value);
                }
            }
            Expression::Index { target, index, .. } => {
                self.visit_expression(target);
                self.visit_expression(index);
            }
            Expression::MemberAccess { target, .. }
            | Expression::Dereference { expr: target, .. }
            | Expression::Box { value: target, .. } => {
                self.visit_expression(target);
            }
            Expression::Reference {
                expr: target,
                is_mutable,
                ..
            } => {
                if let Some(owner) = Self::identifier_name(target) {
                    self.model.borrow_facts.push(BorrowFact {
                        owner,
                        borrower: "<expr>".to_string(),
                        kind: if *is_mutable {
                            BorrowKind::Mutable
                        } else {
                            BorrowKind::Shared
                        },
                        scope_id: self.current_scope_id(),
                    });
                }
                self.visit_expression(target);
            }
            Expression::Closure { body, .. } => {
                self.with_scope(|builder| builder.visit_statements(body));
            }
            Expression::Literal(_) | Expression::Identifier(_) => {}
        }
    }

    fn register_binding(
        &mut self,
        name: String,
        data_type: DataType,
        kind: BindingKind,
        reference_target: Option<String>,
        is_constant: bool,
        is_static: bool,
    ) {
        let index = self.model.bindings.len();
        self.model.bindings.push(BindingInfo {
            name: name.clone(),
            data_type,
            scope_id: self.current_scope_id(),
            scope_depth: self.scope_depth,
            kind,
            reference_target,
            declared_in_unsafe: self.unsafe_depth > 0,
            is_constant,
            is_static,
        });
        self.model
            .bindings_by_name
            .entry(name)
            .or_default()
            .push(index);
    }

    fn with_scope<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Self),
    {
        let parent_id = self.scope_stack.last().copied();
        let scope_id = self.next_scope_id;
        self.next_scope_id += 1;
        self.model.scopes.push(ScopeInfo {
            id: scope_id,
            parent_id,
            depth: self.scope_depth + 1,
            is_unsafe: self.unsafe_depth > 0,
        });
        self.scope_depth += 1;
        self.scope_stack.push(scope_id);
        f(self);
        self.scope_stack.pop();
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    fn current_scope_id(&self) -> usize {
        self.scope_stack.last().copied().unwrap_or(0)
    }

    fn binding_kind(
        data_type: &DataType,
        value: Option<&Expression>,
        is_parameter: bool,
    ) -> BindingKind {
        if is_parameter {
            return match data_type {
                DataType::Ref => BindingKind::SharedRef,
                DataType::RefMut => BindingKind::MutableRef,
                DataType::Box => BindingKind::Boxed,
                _ => BindingKind::Parameter,
            };
        }
        match data_type {
            DataType::Ref => BindingKind::SharedRef,
            DataType::RefMut => BindingKind::MutableRef,
            DataType::Box => BindingKind::Boxed,
            _ => match value {
                Some(Expression::Reference { is_mutable, .. }) => {
                    if *is_mutable {
                        BindingKind::MutableRef
                    } else {
                        BindingKind::SharedRef
                    }
                }
                Some(Expression::Box { .. }) => BindingKind::Boxed,
                _ => BindingKind::Value,
            },
        }
    }

    fn reference_target(value: Option<&Expression>) -> Option<String> {
        match value? {
            Expression::Reference { expr, .. } => Self::identifier_name(expr),
            _ => None,
        }
    }

    fn reference_details(value: &Expression) -> Option<(String, BorrowKind)> {
        match value {
            Expression::Reference {
                expr, is_mutable, ..
            } => Self::identifier_name(expr).map(|owner| {
                (
                    owner,
                    if *is_mutable {
                        BorrowKind::Mutable
                    } else {
                        BorrowKind::Shared
                    },
                )
            }),
            _ => None,
        }
    }

    fn identifier_name(expression: &Expression) -> Option<String> {
        match expression {
            Expression::Identifier(ident) => Some(ident.name.clone()),
            _ => None,
        }
    }
}
