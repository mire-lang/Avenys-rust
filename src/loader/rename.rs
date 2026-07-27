use super::*;

pub(crate) fn prefix_loaded_statements_scoped(
    statements: Vec<ExpandedStatement>,
    module_name: &str,
    module_path: &Path,
) -> Vec<ExpandedStatement> {
    let mut symbols_by_prefix: HashMap<String, HashSet<String>> = HashMap::new();
    for statement in &statements {
        let prefix = statement_prefix(module_name, module_path, &statement.origin);
        if let Some(name) = statement_export_name(&statement.statement) {
            symbols_by_prefix
                .entry(prefix)
                .or_default()
                .insert(name.to_string());
        }
    }

    statements
        .into_iter()
        .map(|mut statement| {
            let prefix = statement_prefix(module_name, module_path, &statement.origin);
            if prefix.is_empty() {
                return statement;
            }
            let module_symbols = symbols_by_prefix.get(&prefix).cloned().unwrap_or_default();
            let renamer = ModuleRenamer {
                prefix: &prefix,
                module_symbols: &module_symbols,
            };
            statement.statement = renamer.rename_statement(statement.statement, true);
            statement
        })
        .collect()
}

fn statement_prefix(module_name: &str, module_path: &Path, origin: &Path) -> String {
    if origin == module_path {
        let file_stem = module_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if file_stem.starts_with('_') {
            return String::new();
        }
        return module_name.to_string();
    }

    let base = module_path.parent().unwrap_or(module_path);
    let Ok(relative) = origin.strip_prefix(base) else {
        return String::new();
    };

    let mut parts = Vec::new();
    for component in relative.components() {
        let part = component.as_os_str().to_string_lossy().to_string();
        if !part.is_empty() {
            parts.push(part);
        }
    }

    if parts.is_empty() {
        return module_name.to_string();
    }

    let file_name = parts.pop().unwrap();
    let file_stem = Path::new(&file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(&file_name)
        .to_string();

    if file_stem.starts_with('_') {
        return String::new();
    }

    if file_stem == "mod" {
        if !parts.is_empty() && (parts[0] == "core" || parts[0] == "ext") {
            parts.remove(0);
        }
        if parts.is_empty() {
            module_name.to_string()
        } else {
            parts.join(".")
        }
    } else {
        if !parts.is_empty() && (parts[0] == "core" || parts[0] == "ext") {
            parts.remove(0);
        }
        parts.push(file_stem);
        if parts.is_empty() {
            module_name.to_string()
        } else {
            parts.join(".")
        }
    }
}

pub(crate) struct ModuleRenamer<'a> {
    pub(super) prefix: &'a str,
    pub(super) module_symbols: &'a HashSet<String>,
}

impl<'a> ModuleRenamer<'a> {
    pub(crate) fn rename_statement(&self, statement: Statement, top_level: bool) -> Statement {
        let mut scope_stack = vec![HashSet::new()];
        self.rename_statement_with_scope(statement, &mut scope_stack, top_level)
    }

