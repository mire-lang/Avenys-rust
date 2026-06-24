use super::*;

impl TypeChecker {
    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.struct_scopes.push(HashMap::new());
        self.ref_scopes.push(HashMap::new());
        self.function_alias_scopes.push(HashMap::new());
        self.function_value_sig_scopes.push(HashMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
        if self.struct_scopes.len() > 1 {
            self.struct_scopes.pop();
        }
        if self.ref_scopes.len() > 1 {
            self.ref_scopes.pop();
        }
        if self.function_alias_scopes.len() > 1 {
            self.function_alias_scopes.pop();
        }
        if self.function_value_sig_scopes.len() > 1 {
            self.function_value_sig_scopes.pop();
        }
    }

    pub(super) fn insert_var(&mut self, name: String, data_type: DataType, is_mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, (data_type, is_mutable));
        }
    }

    pub(super) fn refresh_binding_metadata(
        &mut self,
        name: &str,
        data_type: &DataType,
        value: Option<&Expression>,
    ) {
        self.bind_struct_name(name, data_type, value);
        self.bind_reference_type(name, value);
        self.bind_function_alias(name, data_type, value);
        self.bind_function_value_signature(name, data_type, value);
    }

    fn bind_function_alias(
        &mut self,
        name: &str,
        data_type: &DataType,
        value: Option<&Expression>,
    ) {
        let resolved_alias = if *data_type == DataType::Function {
            value.and_then(|expr| self.function_name_for_expr(expr))
        } else {
            None
        };
        if let Some(scope) = self.function_alias_scopes.last_mut() {
            if let Some(alias) = resolved_alias {
                scope.insert(name.to_string(), alias);
            } else {
                scope.remove(name);
            }
        }
    }

    pub(super) fn function_name_for_expr(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Identifier(ident) => {
                if self.functions.contains_key(&ident.name) {
                    return Some(ident.name.clone());
                }
                let mut stripped = ident.name.clone();
                while let Some(next) = Self::strip_root_namespace(&stripped) {
                    if next == stripped {
                        break;
                    }
                    if self.functions.contains_key(&next) {
                        return Some(next);
                    }
                    stripped = next;
                }
                self.lookup_function_alias(&ident.name)
            }
            _ => None,
        }
    }

    pub(super) fn lookup_function_alias(&self, name: &str) -> Option<String> {
        for scope in self.function_alias_scopes.iter().rev() {
            if let Some(found) = scope.get(name) {
                return Some(found.clone());
            }
        }
        None
    }

    fn bind_function_value_signature(
        &mut self,
        name: &str,
        data_type: &DataType,
        value: Option<&Expression>,
    ) {
        let resolved_sig = if *data_type == DataType::Function {
            value.and_then(|expr| self.function_signature_for_expr(expr))
        } else {
            None
        };
        if let Some(scope) = self.function_value_sig_scopes.last_mut() {
            if let Some(sig) = resolved_sig {
                scope.insert(name.to_string(), sig);
            } else {
                scope.remove(name);
            }
        }
    }

    pub(super) fn function_signature_for_expr(&self, expr: &Expression) -> Option<FunctionSig> {
        match expr {
            Expression::Identifier(ident) => {
                if let Some(sig) = self.functions.get(&ident.name).cloned() {
                    return Some(sig);
                }
                let mut stripped = ident.name.clone();
                while let Some(next) = Self::strip_root_namespace(&stripped) {
                    if next == stripped {
                        break;
                    }
                    if let Some(sig) = self.functions.get(&next).cloned() {
                        return Some(sig);
                    }
                    stripped = next;
                }
                self.lookup_function_value_signature(&ident.name)
            }
            Expression::Call { name, .. } => self.function_return_signatures.get(name).cloned(),
            _ => None,
        }
    }

    pub(super) fn lookup_function_value_signature(&self, name: &str) -> Option<FunctionSig> {
        for scope in self.function_value_sig_scopes.iter().rev() {
            if let Some(found) = scope.get(name) {
                return Some(found.clone());
            }
        }
        None
    }

    pub(super) fn bind_struct_name(
        &mut self,
        name: &str,
        data_type: &DataType,
        value: Option<&Expression>,
    ) {
        let struct_name = data_type
            .struct_name()
            .map(ToOwned::to_owned)
            .or_else(|| value.and_then(|expr| self.struct_name_for_expr(expr)));
        if let Some(scope) = self.struct_scopes.last_mut() {
            if let Some(struct_name) = struct_name {
                scope.insert(name.to_string(), struct_name);
            } else {
                scope.remove(name);
            }
        }
    }

    pub(super) fn bind_reference_type(&mut self, name: &str, value: Option<&Expression>) {
        let referenced_type = value.and_then(|expr| self.referenced_type_from_value(expr));
        if let Some(scope) = self.ref_scopes.last_mut() {
            if let Some(referenced_type) = referenced_type {
                scope.insert(name.to_string(), referenced_type);
            } else {
                scope.remove(name);
            }
        }
    }

    pub(super) fn resolve_assignment_target(
        &mut self,
        target: &AssignmentTarget,
    ) -> Result<Option<(DataType, bool)>> {
        match target {
            AssignmentTarget::Variable(name) => Ok(self.lookup_var(name)),
            AssignmentTarget::Field(path) => {
                let Some((owner, field_path)) = path.split_once('.') else {
                    return Ok(self.lookup_var(path));
                };

                let (mut current_type, is_mutable) = self.lookup_var(owner).ok_or_else(|| {
                    type_error(format!("Assignment to undefined variable '{}'", owner))
                })?;

                for field_name in field_path.split('.') {
                    let struct_name = match &current_type {
                        DataType::StructNamed(name) => name.clone(),
                        other => {
                            return Err(type_error(format!(
                                "Cannot assign field '{}' on non-struct target '{}': {:?}",
                                field_name, owner, other
                            )));
                        }
                    };

                    let class_sig = self.classes.get(&struct_name).ok_or_else(|| {
                        type_error(format!(
                            "Struct '{}' has no field metadata for assignment '{}'",
                            struct_name, path
                        ))
                    })?;
                    let field = class_sig
                        .fields
                        .iter()
                        .find(|field| field.name == field_name)
                        .ok_or_else(|| {
                            type_error(format!(
                                "Struct '{}' has no field '{}'",
                                struct_name, field_name
                            ))
                        })?;
                    current_type = field.data_type.clone();
                }

                Ok(Some((current_type, is_mutable)))
            }
            AssignmentTarget::Index {
                target: index_target,
                index,
            } => {
                let owner_name = target.binding_name().ok_or_else(|| {
                    type_error(
                        "Indexed assignment requires an identifier-backed target".to_string(),
                    )
                })?;
                let (_, is_mutable) = self.lookup_var(owner_name).ok_or_else(|| {
                    type_error(format!("Assignment to undefined variable '{}'", owner_name))
                })?;
                let mut target_expr = index_target.clone();
                let mut index_expr = index.clone();
                let target_type = self.check_expression(&mut target_expr)?;
                let index_type = self.check_expression(&mut index_expr)?;
                if !Self::is_numeric(&index_type) && index_type != DataType::Unknown {
                    return Err(type_error(format!(
                        "Index must be numeric for indexed assignment, got {:?}",
                        index_type
                    )));
                }

                let element_type = match target_type {
                    DataType::Array { element_type, .. }
                    | DataType::Slice { element_type }
                    | DataType::Vector { element_type, .. } => *element_type,
                    DataType::Map { value_type, .. } => *value_type,
                    DataType::List | DataType::Tuple | DataType::Dict => DataType::Anything,
                    DataType::Unknown => DataType::Unknown,
                    other => {
                        return Err(type_error(format!(
                            "Type {:?} does not support indexed assignment",
                            other
                        )));
                    }
                };

                Ok(Some((element_type, is_mutable)))
            }
        }
    }

    pub(super) fn lookup_var(&self, name: &str) -> Option<(DataType, bool)> {
        if name == "self"
            && let Some(ref self_type) = self.impl_self_type
        {
            return Some((self_type.clone(), true));
        }
        for scope in self.scopes.iter().rev() {
            if let Some(data_type) = scope.get(name) {
                return Some(data_type.clone());
            }
        }
        None
    }

    pub(super) fn lookup_struct_name(&self, name: &str) -> Option<String> {
        if name == "self" {
            return self.impl_self_name.clone();
        }
        for scope in self.struct_scopes.iter().rev() {
            if let Some(struct_name) = scope.get(name) {
                return Some(struct_name.clone());
            }
        }
        self.lookup_var(name)
            .and_then(|(data_type, _)| data_type.struct_name().map(ToOwned::to_owned))
    }

    pub(super) fn lookup_ref_type(&self, name: &str) -> Option<DataType> {
        for scope in self.ref_scopes.iter().rev() {
            if let Some(data_type) = scope.get(name) {
                return Some(data_type.clone());
            }
        }
        None
    }

    pub(super) fn struct_name_for_expr(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Call {
                name, data_type, ..
            } if data_type.is_struct_like() => {
                data_type.struct_name().map(ToOwned::to_owned).or_else(|| {
                    if self.classes.contains_key(name) {
                        Some(name.clone())
                    } else if let Some((owner, _method)) = name.split_once('.') {
                        self.lookup_struct_name(owner)
                            .or_else(|| self.classes.contains_key(owner).then(|| owner.to_string()))
                    } else {
                        None
                    }
                })
            }
            Expression::Identifier(Identifier { name, .. }) => self.lookup_struct_name(name),
            Expression::Reference { expr, .. } | Expression::Dereference { expr, .. } => {
                self.struct_name_for_expr(expr)
            }
            _ => None,
        }
    }

    pub(super) fn referenced_type_from_value(&self, expr: &Expression) -> Option<DataType> {
        match expr {
            Expression::Reference { expr, .. } => self.referenced_type_for_expr(expr),
            _ => None,
        }
    }

    pub(super) fn reference_target_is_mutable(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Identifier(Identifier { name, .. }) => self
                .lookup_var(name)
                .map(|(_, is_mutable)| is_mutable)
                .unwrap_or(false),
            Expression::MemberAccess { target, .. } | Expression::Index { target, .. } => {
                self.reference_target_is_mutable(target)
            }
            Expression::Reference { expr, .. } | Expression::Dereference { expr, .. } => {
                self.reference_target_is_mutable(expr)
            }
            _ => false,
        }
    }

    pub(super) fn referenced_type_for_expr(&self, expr: &Expression) -> Option<DataType> {
        match expr {
            Expression::Identifier(Identifier { name, .. }) => self
                .lookup_ref_type(name)
                .or_else(|| {
                    self.lookup_var(name).map(|(data_type, _)| match &data_type {
                        DataType::Ref { inner } | DataType::RefMut { inner } => {
                            *inner.clone()
                        }
                        _ => data_type,
                    })
                }),
            Expression::Reference { expr, .. } => self.referenced_type_for_expr(expr),
            Expression::Dereference { expr, .. } => self.referenced_type_for_expr(expr),
            _ => Some(self.expression_type_hint(expr)),
        }
    }

    pub(super) fn expression_type_hint(&self, expr: &Expression) -> DataType {
        match expr {
            Expression::Identifier(identifier) => identifier.data_type.clone(),
            Expression::BinaryOp { data_type, .. }
            | Expression::UnaryOp { data_type, .. }
            | Expression::NamedArg { data_type, .. }
            | Expression::Call { data_type, .. }
            | Expression::List { data_type, .. }
            | Expression::Dict { data_type, .. }
            | Expression::Tuple { data_type, .. }
            | Expression::Index { data_type, .. }
            | Expression::MemberAccess { data_type, .. }
            | Expression::Reference { data_type, .. }
            | Expression::Dereference { data_type, .. }
            | Expression::Box { data_type, .. }
            | Expression::Pipeline { data_type, .. }
            | Expression::Match { data_type, .. }
            | Expression::Try { data_type, .. }
            | Expression::Ok { data_type, .. }
            | Expression::Err { data_type, .. }
            | Expression::EnumVariantPath { data_type, .. }
            | Expression::EnumVariant { data_type, .. } => data_type.clone(),
            Expression::Literal(Literal::Int(_)) => DataType::I64,
            Expression::Literal(Literal::Float(_)) => DataType::F64,
            Expression::Literal(Literal::Char(_)) => DataType::Char,
            Expression::Literal(Literal::Str(_)) => DataType::Str,
            Expression::Literal(Literal::Bool(_)) => DataType::Bool,
            Expression::Literal(Literal::None) => DataType::None,
            Expression::Literal(Literal::List(_)) => DataType::List,
            Expression::Literal(Literal::Dict(_)) => DataType::Dict,
            Expression::Literal(Literal::Tuple(_)) => DataType::Tuple,
            Expression::Closure { return_type, .. } => return_type.clone(),
        }
    }

    pub(super) fn pipeline_input_element_type(&self, input_type: &DataType) -> DataType {
        match input_type {
            DataType::Vector { element_type, .. }
            | DataType::Array { element_type, .. }
            | DataType::Slice { element_type } => *element_type.clone(),
            DataType::Str => DataType::Str,
            other => other.clone(),
        }
    }

    /// Compute the set of variables captured by a closure body. This is a
    /// scoped free-variable analysis: identifiers that are not parameters or
    /// local declarations, and that resolve to outer-scope variables, become
    /// captures. Nested closures contribute their own free variables to the
    /// outer closure's capture set.
    pub(super) fn collect_captures(
        &self,
        body: &[Statement],
        params: &[(String, DataType)],
        existing: &[(String, DataType)],
    ) -> Vec<(String, DataType)> {
        let mut used = HashSet::new();
        let mut declared: HashSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
        for (n, _) in existing {
            declared.insert(n.clone());
        }
        collect_used_identifiers_in_statements(body, &mut declared, &mut used);
        let mut captures = Vec::new();
        for name in used {
            if name == "self" {
                continue;
            }
            if self.functions.contains_key(&name) {
                continue;
            }
            if let Some((ty, _)) = self.lookup_var(&name) {
                captures.push((name, ty));
            }
        }
        captures.sort_by(|a, b| a.0.cmp(&b.0));
        captures
    }
}

