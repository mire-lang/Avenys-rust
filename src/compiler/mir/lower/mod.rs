use super::*;
use crate::parser::ast::{DataType, Program, Statement};
use std::collections::HashMap;
use std::collections::HashSet;

mod expr;
mod stmt;
mod decl;
mod types;

struct MirLower {
    func: MirFunction,
    next_temp: usize,
    vars: HashMap<String, usize>,
    var_types: HashMap<String, DataType>,
    struct_types: HashMap<String, Vec<(String, DataType)>>,
    enum_types: HashMap<String, Vec<(String, usize)>>,
    bare_to_qualified: HashMap<String, String>,
    method_map: HashMap<String, HashMap<String, String>>,
    current_block: usize,
    closure_functions: Vec<MirFunction>,
    closure_counter: usize,
}

fn extract_struct_types(program: &Program) -> HashMap<String, Vec<(String, DataType)>> {
    let mut struct_types = HashMap::new();
    for stmt in &program.statements {
        if let Statement::Type { name, fields, .. } = stmt {
            let mut field_list = Vec::new();
            for f in fields {
                if let Statement::Let { name, data_type, .. } = f {
                    field_list.push((name.clone(), data_type.clone()));
                }
            }
            struct_types.insert(name.clone(), field_list);
        }
    }
    struct_types
}

fn extract_enum_types(program: &Program) -> HashMap<String, Vec<(String, usize)>> {
    let mut enum_types = HashMap::new();
    for stmt in &program.statements {
        if let Statement::Enum { name, variants, .. } = stmt {
            let mapped: Vec<(String, usize)> = variants
                .iter()
                .enumerate()
                .map(|(i, v)| (v.name.clone(), i))
                .collect();
            enum_types.insert(name.clone(), mapped);
        }
    }
    enum_types
}

fn extract_method_map(program: &Program) -> HashMap<String, HashMap<String, String>> {
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    for stmt in &program.statements {
        if let Statement::Impl { type_name, methods, .. } = stmt {
            let norm = type_name.split_once('[')
                .map(|(b, _)| b.to_string())
                .unwrap_or_else(|| type_name.clone());
            let entry = map.entry(norm).or_default();
            for method in methods {
                if let Statement::Function { name, .. } = method {
                    entry.insert(name.clone(), format!("{}.{}", type_name, name));
                }
            }
        }
    }
    map
}

fn extract_bare_name_map(
    _program: &Program,
    seen_functions: &std::collections::HashSet<String>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for name in seen_functions.iter() {
        if let Some((_, bare)) = name.rsplit_once('.') {
            if seen_functions.contains(bare) {
                continue;
            }
            if !map.contains_key(bare) {
                map.insert(bare.to_string(), name.clone());
            }
        }
    }
    map
}

pub fn lower_program(program: &Program) -> MirProgram {
    let mut functions = Vec::new();
    let mut entry_point = None;
    let mut extern_functions = Vec::new();
    let mut seen_functions = HashSet::new();
    let mut struct_types = extract_struct_types(program);
    let enum_types = extract_enum_types(program);
    let method_map = extract_method_map(program);

    for stmt in &program.statements {
        if let Statement::ExternFunction { name, params, return_type, .. } = stmt {
            extern_functions.push(MirExternFunction {
                name: name.clone(),
                params: params.iter().map(|(_, t)| t.clone()).collect(),
                return_type: return_type.clone(),
            });
        }
    }

    for stmt in &program.statements {
        if let Statement::Function { name, .. } = stmt {
            seen_functions.insert(name.clone());
        }
        if let Statement::Impl { type_name, methods, .. } = stmt {
            for method in methods {
                if let Statement::Function { name, .. } = method {
                    seen_functions.insert(format!("{}.{}", type_name, name));
                }
            }
        }
    }

    let bare_to_qualified = extract_bare_name_map(program, &seen_functions);

    seen_functions.clear();

    for stmt in &program.statements {
        match stmt {
            Statement::Function {
                name,
                params,
                return_type,
                body,
                ..
            } => {
                if !seen_functions.insert(name.clone()) {
                    continue;
                }
                let mir_params = params
                    .iter()
                    .map(|(pname, ptype)| MirParam {
                        name: pname.clone(),
                        data_type: ptype.clone(),
                    })
                    .collect();

                let mut lower = MirLower {
                    func: MirFunction::new(name.clone(), mir_params, return_type.clone()),
                    next_temp: 0,
                    vars: HashMap::new(),
                    var_types: HashMap::new(),
                    struct_types: struct_types.clone(),
                    enum_types: enum_types.clone(),
                    bare_to_qualified: bare_to_qualified.clone(),
                    method_map: method_map.clone(),
                    current_block: 0,
                    closure_functions: Vec::new(),
                    closure_counter: 0,
                };

                lower.lower_function_body(body);
                struct_types.extend(lower.struct_types.clone());
                functions.extend(lower.closure_functions);
                if !lower.func.blocks.is_empty() {
                    lower.func.blocks[0].label = "entry".to_string();
                }
                lower.func.body_hash = lower.func.compute_hash();
                functions.push(lower.func);

                if name == "main" {
                    entry_point = Some(name.clone());
                }
            }
            Statement::Impl {
                type_name,
                methods,
                ..
            } => {
                for method in methods {
                    if let Statement::Function {
                        name,
                        params,
                        return_type,
                        body,
                        ..
                    } = method
                    {
                        let full_name = format!("{}.{}", type_name, name);
                        if !seen_functions.insert(full_name.clone()) {
                            continue;
                        }
                        let mir_params: Vec<MirParam> = params
                            .iter()
                            .map(|(pname, ptype)| {
                                let dt = if pname == "self" && ptype == &DataType::Unknown {
                                    DataType::StructNamed(type_name.clone())
                                } else {
                                    ptype.clone()
                                };
                                MirParam {
                                    name: pname.clone(),
                                    data_type: dt,
                                }
                            })
                            .collect();

                            let mut lower = MirLower {
                                func: MirFunction::new(full_name, mir_params, return_type.clone()),
                                next_temp: 0,
                                vars: HashMap::new(),
                                var_types: HashMap::new(),
                                struct_types: struct_types.clone(),
                                enum_types: enum_types.clone(),
                                bare_to_qualified: bare_to_qualified.clone(),
                                method_map: method_map.clone(),
                                current_block: 0,
                                closure_functions: Vec::new(),
                                closure_counter: 0,
                            };

                        lower.lower_function_body(body);
                        struct_types.extend(lower.struct_types.clone());
                        functions.extend(lower.closure_functions);
                        if !lower.func.blocks.is_empty() {
                            lower.func.blocks[0].label = "entry".to_string();
                        }
                        lower.func.body_hash = lower.func.compute_hash();
                        functions.push(lower.func);
                    }
                }
            }
            _ => {}
        }
    }

    let mut mp = MirProgram::new(functions, entry_point);
    mp.extern_functions = extern_functions;
    mp.struct_types = struct_types;
    mp
}

impl MirLower {
    pub(crate) fn new_temp(&mut self) -> usize {
        let id = self.next_temp;
        self.next_temp += 1;
        id
    }

    pub(crate) fn new_block(&mut self, label: &str) -> usize {
        let id = self.func.blocks.len();
        self.func.push_block(format!("{}_{}", label, id));
        id
    }

    pub(crate) fn entry_block_id(&mut self) -> usize {
        if self.func.blocks.is_empty() {
            self.func.push_block("entry".to_string());
        }
        0
    }
}