    #[allow(clippy::ptr_arg)]
    fn rename_statement_with_scope(
        &self,
        statement: Statement,
        scope_stack: &mut Vec<HashSet<String>>,
        top_level: bool,
    ) -> Statement {
        match statement {
            Statement::Let {
                name,
                data_type,
                value,
                is_constant,
                is_mutable,
                is_static,
                visibility,
                name_line,
                name_column,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let data_type = self.rename_data_type(data_type, scope_stack);
                let value = value.map(|expr| self.rename_expression(expr, scope_stack));
                Statement::Let {
                    name,
                    data_type,
                    value,
                    is_constant,
                    is_mutable,
                    is_static,
                    visibility,
                    name_line,
                    name_column,
                }
            }
            Statement::Assignment {
                target,
                value,
                is_mutable,
                ..
            } => Statement::Assignment {
                target: self.rename_assignment_target(target, scope_stack),
                value: self.rename_expression(value, scope_stack),
                is_mutable,
                line: 0,
                column: 0,
            },
            Statement::Function {
                name,
                type_params,
                type_param_bounds,
                params,
                body,
                return_type,
                visibility,
                is_method,
                attributes,
                name_line,
                name_column,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.extend(type_params.iter().cloned());
                    scope.extend(params.iter().map(|(name, _)| name.clone()));
                }
                let params = params
                    .into_iter()
                    .map(|(param_name, param_type)| {
                        (param_name, self.rename_data_type(param_type, scope_stack))
                    })
                    .collect();
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let return_type = self.rename_data_type(return_type, scope_stack);
                let body = self.rename_statement_block(body, &mut body_scope);
                Statement::Function {
                    attributes,
                    name,
                    type_params,
                    type_param_bounds,
                    params,
                    body,
                    return_type,
                    visibility,
                    is_method,
                    name_line,
                    name_column,
                }
            }
            Statement::Return(expr) => {
                Statement::Return(expr.map(|expr| self.rename_expression(expr, scope_stack)))
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => Statement::If {
                condition: self.rename_expression(condition, scope_stack),
                then_branch: self.rename_statement_block(then_branch, &mut scope_stack.clone()),
                else_branch: else_branch
                    .map(|branch| self.rename_statement_block(branch, &mut scope_stack.clone())),
            },
            Statement::While { condition, body } => Statement::While {
                condition: self.rename_expression(condition, scope_stack),
                body: self.rename_statement_block(body, &mut scope_stack.clone()),
            },
            Statement::For {
                variable,
                index,
                iterable,
                body,
            } => {
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.insert(variable.clone());
                    if let Some(index) = &index {
                        scope.insert(index.clone());
                    }
                }
                Statement::For {
                    variable,
                    index,
                    iterable: self.rename_expression(iterable, scope_stack),
                    body: self.rename_statement_block(body, &mut body_scope),
                }
            }
            Statement::Expression(expr) => {
                Statement::Expression(self.rename_expression(expr, scope_stack))
            }
            Statement::Break => Statement::Break,
            Statement::Continue => Statement::Continue,
            Statement::Find {
                variable,
                iterable,
                body,
            } => {
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.insert(variable.clone());
                }
                Statement::Find {
                    variable,
                    iterable: self.rename_expression(iterable, scope_stack),
                    body: self.rename_statement_block(body, &mut body_scope),
                }
            }
            Statement::Match {
                value,
                cases,
                default,
            } => {
                let value = self.rename_expression(value, scope_stack);
                let cases = cases
                    .into_iter()
                    .map(|(pattern, body)| {
                        let pattern = self.rename_match_pattern(pattern, scope_stack);
                        let mut case_scope = scope_stack.clone();
                        if let Some(scope) = case_scope.last_mut() {
                            scope.extend(match_pattern_bindings(&pattern));
                        }
                        (pattern, self.rename_statement_block(body, &mut case_scope))
                    })
                    .collect();
                let default = self.rename_statement_block(default, &mut scope_stack.clone());
                Statement::Match {
                    value,
                    cases,
                    default,
                }
            }
            Statement::Type {
                visibility,
                name,
                type_params,
                type_param_bounds,
                parent,
                fields,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let mut fields_scope = scope_stack.clone();
                if let Some(scope) = fields_scope.last_mut() {
                    scope.extend(type_params.iter().cloned());
                }
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let parent = parent.map(|parent| self.rename_type_name(parent, scope_stack));
                let fields = self.rename_statement_block(fields, &mut fields_scope);
                Statement::Type {
                    visibility,
                    name,
                    type_params,
                    type_param_bounds,
                    parent,
                    fields,
                }
            }
            Statement::Skill { name, visibility, methods } => Statement::Skill {
                name: self.rename_decl_name(name, scope_stack, top_level),
                visibility,
                methods: methods
                    .into_iter()
                    .map(|mut method| {
                        method.params = method
                            .params
                            .into_iter()
                            .map(|(param_name, param_type)| {
                                (param_name, self.rename_data_type(param_type, scope_stack))
                            })
                            .collect();
                        method.return_type = self.rename_data_type(method.return_type, scope_stack);
                        method
                    })
                    .collect(),
            },
            Statement::Impl {
                trait_name,
                type_name,
                type_params,
                type_param_bounds,
                methods,
            } => {
                let mut body_scope = scope_stack.clone();
                if let Some(scope) = body_scope.last_mut() {
                    scope.extend(type_params.iter().cloned());
                }
                let trait_name = trait_name.map(|name| self.rename_type_name(name, scope_stack));
                let type_name = self.rename_type_name(type_name, scope_stack);
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let methods = self.rename_statement_block(methods, &mut body_scope);
                Statement::Impl {
                    trait_name,
                    type_name,
                    type_params,
                    type_param_bounds,
                    methods,
                }
            }
            Statement::ExternLib { name, path, line, column } => Statement::ExternLib {
                name: self.rename_decl_name(name, scope_stack, top_level),
                path,
                line,
                column,
            },
            Statement::ExternFunction {
                name,
                lib_name,
                params,
                return_type,
                visibility,
                line,
                column,
            } => Statement::ExternFunction {
                name: self.rename_extern_name(name, scope_stack, top_level, &lib_name),
                lib_name,
                params: params
                    .into_iter()
                    .map(|(param_name, param_type)| {
                        (param_name, self.rename_data_type(param_type, scope_stack))
                    })
                    .collect(),
                return_type: self.rename_data_type(return_type, scope_stack),
                visibility,
                line,
                column,
            },
            Statement::Unsafe {
                line, column, body, ..
            } => Statement::Unsafe {
                line,
                column,
                body: self.rename_statement_block(body, &mut scope_stack.clone()),
            },
            Statement::Asm { instructions } => Statement::Asm {
                instructions: instructions
                    .into_iter()
                    .map(|(name, expr)| (name, self.rename_expression(expr, scope_stack)))
                    .collect(),
            },
            Statement::Load { path, alias, items, line, column } => Statement::Load { path, alias, items, line, column },
            Statement::LoadLocal { .. } => statement,
            Statement::Module { name } => Statement::Module {
                name: self.rename_decl_name(name, scope_stack, top_level),
            },
            Statement::Drop { value } => Statement::Drop {
                value: self.rename_expression(value, scope_stack),
            },
            Statement::New {
                value,
                declared_type,
            } => Statement::New {
                value: value.map(|expr| self.rename_expression(expr, scope_stack)),
                declared_type: self.rename_data_type(declared_type, scope_stack),
            },
            Statement::Own { value, inner_type } => Statement::Own {
                value: value.map(|expr| self.rename_expression(expr, scope_stack)),
                inner_type: self.rename_data_type(inner_type, scope_stack),
            },
            Statement::Move { target, value } => Statement::Move {
                target: self.rename_decl_name(target, scope_stack, top_level),
                value: self.rename_expression(value, scope_stack),
            },
            Statement::Enum {
                visibility,
                name,
                type_params,
                type_param_bounds,
                variants,
            } => {
                let name = self.rename_decl_name(name, scope_stack, top_level);
                let type_param_bounds = type_param_bounds
                    .into_iter()
                    .map(|(bound, traits)| {
                        (
                            bound,
                            traits
                                .into_iter()
                                .map(|trait_name| self.rename_type_name(trait_name, scope_stack))
                                .collect(),
                        )
                    })
                    .collect();
                let variants = variants
                    .into_iter()
                    .map(|variant| self.rename_enum_variant(variant, &name, scope_stack))
                    .collect();
                Statement::Enum {
                    visibility,
                    name,
                    type_params,
                    type_param_bounds,
                    variants,
                }
            }
            Statement::Query {
                table,
                bindings,
                ops,
                joins,
                group_by,
            } => Statement::Query {
                table,
                bindings,
                ops: ops
                    .into_iter()
                    .map(|op| self.rename_query_op(op, scope_stack))
                    .collect(),
                joins,
                group_by,
            },
        }
    }

    pub(super) fn rename_statement_block(
        &self,
        statements: Vec<Statement>,
        scope_stack: &mut Vec<HashSet<String>>,
    ) -> Vec<Statement> {
        let mut renamed = Vec::with_capacity(statements.len());
        for statement in statements {
            let renamed_statement = self.rename_statement_with_scope(statement, scope_stack, false);
            let bindings = statement_bindings(&renamed_statement);
            if let Some(scope) = scope_stack.last_mut() {
                scope.extend(bindings);
            }
            renamed.push(renamed_statement);
        }
        renamed
    }

    fn rename_decl_name(
        &self,
        name: String,
        scope_stack: &[HashSet<String>],
        top_level: bool,
    ) -> String {
        if top_level && self.should_prefix(&name, scope_stack) {
            format!("{}.{}", self.prefix, name)
        } else {
            name
        }
    }

    fn rename_extern_name(
        &self,
        name: String,
        scope_stack: &[HashSet<String>],
        top_level: bool,
        lib_name: &str,
    ) -> String {
        if lib_name == "c" {
            name
        } else if top_level && self.should_prefix(&name, scope_stack) {
            format!("{}.{}", self.prefix, name)
        } else {
            name
        }
    }

    pub(super) fn rename_type_name(&self, name: String, scope_stack: &[HashSet<String>]) -> String {
        if self.should_prefix(&name, scope_stack) {
            format!("{}.{}", self.prefix, name)
        } else {
            name
        }
    }

    fn should_prefix(&self, name: &str, scope_stack: &[HashSet<String>]) -> bool {
        self.module_symbols.contains(name) && !is_shadowed(scope_stack, name) && !name.contains('.')
    }

    pub(super) fn rename_data_type(&self, data_type: DataType, scope_stack: &[HashSet<String>]) -> DataType {
        match data_type {
            DataType::StructNamed(name) => {
                DataType::StructNamed(self.rename_type_name(name, scope_stack))
            }
            DataType::EnumNamed(name) => {
                DataType::EnumNamed(self.rename_type_name(name, scope_stack))
            }
            DataType::DynTrait { trait_name } => DataType::DynTrait {
                trait_name: self.rename_type_name(trait_name, scope_stack),
            },
            DataType::Vector {
                element_type,
                dynamic,
            } => DataType::Vector {
                element_type: Box::new(self.rename_data_type(*element_type, scope_stack)),
                dynamic,
            },
            DataType::Slice { element_type } => DataType::Slice {
                element_type: Box::new(self.rename_data_type(*element_type, scope_stack)),
            },
            DataType::Result { ok, err } => DataType::Result {
                ok: Box::new(self.rename_data_type(*ok, scope_stack)),
                err: Box::new(self.rename_data_type(*err, scope_stack)),
            },
            DataType::Maybe { inner } => DataType::Maybe {
                inner: Box::new(self.rename_data_type(*inner, scope_stack)),
            },
            DataType::Map {
                key_type,
                value_type,
            } => DataType::Map {
                key_type: Box::new(self.rename_data_type(*key_type, scope_stack)),
                value_type: Box::new(self.rename_data_type(*value_type, scope_stack)),
            },
            DataType::Array { element_type, size } => DataType::Array {
                element_type: Box::new(self.rename_data_type(*element_type, scope_stack)),
                size,
            },
            DataType::Ref { inner } => DataType::Ref {
                inner: Box::new(self.rename_data_type(*inner, scope_stack)),
            },
            DataType::RefMut { inner } => DataType::RefMut {
                inner: Box::new(self.rename_data_type(*inner, scope_stack)),
            },
            other => other,
        }
    }

    fn rename_assignment_target(
        &self,
        target: AssignmentTarget,
        scope_stack: &[HashSet<String>],
    ) -> AssignmentTarget {
        match target {
            AssignmentTarget::Variable(name) => {
                AssignmentTarget::Variable(self.rename_type_name(name, scope_stack))
            }
            AssignmentTarget::Field(path) => {
                let mut parts = path.split('.').map(ToString::to_string).collect::<Vec<_>>();
                if let Some(root) = parts.first_mut() {
                    *root = self.rename_type_name(root.clone(), scope_stack);
                }
                AssignmentTarget::Field(parts.join("."))
            }
            AssignmentTarget::Index { target, index } => AssignmentTarget::Index {
                target: Box::new(self.rename_expression(*target, scope_stack)),
                index: Box::new(self.rename_expression(*index, scope_stack)),
            },
        }
    }

    pub(super) fn rename_match_pattern(
        &self,
        pattern: Expression,
        scope_stack: &[HashSet<String>],
    ) -> Expression {
        match pattern {
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
                    .map(|payload| match payload {
                        Expression::Identifier(_) => payload,
                        other => self.rename_expression(other, scope_stack),
                    })
                    .collect(),
                data_type,
                line: 0,
                column: 0,
            },
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                data_type,
                ..
            } => Expression::EnumVariantPath {
                enum_name: self.rename_type_name(enum_name, scope_stack),
                variant_name,
                data_type,
                line: 0,
                column: 0,
            },
            Expression::Call {
                name,
                args,
                type_args,
                name_line,
                name_column,
                data_type,
            } if name == "__match_guard" || name == "__match_or" => Expression::Call {
                name,
                args: args
                    .into_iter()
                    .map(|arg| self.rename_match_pattern(arg, scope_stack))
                    .collect(),
                type_args: type_args
                    .into_iter()
                    .map(|data_type| self.rename_data_type(data_type, scope_stack))
                    .collect(),
                name_line,
                name_column,
                data_type,
            },
            other => self.rename_expression(other, scope_stack),
        }
    }

}

