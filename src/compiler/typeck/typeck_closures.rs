use super::*;
use crate::error::type_error_at_span;

impl TypeChecker {
    pub(super) fn check_expression_allow_unknown_identifier(
        &mut self,
        expression: &mut Expression,
    ) -> Result<DataType> {
        match expression {
            Expression::Identifier(ident) => {
                if let Some((resolved, _)) = self.lookup_var(&ident.name) {
                    ident.data_type = resolved.clone();
                    Ok(resolved)
                } else {
                    ident.data_type = DataType::Unknown;
                    Ok(DataType::Unknown)
                }
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                data_type,
            } if Self::is_logical_operator(operator) => {
                let left_type = self.check_expression_allow_unknown_identifier(left)?;
                let right_type = self.check_expression_allow_unknown_identifier(right)?;
                let resolved = self.resolve_binary_type(operator, &left_type, &right_type)?;
                *data_type = resolved.clone();
                Ok(resolved)
            }
            _ => self.check_expression(expression),
        }
    }

    pub(super) fn closure_return_type(expr: &Expression, context: &str) -> Result<DataType> {
        if let Expression::Closure { return_type, .. } = expr {
            Ok(return_type.clone())
        } else {
            Err(type_error(
                0,
                0,
                format!("{} must be represented as a closure in the AST", context),
            ))
        }
    }

    pub(super) fn infer_list_element_type(list_type: DataType) -> Result<DataType> {
        match list_type {
            DataType::Vector { element_type, .. } => Ok(*element_type),
            DataType::Array { element_type, .. } => Ok(*element_type),
            DataType::Slice { element_type } => Ok(*element_type),
            DataType::List => Ok(DataType::Anything),
            other => Err(type_error(
                0,
                0,
                format!(
                    "High-order list function expects vec/arr/slice input, got {:?}",
                    other
                ),
            )),
        }
    }

    pub(super) fn check_closure_with_expected_params(
        &mut self,
        expr: &mut Expression,
        expected_params: &[DataType],
        context: &str,
    ) -> Result<DataType> {
        let Expression::Closure {
            params,
            body,
            return_type,
            capture,
        } = expr
        else {
            return Err(type_error_at_span(
                self.current_span,
                format!("{} expects a closure argument", context),
            ));
        };

        if params.len() != expected_params.len() {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "{} expects a closure with {} parameter(s), got {}",
                    context,
                    expected_params.len(),
                    params.len()
                ),
            ));
        }

        self.push_scope();

        let captures = self.collect_captures(body, params, capture);
        *capture = captures;

        for (name, data_type) in capture.iter() {
            self.insert_var(name.clone(), data_type.clone(), true);
        }

        for ((name, param_type), expected_type) in params.iter_mut().zip(expected_params.iter()) {
            let resolved = Self::unify_types(param_type, expected_type)?;
            *param_type = resolved.clone();
            self.insert_var(name.clone(), resolved, true);
        }

        self.return_type_stack.push(return_type.clone());
        self.check_statements(body)?;
        if !statements_contain_explicit_return(body)
            && let Some(expr) = implicit_return_expression_mut(body)
        {
            let tail_type = self.check_expression(expr)?;
            if let Some(current) = self.return_type_stack.last_mut() {
                let unified = Self::unify_types(current, &tail_type)?;
                *current = unified;
            }
        }
        let inferred_return = self.return_type_stack.pop().unwrap_or(DataType::Unknown);

        if *return_type == DataType::Unknown {
            *return_type = inferred_return.clone();
        } else if inferred_return != DataType::Unknown
            && !self.is_assignable(return_type, &inferred_return)
        {
            return Err(type_error_at_span(
                self.current_span,
                format!(
                    "{} return type mismatch: declared {:?}, inferred {:?}",
                    context, return_type, inferred_return
                ),
            ));
        }

        self.pop_scope();
        Ok(return_type.clone())
    }
}
