use crate::error::Result;
use crate::parser::ast::{DataType, Expression, Statement};

use crate::compiler::typeck::{
    ClassFieldSig, ClassSig, EnumVariantSig, FunctionSig, TraitSig, TypeChecker,
};

impl TypeChecker {
    pub(super) fn collect_function_signatures(&mut self, statements: &[Statement]) -> Result<()> {
        for statement in statements {
            match statement {
                Statement::Function {
                    name,
                    type_params,
                    type_param_bounds,
                    params,
                    return_type,
                    ..
                } => {
                    self.functions.insert(
                        name.replace("::", "."),
                        FunctionSig {
                            type_params: type_params.clone(),
                            type_param_bounds: type_param_bounds.clone(),
                            params: params.iter().map(|(_, t)| t.clone()).collect(),
                            return_type: return_type.clone(),
                        },
                    );
                }
                Statement::ExternFunction {
                    name,
                    params,
                    return_type,
                    ..
                } => {
                    self.functions.insert(
                        name.replace("::", "."),
                        FunctionSig {
                            type_params: Vec::new(),
                            type_param_bounds: Vec::new(),
                            params: params.iter().map(|(_, t)| t.clone()).collect(),
                            return_type: return_type.clone(),
                        },
                    );
                }
                Statement::Impl {
                    trait_name,
                    type_name,
                    methods,
                    ..
                } => {
                    if let Some(trait_name) = trait_name {
                        self.impl_traits
                            .entry(type_name.clone())
                            .or_default()
                            .insert(trait_name.clone());
                    }
                    for method in methods {
                        if let Statement::Function {
                            name,
                            params,
                            return_type,
                            ..
                        } = method
                        {
                            let mut full_params = params.clone();
                            if let Some((_, self_ty)) =
                                full_params.iter_mut().find(|(param, _)| param == "self")
                            {
                                *self_ty = DataType::StructNamed(type_name.clone());
                            }
                            self.functions.insert(
                                format!("{}.{}", type_name, name),
                                FunctionSig {
                                    type_params: Vec::new(),
                                    type_param_bounds: Vec::new(),
                                    params: full_params.iter().map(|(_, t)| t.clone()).collect(),
                                    return_type: return_type.clone(),
                                },
                            );
                        }
                    }
                    self.collect_function_signatures(methods)?;
                }
                Statement::Module { .. } => {} // Module declarations don't contain functions
                Statement::Skill { name, methods, .. } => {
                    self.traits.insert(
                        name.clone(),
                        TraitSig {
                            methods: methods.clone(),
                        },
                    );
                }
                Statement::Type {
                    name,
                    type_params,
                    type_param_bounds,
                    fields,
                    ..
                } => {
                    let type_fields = fields
                        .iter()
                        .filter_map(|statement| match statement {
                            Statement::Let {
                                name,
                                data_type,
                                value,
                                ..
                            } => Some(ClassFieldSig {
                                name: name.clone(),
                                data_type: data_type.clone(),
                                has_default: value.is_some(),
                            }),
                            _ => None,
                        })
                        .collect();
                    self.classes.insert(
                        name.clone(),
                        ClassSig {
                            type_params: type_params.clone(),
                            type_param_bounds: type_param_bounds.clone(),
                            fields: type_fields,
                        },
                    );
                    self.collect_function_signatures(fields)?
                }
                Statement::Enum {
                    name,
                    type_params,
                    type_param_bounds,
                    variants,
                    ..
                } => {
                    for variant in variants {
                        let full_name = format!("{}.{}", name, variant.name);
                        self.enum_variants.insert(
                            full_name,
                            EnumVariantSig {
                                type_params: type_params.clone(),
                                type_param_bounds: type_param_bounds.clone(),
                                payload_names: variant.payload_names.clone(),
                                payload_types: variant.data_types.clone(),
                            },
                        );
                    }
                    self.insert_var(name.clone(), DataType::EnumNamed(name.clone()), true);
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn collect_function_return_signatures(
        &mut self,
        statements: &[Statement],
    ) -> Result<()> {
        for statement in statements {
            match statement {
                Statement::Function {
                    name,
                    return_type,
                    body,
                    ..
                } => {
                    if *return_type == DataType::Function
                        && let Some(sig) = self.infer_returned_function_signature(body)
                    {
                        self.function_return_signatures.insert(name.replace("::", "."), sig);
                    }
                }
                Statement::Impl {
                    type_name, methods, ..
                } => {
                    for method in methods {
                        if let Statement::Function {
                            name,
                            return_type,
                            body,
                            ..
                        } = method
                            && *return_type == DataType::Function
                            && let Some(sig) = self.infer_returned_function_signature(body)
                        {
                            self.function_return_signatures
                                .insert(format!("{}.{}", type_name, name), sig);
                        }
                    }
                    self.collect_function_return_signatures(methods)?;
                }
                Statement::Module { .. } => {} // Module declarations don't contain functions
                Statement::Type { fields, .. } => {
                    self.collect_function_return_signatures(fields)?
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn infer_returned_function_signature(&self, body: &[Statement]) -> Option<FunctionSig> {
        body.iter().find_map(|statement| match statement {
            Statement::Return(Some(Expression::Identifier(ident))) => {
                self.functions.get(&ident.name).cloned().or_else(|| {
                    let mut stripped = ident.name.clone();
                    loop {
                        if let Some(next) = Self::strip_root_namespace(&stripped) {
                            if next == stripped {
                                break None;
                            }
                            if let Some(sig) = self.functions.get(&next).cloned() {
                                break Some(sig);
                            }
                            stripped = next;
                        } else {
                            break None;
                        }
                    }
                })
            }
            _ => None,
        })
    }
}