fn is_shadowed(scope_stack: &[HashSet<String>], name: &str) -> bool {
    scope_stack.iter().rev().any(|scope| scope.contains(name))
}

pub(super) fn match_pattern_bindings(pattern: &Expression) -> Vec<String> {
    let mut bindings = Vec::new();
    match pattern {
        Expression::EnumVariant { payloads, .. } => {
            for payload in payloads {
                if let Expression::Identifier(Identifier { name, .. }) = payload {
                    bindings.push(name.clone());
                }
            }
        }
        Expression::Call { name, args, .. } if name == "__match_guard" || name == "__match_or" => {
            if let Some(inner) = args.first() {
                bindings.extend(match_pattern_bindings(inner));
            }
        }
        _ => {}
    }
    bindings
}

fn statement_bindings(statement: &Statement) -> Vec<String> {
    let mut bindings = Vec::new();
    match statement {
        Statement::Let { name, .. }
        | Statement::Function { name, .. }
        | Statement::Type { name, .. }
        | Statement::Skill { name, .. }
        | Statement::Module { name, .. }
        | Statement::Enum { name, .. }
        | Statement::ExternLib { name, .. }
        | Statement::ExternFunction { name, .. } => bindings.push(name.clone()),
        Statement::For {
            variable, index, ..
        } => {
            bindings.push(variable.clone());
            if let Some(index) = index {
                bindings.push(index.clone());
            }
        }
        Statement::Find { variable, .. }
        | Statement::Move {
            target: variable, ..
        } => bindings.push(variable.clone()),
        _ => {}
    }
    bindings
}