fn collect_used_identifiers_in_statements(
    stmts: &[Statement],
    declared: &mut HashSet<String>,
    used: &mut HashSet<String>,
) {
    for stmt in stmts {
        collect_used_identifiers_in_statement(stmt, declared, used);
    }
}

fn collect_used_identifiers_in_statement(
    stmt: &Statement,
    declared: &mut HashSet<String>,
    used: &mut HashSet<String>,
) {
    match stmt {
        Statement::Let { name, value, .. } => {
            if let Some(v) = value {
                collect_used_identifiers_in_expr(v, declared, used);
            }
            declared.insert(name.clone());
        }
        Statement::Assignment { target, value, .. } => {
            collect_used_identifiers_in_assignment_target(target, declared, used);
            collect_used_identifiers_in_expr(value, declared, used);
            if let AssignmentTarget::Variable(n) = target
                && !declared.contains(n) {
                    used.insert(n.clone());
                }
        }
        Statement::Return(Some(expr))
        | Statement::Expression(expr)
        | Statement::Drop { value: expr } => {
            collect_used_identifiers_in_expr(expr, declared, used);
        }
        Statement::Return(None) => {}
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_used_identifiers_in_expr(condition, declared, used);
            {
                let mut then_declared = declared.clone();
                collect_used_identifiers_in_statements(then_branch, &mut then_declared, used);
            }
            if let Some(else_b) = else_branch {
                let mut else_declared = declared.clone();
                collect_used_identifiers_in_statements(else_b, &mut else_declared, used);
            }
        }
        Statement::While { condition, body } => {
            collect_used_identifiers_in_expr(condition, declared, used);
            let mut body_declared = declared.clone();
            collect_used_identifiers_in_statements(body, &mut body_declared, used);
        }
        Statement::For {
            variable,
            index,
            iterable,
            body,
        } => {
            collect_used_identifiers_in_expr(iterable, declared, used);
            let mut body_declared = declared.clone();
            body_declared.insert(variable.clone());
            if let Some(idx) = index {
                body_declared.insert(idx.clone());
            }
            collect_used_identifiers_in_statements(body, &mut body_declared, used);
        }
        Statement::Find {
            variable,
            iterable,
            body,
        } => {
            collect_used_identifiers_in_expr(iterable, declared, used);
            let mut body_declared = declared.clone();
            body_declared.insert(variable.clone());
            collect_used_identifiers_in_statements(body, &mut body_declared, used);
        }
        Statement::Match {
            value,
            cases,
            default,
        } => {
            collect_used_identifiers_in_expr(value, declared, used);
            for (pattern, body) in cases {
                let mut pat_declared = declared.clone();
                collect_pattern_bindings(pattern, &mut pat_declared);
                collect_used_identifiers_in_statements(body, &mut pat_declared, used);
            }
            let mut default_declared = declared.clone();
            collect_used_identifiers_in_statements(default, &mut default_declared, used);
        }
        Statement::Function {
            name, params, body, ..
        } => {
            declared.insert(name.clone());
            let mut fn_declared: HashSet<String> =
                params.iter().map(|(n, _)| n.clone()).collect();
            collect_used_identifiers_in_statements(body, &mut fn_declared, used);
        }
        Statement::Unsafe { body } => {
            let mut unsafe_declared = declared.clone();
            collect_used_identifiers_in_statements(body, &mut unsafe_declared, used);
        }
        Statement::Asm { instructions } => {
            for (_, expr) in instructions {
                collect_used_identifiers_in_expr(expr, declared, used);
            }
        }
        Statement::New { value: Some(v), .. } => {
            collect_used_identifiers_in_expr(v, declared, used);
        }
        Statement::New { .. } => {}
        Statement::Own { value: Some(v), .. } => {
            collect_used_identifiers_in_expr(v, declared, used);
        }
        Statement::Own { .. } => {}
        Statement::Move { target, value } => {
            collect_used_identifiers_in_expr(value, declared, used);
            if !declared.contains(target) {
                used.insert(target.clone());
            }
        }
        _ => {}
    }
}

fn collect_used_identifiers_in_assignment_target(
    target: &AssignmentTarget,
    declared: &mut HashSet<String>,
    used: &mut HashSet<String>,
) {
    match target {
        AssignmentTarget::Variable(_) | AssignmentTarget::Field(_) => {}
        AssignmentTarget::Index { target, index } => {
            collect_used_identifiers_in_expr(target, declared, used);
            collect_used_identifiers_in_expr(index, declared, used);
        }
    }
}

fn collect_pattern_bindings(pattern: &Expression, declared: &mut HashSet<String>) {
    match pattern {
        Expression::Identifier(ident) => {
            declared.insert(ident.name.clone());
        }
        Expression::Tuple { elements, .. } | Expression::List { elements, .. } => {
            for e in elements {
                collect_pattern_bindings(e, declared);
            }
        }
        Expression::EnumVariant { payloads, .. } => {
            for p in payloads {
                collect_pattern_bindings(p, declared);
            }
        }
        _ => {}
    }
}

fn collect_used_identifiers_in_expr(
    expr: &Expression,
    declared: &HashSet<String>,
    used: &mut HashSet<String>,
) {
    match expr {
        Expression::Identifier(ident) => {
            if !declared.contains(&ident.name) {
                used.insert(ident.name.clone());
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_used_identifiers_in_expr(left, declared, used);
            collect_used_identifiers_in_expr(right, declared, used);
        }
        Expression::UnaryOp { operand, .. } => {
            collect_used_identifiers_in_expr(operand, declared, used);
        }
        Expression::NamedArg { value, .. } => {
            collect_used_identifiers_in_expr(value, declared, used);
        }
        Expression::Call { name, args, .. } => {
            if !name.contains('.') && !declared.contains(name) {
                used.insert(name.clone());
            }
            if let Some((prefix, _)) = name.split_once('.')
                && prefix != "self" && !declared.contains(prefix) {
                    used.insert(prefix.to_string());
                }
            for arg in args {
                collect_used_identifiers_in_expr(arg, declared, used);
            }
        }
        Expression::List { elements, .. } | Expression::Tuple { elements, .. } => {
            for e in elements {
                collect_used_identifiers_in_expr(e, declared, used);
            }
        }
        Expression::Dict { entries, .. } => {
            for (k, v) in entries {
                collect_used_identifiers_in_expr(k, declared, used);
                collect_used_identifiers_in_expr(v, declared, used);
            }
        }
        Expression::Index { target, index, .. } => {
            collect_used_identifiers_in_expr(target, declared, used);
            collect_used_identifiers_in_expr(index, declared, used);
        }
        Expression::MemberAccess { target, .. } => {
            collect_used_identifiers_in_expr(target, declared, used);
        }
        Expression::Reference { expr, .. }
        | Expression::Dereference { expr, .. }
        | Expression::Box { value: expr, .. }
        | Expression::Try { expr, .. }
        | Expression::Ok { value: expr, .. }
        | Expression::Err { value: expr, .. } => {
            collect_used_identifiers_in_expr(expr, declared, used);
        }
        Expression::Pipeline { input, stage, .. } => {
            collect_used_identifiers_in_expr(input, declared, used);
            collect_used_identifiers_in_expr(stage, declared, used);
        }
        Expression::Match {
            value,
            cases,
            default,
            ..
        } => {
            collect_used_identifiers_in_expr(value, declared, used);
            for (pat, body_expr) in cases {
                let mut pat_declared = declared.clone();
                collect_pattern_bindings(pat, &mut pat_declared);
                collect_used_identifiers_in_expr(body_expr, &pat_declared, used);
            }
            collect_used_identifiers_in_expr(default, declared, used);
        }
        Expression::Closure { params, body, .. } => {
            let mut inner_declared = declared.clone();
            for (n, _) in params {
                inner_declared.insert(n.clone());
            }
            collect_used_identifiers_in_statements(body, &mut inner_declared, used);
        }
        Expression::EnumVariant { payloads, .. } => {
            for p in payloads {
                collect_used_identifiers_in_expr(p, declared, used);
            }
        }
        Expression::Literal(Literal::List(elements))
        | Expression::Literal(Literal::Tuple(elements)) => {
            for e in elements {
                collect_used_identifiers_in_expr(e, declared, used);
            }
        }
        Expression::Literal(Literal::Dict(entries)) => {
            for ((k, v), _) in entries {
                collect_used_identifiers_in_expr(k, declared, used);
                collect_used_identifiers_in_expr(v, declared, used);
            }
        }
        _ => {}
    }
}
