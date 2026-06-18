use super::*;
use crate::parser::ast::{
    AssignmentTarget, DataType, Expression, Literal, Program, Statement,
};
use std::collections::HashMap;
use std::collections::HashSet;

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
            // Normalize generic type names: Box[T] → Box
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

fn extract_data_type(expr: &Expression) -> DataType {
    match expr {
        Expression::Literal(lit) => match lit {
            Literal::Int(_) => DataType::I64,
            Literal::Float(_) => DataType::F64,
            Literal::Bool(_) => DataType::Bool,
            Literal::Str(_) => DataType::Str,
            Literal::Char(_) => DataType::Char,
            Literal::None => DataType::None,
            Literal::List(_) => DataType::List,
            Literal::Dict(..) => DataType::Dict,
            Literal::Tuple(_) => DataType::Tuple,
        },
        Expression::Identifier(id) => id.data_type.clone(),
        Expression::BinaryOp { data_type, .. } => data_type.clone(),
        Expression::UnaryOp { data_type, .. } => data_type.clone(),
        Expression::NamedArg { data_type, .. } => data_type.clone(),
        Expression::Call { data_type, .. } => data_type.clone(),
        Expression::List { data_type, .. } => data_type.clone(),
        Expression::Dict { data_type, .. } => data_type.clone(),
        Expression::Tuple { data_type, .. } => data_type.clone(),
        Expression::Index { data_type, .. } => data_type.clone(),
        Expression::MemberAccess { data_type, .. } => data_type.clone(),
        Expression::Closure { return_type, .. } => return_type.clone(),
        Expression::Reference { data_type, .. } => data_type.clone(),
        Expression::Dereference { data_type, .. } => data_type.clone(),
        Expression::Box { data_type, .. } => data_type.clone(),
        Expression::Pipeline { data_type, .. } => data_type.clone(),
        Expression::Try { data_type, .. } => data_type.clone(),
        Expression::Ok { data_type, .. } => data_type.clone(),
        Expression::Err { data_type, .. } => data_type.clone(),
        Expression::Match { data_type, .. } => data_type.clone(),
        Expression::EnumVariantPath { data_type, .. } => data_type.clone(),
        Expression::EnumVariant { data_type, .. } => data_type.clone(),
    }
}

fn is_map_or_dict_type(dt: &DataType) -> bool {
    matches!(dt, DataType::Map { .. } | DataType::Dict)
}

fn is_trivial_deref(source: &DataType, target: &DataType) -> bool {
    if !matches!(source, DataType::Ref { .. } | DataType::RefMut { .. }) {
        return false;
    }
    // If the target type maps to "ptr" at LLVM level (same as any Ref type),
    // then the dereference is a no-op: &T and T have the same representation.
    matches!(target,
        DataType::Str
        | DataType::List
        | DataType::Vector { .. }
        | DataType::Dict
        | DataType::Map { .. }
        | DataType::Box
        | DataType::Struct
        | DataType::StructNamed(_)
        | DataType::Function
        | DataType::Tuple
        | DataType::Set
        | DataType::Datetime
        | DataType::Slice { .. }
        | DataType::DynTrait { .. }
        | DataType::Result { .. }
    )
}

fn data_type_to_kind(dt: &DataType) -> i64 {
    match dt {
        DataType::Bool => 2,
        DataType::Str => 3,
        DataType::Map { .. } | DataType::Dict => 4,
        DataType::Vector { .. }
        | DataType::List
        | DataType::Array { .. }
        | DataType::Struct
        | DataType::StructNamed(_)
        | DataType::Ref { .. }
        | DataType::RefMut { .. }
        | DataType::Box => 5,
        _ => 1,
    }
}

fn extract_bare_name_map(
    _program: &Program,
    seen_functions: &std::collections::HashSet<String>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for name in seen_functions.iter() {
        if let Some((_, bare)) = name.rsplit_once('.') {
            if seen_functions.contains(bare) {
                // Ambiguous — bare name already exists as a defined function
                continue;
            }
            // Only insert if no previous mapping conflicts
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

    // Collect all function names first for bare name resolution
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

    // Reset seen_functions for dedup during function processing
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
    fn new_temp(&mut self) -> usize {
        let id = self.next_temp;
        self.next_temp += 1;
        id
    }

    fn new_block(&mut self, label: &str) -> usize {
        let id = self.func.blocks.len();
        self.func.push_block(format!("{}_{}", label, id));
        id
    }

    fn lower_call_args(&mut self, name: &str, args: &[Expression]) -> Vec<MirValue> {
        let needs_wrap = name == "dasu" || name == "print" || name == "str";
        args.iter()
            .map(|a| {
                let arg_type = extract_data_type(a);
                let lowered = self.lower_expression(a);
                if needs_wrap && is_map_or_dict_type(&arg_type) {
                    let str_result = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(str_result),
                        MirOp::Call(
                            MirValue::Global("rt_dict_to_string".to_string()),
                            vec![lowered],
                            MirType { data_type: DataType::Unknown },
                        ),
                        (0, 0),
                    );
                    MirValue::temp(str_result)
                } else {
                    lowered
                }
            })
            .collect()
    }

    fn entry_block_id(&mut self) -> usize {
        if self.func.blocks.is_empty() {
            self.func.push_block("entry".to_string());
        }
        0
    }

    fn lower_function_body(&mut self, body: &[Statement]) {
        let entry_id = self.entry_block_id();

        let nparams = self.func.params.len();
        let mut param_list: Vec<(String, DataType, usize)> = Vec::with_capacity(nparams);
        for i in 0..nparams {
            let ptr = self.new_temp();
            param_list.push((
                self.func.params[i].name.clone(),
                self.func.params[i].data_type.clone(),
                ptr,
            ));
        }

        {
            let entry = &mut self.func.blocks[entry_id];
            for (pname, ptype, ptr) in &param_list {
                entry.push(
                    Some(*ptr),
                    MirOp::Alloca(MirType {
                        data_type: ptype.clone(),
                    }),
                    (0, 0),
                );
                self.var_types.insert(pname.clone(), ptype.clone());
            }
            for (pname, _ptype, ptr) in &param_list {
                entry.push(
                    None,
                    MirOp::Store(MirValue::temp(*ptr), MirValue::Param(pname.clone())),
                    (0, 0),
                );
            }
        }
        for (pname, _ptype, ptr) in &param_list {
            self.vars.insert(pname.clone(), *ptr);
        }

        for stmt in body {
            self.lower_statement(stmt);
        }
    }

    fn lower_statement(&mut self, stmt: &Statement) {
        let loc = (0, 0);
        match stmt {
            Statement::Let {
                name,
                data_type,
                value,
                ..
            } => {
                if self.func.blocks.is_empty() {
                    self.func.push_block("entry".to_string());
                }
                let last = self.current_block;
                let ptr = self.new_temp();
                self.func.blocks[last].push(
                    Some(ptr),
                    MirOp::Alloca(MirType {
                        data_type: data_type.clone(),
                    }),
                    loc,
                );
                self.vars.insert(name.clone(), ptr);
                self.var_types.insert(name.clone(), data_type.clone());

                if let Some(val) = value {
                    // Special-case: array literal initialization
                    if let DataType::Array { element_type, .. } = data_type {
                        if let Expression::List { elements, .. } = val {
                            let elem_llvm = llvm_elem_type_str(element_type);
                            for (i, elem) in elements.iter().enumerate() {
                                let elem_val = self.lower_expression(elem);
                                let last = self.current_block;
                                let gep = self.new_temp();
                                self.func.blocks[last].push(
                                    Some(gep),
                                    MirOp::Gep(
                                        MirValue::temp(ptr),
                                        vec![MirValue::Const(MirConst::Int(i as i64))],
                                        elem_llvm.clone(),
                                    ),
                                    loc,
                                );
                                self.func.blocks[last].push(
                                    None,
                                    MirOp::Store(MirValue::temp(gep), elem_val),
                                    loc,
                                );
                            }
                            return;
                        }
                    }
                    let v = self.lower_expression(val);
                    let last = self.current_block;
                    self.func.blocks[last].push(None, MirOp::Store(MirValue::temp(ptr), v), loc);
                }
            }
            Statement::Assignment { target, value, .. } => {
                let v = self.lower_expression(value);
                let last = self.current_block;
                match target {
                    AssignmentTarget::Variable(name) => {
                        if let Some(&ptr) = self.vars.get(name) {
                            self.func.blocks[last]
                                .push(None, MirOp::Store(MirValue::temp(ptr), v), loc);
                        }
                    }
                    AssignmentTarget::Index { target, index } => {
                        let target_val = self.lower_expression(target);
                        let index_val = self.lower_expression(index);
                        let target_type = extract_data_type(target);
                        let last = self.current_block;

                        // Runtime bounds check before indexing.
                        match &target_type {
                            DataType::Array { size, .. } => {
                                self.func.blocks[last].push(
                                    None,
                        MirOp::Call(
                            MirValue::Global("rt_check_bounds_i64".to_string()),
                                        vec![
                                            index_val.clone(),
                                            MirValue::Const(MirConst::Int(*size as i64)),
                                        ],
                                        MirType { data_type: DataType::None },
                                    ),
                                    loc,
                                );
                            }
                            DataType::Vector { .. } | DataType::List => {
                                let len_val = self.new_temp();
                                self.func.blocks[last].push(
                                    Some(len_val),
                        MirOp::Call(
                            MirValue::Global("rt_list_len".to_string()),
                                        vec![target_val.clone()],
                                        MirType { data_type: DataType::I64 },
                                    ),
                                    loc,
                                );
                                self.func.blocks[last].push(
                                    None,
                        MirOp::Call(
                            MirValue::Global("rt_check_bounds_i64".to_string()),
                                        vec![index_val.clone(), MirValue::temp(len_val)],
                                        MirType { data_type: DataType::None },
                                    ),
                                    loc,
                                );
                            }
                            _ => {}
                        }

                        let gep = self.new_temp();
                        let elem_ty = self.get_target_elem_type(target);
                        self.func.blocks[last].push(
                            Some(gep),
                            MirOp::Gep(target_val, vec![index_val], elem_ty),
                            loc,
                        );
                        self.func.blocks[last]
                            .push(None, MirOp::Store(MirValue::temp(gep), v), loc);
                    }
                    AssignmentTarget::Field(path) => {
                        let parts: Vec<&str> = path.splitn(2, '.').collect();
                        if parts.len() == 2 {
                            let var_name = parts[0].to_string();
                            let field_name = parts[1].to_string();
                            let ptr = self.vars.get(&var_name).copied();
                            let var_type = self.var_types.get(&var_name).cloned();
                            if let (Some(ptr), Some(var_type)) = (ptr, var_type) {
                                if let DataType::StructNamed(ref struct_name) = var_type {
                                    let norm_name = struct_name.split_once('[')
                                        .map(|(base, _)| base.to_string())
                                        .unwrap_or_else(|| struct_name.clone());
                                    let field_idx = self.struct_types.get(&norm_name)
                                        .and_then(|fields| fields.iter().position(|(n, _)| n == &field_name));
                                    if let Some(field_idx) = field_idx {
                                        let data_ptr = self.new_temp();
                                        self.func.blocks[last].push(
                                            Some(data_ptr),
                                            MirOp::Load(
                                                MirValue::temp(ptr),
                                                MirType { data_type: var_type },
                                            ),
                                            loc,
                                        );
                                        let gep = self.new_temp();
                                        self.func.blocks[last].push(
                                            Some(gep),
                                            MirOp::Gep(
                                                MirValue::temp(data_ptr),
                                                vec![
                                                    MirValue::Const(MirConst::Int(0)),
                                                    MirValue::Const(MirConst::Int(field_idx as i64)),
                                                ],
                                                norm_name,
                                            ),
                                            loc,
                                        );
                                        self.func.blocks[last]
                                            .push(None, MirOp::Store(MirValue::temp(gep), v), loc);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Statement::Expression(expr) => {
                let _ = self.lower_expression(expr);
            }
            Statement::Return(val) => {
                let v = val.as_ref().map(|e| self.lower_expression(e));
                let last = self.current_block;
                self.func.blocks[last].terminator = MirTerminator::Ret(v);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.lower_expression(condition);
                let pre_if = self.current_block;

                let then_block = self.new_block("if_then");
                self.current_block = then_block;

                for stmt in then_branch {
                    self.lower_statement(stmt);
                }
                let then_last = self.current_block;

                let else_block = self.new_block("if_else");
                self.current_block = else_block;

                if let Some(else_body) = else_branch {
                    for stmt in else_body {
                        self.lower_statement(stmt);
                    }
                }
                let else_last = self.current_block;

                let end_block = self.new_block("if_end");
                if matches!(self.func.blocks[then_last].terminator, MirTerminator::Unreachable) {
                    self.func.blocks[then_last].terminator = MirTerminator::Br(end_block);
                }
                if matches!(self.func.blocks[else_last].terminator, MirTerminator::Unreachable) {
                    self.func.blocks[else_last].terminator = MirTerminator::Br(end_block);
                }
                self.func.blocks[pre_if].terminator =
                    MirTerminator::BrCond(cond, then_block, else_block);
                self.current_block = end_block;
            }
            Statement::While { condition, body, .. } => {
                let pre_while_block = self.current_block;

                let cond_block = self.new_block("while_cond");
                self.current_block = cond_block;

                let cond = self.lower_expression(condition);

                let body_block = self.new_block("while_body");
                self.current_block = body_block;

                for stmt in body {
                    self.lower_statement(stmt);
                }
                if matches!(self.func.blocks[self.current_block].terminator, MirTerminator::Unreachable) {
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(cond_block);
                }

                let end_block = self.new_block("while_end");
                self.func.blocks[cond_block].terminator =
                    MirTerminator::BrCond(cond, body_block, end_block);
                self.func.blocks[pre_while_block].terminator = MirTerminator::Br(cond_block);
                self.current_block = end_block;
            }
            Statement::For { variable, index, iterable, body } => {
                let idx_ptr = self.new_temp();

                // alloca for index
                self.func.blocks[self.current_block].push(
                    Some(idx_ptr),
                    MirOp::Alloca(MirType { data_type: DataType::I64 }),
                    loc,
                );

                // evaluate iterator (call result used directly, no alloca)
                let iter_val = self.lower_expression(iterable);

                // store initial index 0
                self.func.blocks[self.current_block].push(
                    None,
                    MirOp::Store(
                        MirValue::temp(idx_ptr),
                        MirValue::Const(MirConst::Int(0)),
                    ),
                    loc,
                );

                let pre_for_block = self.current_block;
                let cond_block = self.new_block("for_cond");

                // Push cond code to cond_block directly (it's not the "current" block yet)
                let idx_loaded = self.new_temp();
                self.func.blocks[cond_block].push(
                    Some(idx_loaded),
                    MirOp::Load(MirValue::temp(idx_ptr), MirType { data_type: DataType::I64 }),
                    loc,
                );
                let len_val = self.new_temp();
                self.func.blocks[cond_block].push(
                    Some(len_val),
                    MirOp::Call(
                        MirValue::Global("rt_list_len".to_string()),
                        vec![iter_val.clone()],
                        MirType { data_type: DataType::I64 },
                    ),
                    loc,
                );
                let cond = self.new_temp();
                self.func.blocks[cond_block].push(
                    Some(cond),
                    MirOp::ICmp(MirCmp::Lt, MirValue::temp(idx_loaded), MirValue::temp(len_val)),
                    loc,
                );

                let body_block = self.new_block("for_body");
                self.current_block = body_block;

                // In body: variable = iter[idx]
                let elem_ptr = self.new_temp();
                let elem_idx_i64 = self.new_temp();
                let elem_offset_i64 = self.new_temp();
                let elem_idx = self.new_temp();
                self.func.blocks[body_block].push(
                    Some(elem_idx_i64),
                    MirOp::Load(MirValue::temp(idx_ptr), MirType { data_type: DataType::I64 }),
                    loc,
                );
                // List layout: [len, elem0, elem1, ...]; elements start at offset 1.
                self.func.blocks[body_block].push(
                    Some(elem_offset_i64),
                    MirOp::Add(MirValue::temp(elem_idx_i64), MirValue::Const(MirConst::Int(1))),
                    loc,
                );
                self.func.blocks[body_block].push(
                    Some(elem_idx),
                    MirOp::Trunc(
                        MirValue::temp(elem_offset_i64),
                        MirType { data_type: DataType::I32 },
                    ),
                    loc,
                );
                self.func.blocks[body_block].push(
                    Some(elem_ptr),
                    MirOp::Gep(iter_val.clone(), vec![MirValue::temp(elem_idx)], "ptr".to_string()),
                    loc,
                );
                let elem_val = self.new_temp();
                self.func.blocks[body_block].push(
                    Some(elem_val),
                    MirOp::Load(MirValue::temp(elem_ptr), MirType { data_type: DataType::Unknown }),
                    loc,
                );

                // Alloca and store the loop variable
                let var_ptr = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(var_ptr),
                    MirOp::Alloca(MirType { data_type: DataType::Unknown }),
                    loc,
                );
                self.vars.insert(variable.clone(), var_ptr);
                self.var_types.insert(variable.clone(), DataType::Unknown);
                self.func.blocks[self.current_block].push(
                    None,
                    MirOp::Store(MirValue::temp(var_ptr), MirValue::temp(elem_val)),
                    loc,
                );

                // Register index variable if provided
                if let Some(idx_name) = index {
                    self.vars.insert(idx_name.clone(), idx_ptr);
                    self.var_types.insert(idx_name.clone(), DataType::I64);
                }

                for stmt in body {
                    self.lower_statement(stmt);
                }

                // Only add increment and back-edge if body didn't return/break
                let last_body = self.current_block;
                let body_terminated = !matches!(self.func.blocks[last_body].terminator, MirTerminator::Unreachable);
                if !body_terminated {
                    let old_idx = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(old_idx),
                        MirOp::Load(MirValue::temp(idx_ptr), MirType { data_type: DataType::I64 }),
                        loc,
                    );
                    let new_idx = self.new_temp();
                    self.func.blocks[self.current_block].push(
                        Some(new_idx),
                        MirOp::Add(MirValue::temp(old_idx), MirValue::Const(MirConst::Int(1))),
                        loc,
                    );
                    self.func.blocks[self.current_block].push(
                        None,
                        MirOp::Store(MirValue::temp(idx_ptr), MirValue::temp(new_idx)),
                        loc,
                    );

                    // Back edge: body → cond
                    self.func.blocks[last_body].terminator = MirTerminator::Br(cond_block);
                }

                // Create end_block and fix up cond_block terminator
                let end_block = self.new_block("for_end");
                self.func.blocks[cond_block].terminator =
                    MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);
                self.func.blocks[pre_for_block].terminator = MirTerminator::Br(cond_block);
                self.current_block = end_block;
            }
            Statement::Unsafe { body } => {
                for stmt in body {
                    self.lower_statement(stmt);
                }
            }
            _ => {}
        }
    }

    fn lower_expression(&mut self, expr: &Expression) -> MirValue {
        let loc = (0, 0);
        match expr {
            Expression::Literal(lit) => self.lower_literal(lit),
            Expression::Identifier(id) => {
                if let Some(&ptr) = self.vars.get(&id.name) {
                    let ty = self.var_types.get(&id.name).cloned().unwrap_or(DataType::Unknown);
                    // Don't load aggregate types — keep pointer for GEP
                    if matches!(&ty, DataType::Array { .. }) {
                        return MirValue::temp(ptr);
                    }
                    let loaded = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(loaded),
                        MirOp::Load(
                            MirValue::temp(ptr),
                            MirType { data_type: ty.clone() },
                        ),
                        loc,
                    );
                    let loaded_val = MirValue::temp(loaded);
                    if id.data_type != DataType::Unknown && id.data_type != ty {
                        return self.emit_convert(loaded_val, &ty, &id.data_type, loc);
                    }
                    loaded_val
                } else {
                    MirValue::Global(id.name.clone())
                }
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                ..
            } => {
                let l = self.lower_expression(left);
                let r = self.lower_expression(right);
                let result = self.new_temp();
                let mir_op = match operator.as_str() {
                    "+" => MirOp::Add(l, r),
                    "-" => MirOp::Sub(l, r),
                    "*" => MirOp::Mul(l, r),
                    "/" => MirOp::SDiv(l, r),
                    "%" => MirOp::SRem(l, r),
                    "==" => MirOp::ICmp(MirCmp::Eq, l, r),
                    "!=" => MirOp::ICmp(MirCmp::Ne, l, r),
                    "<" => MirOp::ICmp(MirCmp::Lt, l, r),
                    "<=" => MirOp::ICmp(MirCmp::Le, l, r),
                    ">" => MirOp::ICmp(MirCmp::Gt, l, r),
                    ">=" => MirOp::ICmp(MirCmp::Ge, l, r),
                    "&&" => MirOp::And(l, r),
                    "||" => MirOp::Or(l, r),
                    _ => MirOp::Add(l, r),
                };
                let last = self.current_block;
                self.func.blocks[last].push(Some(result), mir_op, loc);
                MirValue::temp(result)
            }
            Expression::Call { name, args, data_type, .. } if name == "__if_expr" && args.len() == 3 => {
                let cond = self.lower_expression(&args[0]);
                let then_expr = Self::extract_closure_expr(&args[1]);
                let else_expr = Self::extract_closure_expr(&args[2]);
                let then_val = self.lower_expression(then_expr);
                let else_val = self.lower_expression(else_expr);
                let result = self.new_temp();
                let ret_ty = MirType { data_type: data_type.clone() };

                let pre_ifexpr = self.current_block;
                self.func.blocks[pre_ifexpr].push(
                    Some(result),
                    MirOp::Alloca(ret_ty.clone()),
                    loc,
                );

                let then_block = self.new_block("ifexpr_then");
                let else_block = self.new_block("ifexpr_else");
                let end_block = self.new_block("ifexpr_end");

                self.func.blocks[pre_ifexpr].terminator =
                    MirTerminator::BrCond(cond, then_block, else_block);

                self.func.blocks[then_block].push(
                    None,
                    MirOp::Store(MirValue::temp(result), then_val),
                    loc,
                );
                self.func.blocks[then_block].terminator =
                    MirTerminator::Br(end_block);

                self.func.blocks[else_block].push(
                    None,
                    MirOp::Store(MirValue::temp(result), else_val),
                    loc,
                );
                self.func.blocks[else_block].terminator =
                    MirTerminator::Br(end_block);

                let loaded = self.new_temp();
                self.func.blocks[end_block].push(
                    Some(loaded),
                    MirOp::Load(MirValue::temp(result), ret_ty),
                    loc,
                );
                self.current_block = end_block;
                MirValue::temp(loaded)
            }
            Expression::Call {
                name,
                args,
                ..
            } if name == "__type_matches" => {
                MirValue::Const(MirConst::Bool(true))
            }
            Expression::Call {
                name,
                args,
                ..
            } if name == "range" => {
                let mir_args: Vec<MirValue> =
                    args.iter().map(|a| self.lower_expression(a)).collect();
                let result = self.new_temp();
                let last = self.current_block;
                self.func.blocks[last].push(
                    Some(result),
                MirOp::Call(
                    MirValue::Global("rt_math_range_i64".to_string()),
                    mir_args,
                    MirType { data_type: DataType::Unknown },
                ),
                    loc,
                );
                MirValue::temp(result)
            }
            Expression::Call {
                name,
                args,
                ..
            } if name == "len" && !args.is_empty() => {
                let arg_val = self.lower_expression(&args[0]);
                let arg_type = extract_data_type(&args[0]);
                let rt_name = match arg_type {
                    DataType::Str | DataType::Ref { .. } | DataType::RefMut { .. } => "rt_strings_len",
                    DataType::Vector { .. } | DataType::List => "rt_list_len",
                    DataType::Map { .. } | DataType::Dict => "rt_dicts_len",
                    _ => "rt_list_len",
                };
                let result = self.new_temp();
                let last = self.current_block;
                self.func.blocks[last].push(
                    Some(result),
                    MirOp::Call(
                        MirValue::Global(rt_name.to_string()),
                        vec![arg_val],
                        MirType { data_type: DataType::I64 },
                    ),
                    loc,
                );
                MirValue::temp(result)
            }
            Expression::Call { name, args, .. } if name == "lists.map" && args.len() == 2 => {
                self.lower_lists_map(args)
            }
            Expression::Call { name, args, .. } if name == "lists.filter" && args.len() == 2 => {
                self.lower_lists_filter(args)
            }
            Expression::Call { name, args, .. } if name == "lists.fold" && args.len() == 3 => {
                self.lower_lists_fold(args)
            }
            Expression::Call {
                name,
                args,
                data_type,
                ..
            } => {
                let is_instance_method = name.contains('.') && !name.contains("::")
                    && name.split_once('.').map(|(prefix, _method)|
                        self.var_types.contains_key(prefix)
                    ).unwrap_or(false);
                let (resolved_name, mir_args) = if is_instance_method {
                    let (prefix, method) = name.split_once('.').unwrap();
                    let var_ty = self.var_types.get(prefix).unwrap().clone();
                    let struct_name = match &var_ty {
                        DataType::StructNamed(s) => s.clone(),
                        _ => String::new(),
                    };
                    let norm = struct_name.split_once('[')
                        .map(|(b, _)| b.to_string())
                        .unwrap_or(struct_name);
                    let qualified = self.method_map.get(&norm)
                        .and_then(|methods| methods.get(method))
                        .cloned();
                    match qualified {
                        Some(qn) => {
                            let receiver = self.lower_expression(
                                &Expression::Identifier(
                                    crate::parser::ast::Identifier {
                                        name: prefix.to_string(),
                                        data_type: DataType::Unknown,
                                        line: 0,
                                        column: 0,
                                    }
                                )
                            );
                            let mut instance_args = vec![receiver];
                            instance_args.extend(self.lower_call_args(name, args));
                            (qn, instance_args)
                        }
                        None => {
                            let mir_args = self.lower_call_args(name, args);
                            let resolved = self
                                .bare_to_qualified
                                .get(name.as_str())
                                .cloned()
                                .unwrap_or_else(|| name.clone());
                            (resolved, mir_args)
                        }
                    }
                } else {
                    let mir_args = self.lower_call_args(name, args);
                    let resolved = self
                        .bare_to_qualified
                        .get(name.as_str())
                        .cloned()
                        .unwrap_or_else(|| name.clone());
                    (resolved, mir_args)
                };
                let last = self.current_block;
                if matches!(data_type, DataType::None) {
                    self.func.blocks[last].push(
                        None,
                        MirOp::Call(
                            MirValue::FunctionRef { name: resolved_name, env: Box::new(MirValue::Const(MirConst::None)) },
                            mir_args,
                            MirType {
                                data_type: data_type.clone(),
                            },
                        ),
                        loc,
                    );
                    MirValue::Const(MirConst::None)
                } else {
                    let result = self.new_temp();
                    self.func.blocks[last].push(
                        Some(result),
                        MirOp::Call(
                            MirValue::FunctionRef { name: resolved_name, env: Box::new(MirValue::Const(MirConst::None)) },
                            mir_args,
                            MirType {
                                data_type: data_type.clone(),
                            },
                        ),
                        loc,
                    );
                    MirValue::temp(result)
                }
            }
            Expression::Closure {
                params,
                body,
                return_type,
                capture,
            } => {
                self.lower_capturing_closure(params, body, return_type, capture)
            }
            Expression::Match {
                value,
                cases,
                default,
                data_type,
                ..
            } => {
                let match_val = self.lower_expression(value);
                let result_ptr = self.new_temp();
                let result_type = MirType {
                    data_type: data_type.clone(),
                };
                let initial_block = self.current_block;

                self.func.blocks[initial_block].push(
                    Some(result_ptr),
                    MirOp::Alloca(result_type.clone()),
                    loc,
                );

                let n = cases.len();
                // Create all chk blocks first so we know their indices
                let mut chk_blocks = Vec::with_capacity(n);
                for i in 0..n {
                    chk_blocks.push(self.new_block(&format!("match_chk_{}", i)));
                }

                // Fill in the chk blocks with pattern comparisons
                // Default block doesn't exist yet but we know its future index:
                // after all n chk blocks, we'll create n case blocks, then default, then end
                let first_chk = chk_blocks[0];
                let chk_base = first_chk;
                let case_base = first_chk + n;
                let default_idx = case_base + n;
                let end_idx = default_idx + 1;

                self.func.blocks[initial_block].terminator = MirTerminator::Br(first_chk);

                for (i, (pattern, _body)) in cases.iter().enumerate() {
                    let chk = chk_blocks[i];
                    let cs = case_base + i;
                    let next = if i + 1 < n { chk_base + i + 1 } else { default_idx };

                    match pattern {
                        Expression::Literal(lit) => {
                            let lit_val = self.lower_literal(lit);
                            let cmp = self.new_temp();
                            self.func.blocks[chk].push(
                                Some(cmp),
                                MirOp::ICmp(MirCmp::Eq, match_val.clone(), lit_val),
                                loc,
                            );
                            self.func.blocks[chk].terminator =
                                MirTerminator::BrCond(MirValue::temp(cmp), cs, next);
                        }
                        Expression::EnumVariant {
                            enum_name,
                            variant_name,
                            ..
                        }
                        | Expression::EnumVariantPath {
                            enum_name,
                            variant_name,
                            ..
                        } => {
                            let discriminant = self
                                .enum_types
                                .get(enum_name)
                                .and_then(|variants| {
                                    variants
                                        .iter()
                                        .find(|(n, _)| n == variant_name)
                                        .map(|(_, idx)| *idx as i64)
                                })
                                .unwrap_or(0);
                            let cmp = self.new_temp();
                            self.func.blocks[chk].push(
                                Some(cmp),
                                MirOp::ICmp(
                                    MirCmp::Eq,
                                    match_val.clone(),
                                    MirValue::Const(MirConst::Int(discriminant)),
                                ),
                                loc,
                            );
                            self.func.blocks[chk].terminator =
                                MirTerminator::BrCond(MirValue::temp(cmp), cs, next);
                        }
                        _ => {
                            self.func.blocks[chk].terminator =
                                MirTerminator::Br(cs);
                        }
                    }
                }

                // Lower each case body into its case block
                for (i, (_pattern, body)) in cases.iter().enumerate() {
                    let cs = self.new_block(&format!("match_case_{}", i));
                    self.current_block = cs;
                    let body_val = self.lower_expression(body);
                    self.func.blocks[self.current_block]
                        .push(None, MirOp::Store(MirValue::temp(result_ptr), body_val), loc);
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(end_idx);
                }

                // Default block — create and lower default body
                let default_block = self.new_block("match_default");
                self.current_block = default_block;
                {
                    let default_val = self.lower_expression(default);
                    self.func.blocks[self.current_block]
                        .push(None, MirOp::Store(MirValue::temp(result_ptr), default_val), loc);
                    self.func.blocks[self.current_block].terminator = MirTerminator::Br(end_idx);
                }

                // End block — load result
                let end_block = self.new_block("match_end");
                self.current_block = end_block;
                let loaded = self.new_temp();
                self.func.blocks[self.current_block].push(
                    Some(loaded),
                    MirOp::Load(MirValue::temp(result_ptr), result_type),
                    loc,
                );
                MirValue::temp(loaded)
            }
            Expression::NamedArg { value, .. } => {
                self.lower_expression(value)
            }
            Expression::MemberAccess { target, member, data_type } => {
                let struct_name = self.get_struct_name(target);
                if let Some(struct_name) = struct_name {
                    let norm_name = struct_name
                        .split_once('[')
                        .map(|(base, _)| base.to_string())
                        .unwrap_or_else(|| struct_name.clone());
                    if let Some(fields) = self.struct_types.get(&norm_name) {
                        if let Some(field_index) =
                            fields.iter().position(|(name, _)| name == member)
                        {
                            let actual_field_type = fields[field_index].1.clone();
                            let target_val = self.lower_expression(target);
                            let last = self.current_block;
                            let gep_result = self.new_temp();
                            self.func.blocks[last].push(
                                Some(gep_result),
                                MirOp::Gep(
                                    target_val,
                                    vec![
                                        MirValue::Const(MirConst::Int(0)),
                                        MirValue::Const(MirConst::Int(field_index as i64)),
                                    ],
                                    norm_name.clone(),
                                ),
                                loc,
                            );
                            if matches!(actual_field_type, DataType::Array { .. }) {
                                return MirValue::temp(gep_result);
                            }
                            let load_result = self.new_temp();
                            self.func.blocks[last].push(
                                Some(load_result),
                                MirOp::Load(
                                    MirValue::temp(gep_result),
                                    MirType { data_type: actual_field_type.clone() },
                                ),
                                loc,
                            );
                            if *data_type != actual_field_type {
                                return self.emit_convert(MirValue::temp(load_result), &actual_field_type, data_type, loc);
                            }
                            return MirValue::temp(load_result);
                        }
                    }
                }
                MirValue::Const(MirConst::None)
            }
            Expression::Tuple { elements, data_type } => {
                match data_type {
                    DataType::StructNamed(name) => {
                        let mir_args: Vec<MirValue> =
                            elements.iter().map(|e| self.lower_expression(e)).collect();
                        let result = self.new_temp();
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            Some(result),
                            MirOp::Call(
                                MirValue::FunctionRef { name: name.clone(), env: Box::new(MirValue::Const(MirConst::None)) },
                                mir_args,
                                MirType {
                                    data_type: data_type.clone(),
                                },
                            ),
                            loc,
                        );
                        MirValue::temp(result)
                    }
                    _ => MirValue::Const(MirConst::None),
                }
            }

            Expression::Index { target, index, data_type } => {
                let target_val = self.lower_expression(target);
                let index_val = self.lower_expression(index);
                let target_type = extract_data_type(target);
                let last = self.current_block;

                // Runtime bounds check before indexing.
                match &target_type {
                    DataType::Array { size, .. } => {
                        self.func.blocks[last].push(
                            None,
                            MirOp::Call(
                                MirValue::Global("rt_check_bounds_i64".to_string()),
                                vec![
                                    index_val.clone(),
                                    MirValue::Const(MirConst::Int(*size as i64)),
                                ],
                                MirType { data_type: DataType::None },
                            ),
                            loc,
                        );
                    }
                    DataType::Vector { .. } | DataType::List => {
                        let len_val = self.new_temp();
                        self.func.blocks[last].push(
                            Some(len_val),
                            MirOp::Call(
                                MirValue::Global("rt_list_len".to_string()),
                                vec![target_val.clone()],
                                MirType { data_type: DataType::I64 },
                            ),
                            loc,
                        );
                        self.func.blocks[last].push(
                            None,
                            MirOp::Call(
                                MirValue::Global("rt_check_bounds_i64".to_string()),
                                vec![index_val.clone(), MirValue::temp(len_val)],
                                MirType { data_type: DataType::None },
                            ),
                            loc,
                        );
                    }
                    _ => {}
                }

                let gep = self.new_temp();
                let elem_llvm = llvm_elem_type_str(data_type);
                self.func.blocks[last].push(
                    Some(gep),
                    MirOp::Gep(target_val, vec![index_val], elem_llvm),
                    loc,
                );
                let loaded = self.new_temp();
                self.func.blocks[last].push(
                    Some(loaded),
                    MirOp::Load(
                        MirValue::temp(gep),
                        MirType { data_type: data_type.clone() },
                    ),
                    loc,
                );
                MirValue::temp(loaded)
            }
            Expression::Reference { expr, .. } => {
                if let Expression::Identifier(id) = expr.as_ref() {
                    if let Some(&ptr) = self.vars.get(&id.name) {
                        MirValue::temp(ptr)
                    } else {
                        MirValue::Const(MirConst::None)
                    }
                } else {
                    // For non-trivial reference targets, lower the expr and return its ptr
                    // Currently unsupported, fallback
                    MirValue::Const(MirConst::None)
                }
            }
            Expression::Dereference { expr, data_type } => {
                let ptr_val = self.lower_expression(expr);
                let source_type = extract_data_type(expr);
                if is_trivial_deref(&source_type, data_type) {
                    ptr_val
                } else {
                    let loaded = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(loaded),
                        MirOp::Load(
                            ptr_val,
                            MirType { data_type: data_type.clone() },
                        ),
                        loc,
                    );
                    MirValue::temp(loaded)
                }
            }
            Expression::UnaryOp { operator, operand, .. } => {
                let op_val = self.lower_expression(operand);
                let result = self.new_temp();
                let last = self.current_block;
                match operator.as_str() {
                    "-" => {
                        let zero = MirValue::Const(MirConst::Int(0));
                        self.func.blocks[last].push(
                            Some(result),
                            MirOp::Sub(zero, op_val),
                            loc,
                        );
                    }
                    "!" => {
                        let zero = MirValue::Const(MirConst::Bool(false));
                        self.func.blocks[last].push(
                            Some(result),
                            MirOp::ICmp(MirCmp::Eq, op_val, zero),
                            loc,
                        );
                    }
                    _ => {}
                }
                MirValue::temp(result)
            }
            Expression::List { elements, element_type: _, data_type } => {
                match data_type {
                    DataType::Array { element_type, .. } => {
                        let last = self.current_block;
                        let arr_ptr = self.new_temp();
                        self.func.blocks[last].push(
                            Some(arr_ptr),
                            MirOp::Alloca(MirType { data_type: data_type.clone() }),
                            loc,
                        );
                        let elem_llvm = llvm_elem_type_str(element_type);
                        for (i, elem) in elements.iter().enumerate() {
                            let elem_val = self.lower_expression(elem);
                            let last = self.current_block;
                            let gep = self.new_temp();
                            self.func.blocks[last].push(
                                Some(gep),
                                MirOp::Gep(
                                    MirValue::temp(arr_ptr),
                                    vec![MirValue::Const(MirConst::Int(i as i64))],
                                    elem_llvm.clone(),
                                ),
                                loc,
                            );
                            self.func.blocks[last].push(
                                None,
                                MirOp::Store(MirValue::temp(gep), elem_val),
                                loc,
                            );
                        }
                        MirValue::temp(arr_ptr)
                    }
                    DataType::Vector { element_type, .. } => {
                        let last = self.current_block;
                        let list_ptr = self.new_temp();
                        self.func.blocks[last].push(
                            Some(list_ptr),
                            MirOp::Alloca(MirType { data_type: DataType::Unknown }),
                            loc,
                        );
                        let init = self.new_temp();
                        self.func.blocks[last].push(
                            Some(init),
                            MirOp::Call(
                                MirValue::Global("rt_list_create".to_string()),
                                vec![
                                    MirValue::Const(MirConst::Int(4)),
                                    MirValue::Const(MirConst::Int(8)),
                                ],
                                MirType { data_type: DataType::Unknown },
                            ),
                            loc,
                        );
                        let last = self.current_block;
                        self.func.blocks[last].push(
                            None,
                            MirOp::Store(MirValue::temp(list_ptr), MirValue::temp(init)),
                            loc,
                        );
                        let push_fn = match element_type.as_ref() {
                            DataType::I64 | DataType::U64 | DataType::Char => "rt_list_push_i64",
                            _ => "rt_list_push_ptr",
                        };
                        for elem in elements {
                            let elem_val = self.lower_expression(elem);
                            let last = self.current_block;
                            let loaded = self.new_temp();
                            self.func.blocks[last].push(
                                Some(loaded),
                                MirOp::Load(
                                    MirValue::temp(list_ptr),
                                    MirType { data_type: DataType::Unknown },
                                ),
                                loc,
                            );
                            let pushed = self.new_temp();
                            let last = self.current_block;
                            self.func.blocks[last].push(
                                Some(pushed),
                                MirOp::Call(
                                    MirValue::Global(push_fn.to_string()),
                                    vec![MirValue::temp(loaded), elem_val],
                                    MirType { data_type: DataType::Unknown },
                                ),
                                loc,
                            );
                            let last = self.current_block;
                            self.func.blocks[last].push(
                                None,
                                MirOp::Store(MirValue::temp(list_ptr), MirValue::temp(pushed)),
                                loc,
                            );
                        }
                        let last = self.current_block;
                        let final_list = self.new_temp();
                        self.func.blocks[last].push(
                            Some(final_list),
                            MirOp::Load(
                                MirValue::temp(list_ptr),
                                MirType { data_type: DataType::Unknown },
                            ),
                            loc,
                        );
                        MirValue::temp(final_list)
                    }
                    _ => MirValue::Const(MirConst::None),
                }
            }
            Expression::EnumVariantPath {
                enum_name,
                variant_name,
                ..
            } => {
                let discriminant = self
                    .enum_types
                    .get(enum_name)
                    .and_then(|variants| {
                        variants
                            .iter()
                            .find(|(n, _)| n == variant_name)
                            .map(|(_, idx)| *idx as i64)
                    })
                    .unwrap_or(0);
                MirValue::Const(MirConst::Int(discriminant))
            }
            Expression::EnumVariant {
                enum_name,
                variant_name,
                ..
            } => {
                let discriminant = self
                    .enum_types
                    .get(enum_name)
                    .and_then(|variants| {
                        variants
                            .iter()
                            .find(|(n, _)| n == variant_name)
                            .map(|(_, idx)| *idx as i64)
                    })
                    .unwrap_or(0);
                MirValue::Const(MirConst::Int(discriminant))
            }
            Expression::Dict { entries, data_type, .. } => {
                let vt = match data_type {
                    DataType::Map { value_type, .. } => value_type.as_ref(),
                    _ => &DataType::Unknown,
                };
                let last = self.current_block;
                let dict_ptr = self.new_temp();
                self.func.blocks[last].push(
                    Some(dict_ptr),
                    MirOp::Alloca(MirType { data_type: DataType::Unknown }),
                    loc,
                );
                let last = self.current_block;
                self.func.blocks[last].push(
                    None,
                    MirOp::Store(MirValue::temp(dict_ptr), MirValue::Const(MirConst::None)),
                    loc,
                );
                for (key_expr, val_expr) in entries {
                    let key_val = self.lower_expression(key_expr);
                    let val_val = self.lower_expression(val_expr);
                    let last = self.current_block;
                    let cur_dict = self.new_temp();
                    self.func.blocks[last].push(
                        Some(cur_dict),
                        MirOp::Load(
                            MirValue::temp(dict_ptr),
                            MirType { data_type: DataType::Unknown },
                        ),
                        loc,
                    );
                    let is_scalar = vt == &DataType::I64
                        || vt == &DataType::U64
                        || vt == &DataType::Char
                        || vt == &DataType::Bool
                        || vt == &DataType::I32
                        || vt == &DataType::U32;
                    let set_fn = if is_scalar {
                        "rt_dicts_set_i64"
                    } else {
                        "rt_dicts_set_with_kind"
                    };
                    let mut call_args = vec![MirValue::temp(cur_dict), key_val, val_val];
                    if !is_scalar {
                        call_args.push(MirValue::Const(MirConst::Int(data_type_to_kind(vt))));
                    }
                    let pushed = self.new_temp();
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        Some(pushed),
                            MirOp::Call(
                                MirValue::Global(set_fn.to_string()),
                                call_args,
                                MirType { data_type: DataType::Unknown },
                            ),
                        loc,
                    );
                    let last = self.current_block;
                    self.func.blocks[last].push(
                        None,
                        MirOp::Store(MirValue::temp(dict_ptr), MirValue::temp(pushed)),
                        loc,
                    );
                }
                let last = self.current_block;
                let final_dict = self.new_temp();
                self.func.blocks[last].push(
                    Some(final_dict),
                    MirOp::Load(
                        MirValue::temp(dict_ptr),
                        MirType { data_type: DataType::Unknown },
                    ),
                    loc,
                );
                MirValue::temp(final_dict)
            }
            _ => MirValue::Const(MirConst::None),
        }
    }

    fn lower_closure_function(
        &mut self,
        params: &[(String, DataType)],
        body: &[Statement],
        return_type: &DataType,
        captures: &[(String, DataType)],
        env_struct_name: &str,
    ) -> String {
        let name = format!("closure_{}", self.closure_counter);
        self.closure_counter += 1;
        let mir_params: Vec<MirParam> = params
            .iter()
            .map(|(n, t)| MirParam {
                name: n.clone(),
                data_type: t.clone(),
            })
            .collect();
        let mut lower = MirLower {
            func: MirFunction::new(name.clone(), mir_params, return_type.clone()),
            next_temp: 0,
            vars: HashMap::new(),
            var_types: HashMap::new(),
            struct_types: self.struct_types.clone(),
            enum_types: self.enum_types.clone(),
            bare_to_qualified: self.bare_to_qualified.clone(),
            method_map: self.method_map.clone(),
            current_block: 0,
            closure_functions: Vec::new(),
            closure_counter: 0,
        };
        lower.func.noinline = true;
        // Create the entry block and wire captures BEFORE lowering body,
        // so the body can reference them as local variables.
        let entry_id = lower.entry_block_id();
        for (idx, (cap_name, cap_type)) in captures.iter().enumerate() {
            let gep = lower.new_temp();
            lower.func.blocks[entry_id].push(
                Some(gep),
                MirOp::Gep(
                    MirValue::EnvPtr,
                    vec![
                        MirValue::Const(MirConst::Int(0)),
                        MirValue::Const(MirConst::Int(idx as i64)),
                    ],
                    env_struct_name.to_string(),
                ),
                (0, 0),
            );
            lower.vars.insert(cap_name.clone(), gep);
            lower.var_types.insert(cap_name.clone(), cap_type.clone());
        }
        lower.lower_function_body(body);
        lower.func.body_hash = lower.func.compute_hash();
        // Merge any synthetic struct types created by nested closures.
        self.struct_types.extend(lower.struct_types.drain());
        self.closure_functions.extend(lower.closure_functions);
        self.closure_functions.push(lower.func);
        name
    }

    fn lower_capturing_closure(
        &mut self,
        params: &[(String, DataType)],
        body: &[Statement],
        return_type: &DataType,
        captures: &[(String, DataType)],
    ) -> MirValue {
        let loc = (0, 0);
        eprintln!("LOWER CAPTURING CLOSURE captures={:?} return_type={:?}", captures, return_type);
        let env_struct_name = format!("closure_env_{}", self.closure_counter);
        let env_fields: Vec<(String, DataType)> = captures
            .iter()
            .map(|(n, t)| (format!("capture_{}", n), t.clone()))
            .collect();
        self.struct_types
            .insert(env_struct_name.clone(), env_fields);

        let name = self.lower_closure_function(
            params,
            body,
            return_type,
            captures,
            &env_struct_name,
        );

        // Allocate the environment struct on the heap.
        let env_size = (captures.len() as i64) * 8;
        let env_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(env_ptr),
            MirOp::Call(
                MirValue::Global("rt_closure_env_alloc".to_string()),
                vec![MirValue::Const(MirConst::Int(env_size))],
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );

        // Store each captured value into the corresponding field.
        for (idx, (cap_name, cap_type)) in captures.iter().enumerate() {
            let cap_val = self.load_variable(cap_name, cap_type);
            let gep = self.new_temp();
            self.func.blocks[self.current_block].push(
                Some(gep),
                MirOp::Gep(
                    MirValue::temp(env_ptr),
                    vec![
                        MirValue::Const(MirConst::Int(0)),
                        MirValue::Const(MirConst::Int(idx as i64)),
                    ],
                    env_struct_name.clone(),
                ),
                loc,
            );
            self.func.blocks[self.current_block].push(
                None,
                MirOp::Store(MirValue::temp(gep), cap_val),
                loc,
            );
        }

        MirValue::FunctionRef {
            name,
            env: Box::new(MirValue::temp(env_ptr)),
        }
    }

    fn load_variable(&mut self, name: &str, data_type: &DataType) -> MirValue {
        let ptr = self
            .vars
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("unknown captured variable '{}'", name));
        let tmp = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(tmp),
            MirOp::Load(
                MirValue::temp(ptr),
                MirType {
                    data_type: data_type.clone(),
                },
            ),
            (0, 0),
        );
        MirValue::temp(tmp)
    }

    fn extract_closure_expr(expr: &Expression) -> &Expression {
        if let Expression::Closure { body, .. } = expr {
            if let Some(Statement::Return(Some(inner))) = body.first() {
                return inner;
            }
        }
        expr
    }

    fn lower_lists_map(&mut self, args: &[Expression]) -> MirValue {
        let loc = (0, 0);
        let closure_val = self.lower_expression(&args[0]);
        let list_val = self.lower_expression(&args[1]);

        // result = rt_list_create(4, 8)
        let result_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(result_ptr),
            MirOp::Alloca(MirType { data_type: DataType::Unknown }),
            loc,
        );
        let init = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(init),
            MirOp::Call(
                MirValue::Global("rt_list_create".to_string()),
                vec![
                    MirValue::Const(MirConst::Int(4)),
                    MirValue::Const(MirConst::Int(8)),
                ],
                MirType { data_type: DataType::Unknown },
            ),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(init)),
            loc,
        );

        // i = 0
        let i_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(i_ptr),
            MirOp::Alloca(MirType { data_type: DataType::I64 }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::Const(MirConst::Int(0))),
            loc,
        );

        let pre_block = self.current_block;
        let cond_block = self.new_block("map_cond");
        let body_block = self.new_block("map_body");
        let end_block = self.new_block("map_end");
        self.func.blocks[pre_block].terminator = MirTerminator::Br(cond_block);

        // cond: i < len(list)
        self.current_block = cond_block;
        let i_loaded = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(i_loaded),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let len_val = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(len_val),
            MirOp::Call(
                MirValue::Global("rt_list_len".to_string()),
                vec![list_val.clone()],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        let cond = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(cond),
            MirOp::ICmp(MirCmp::Lt, MirValue::temp(i_loaded), MirValue::temp(len_val)),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        // body
        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let elem = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        let mapped = self.new_temp();
        self.func.blocks[body_block].push(
            Some(mapped),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(elem)],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        let loaded_result = self.new_temp();
        self.func.blocks[body_block].push(
            Some(loaded_result),
            MirOp::Load(MirValue::temp(result_ptr), MirType { data_type: DataType::Unknown }),
            loc,
        );
        let pushed = self.new_temp();
        self.func.blocks[body_block].push(
            Some(pushed),
            MirOp::Call(
                MirValue::Global("rt_list_push_i64".to_string()),
                vec![MirValue::temp(loaded_result), MirValue::temp(mapped)],
                MirType { data_type: DataType::Unknown },
            ),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(pushed)),
            loc,
        );
        // i = i + 1
        let old_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(old_i),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let new_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(new_i),
            MirOp::Add(MirValue::temp(old_i), MirValue::Const(MirConst::Int(1))),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::temp(new_i)),
            loc,
        );
        self.func.blocks[body_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = end_block;
        let final_result = self.new_temp();
        self.func.blocks[end_block].push(
            Some(final_result),
            MirOp::Load(MirValue::temp(result_ptr), MirType { data_type: DataType::Unknown }),
            loc,
        );
        MirValue::temp(final_result)
    }

    fn lower_lists_filter(&mut self, args: &[Expression]) -> MirValue {
        let loc = (0, 0);
        let closure_val = self.lower_expression(&args[0]);
        let list_val = self.lower_expression(&args[1]);

        let result_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(result_ptr),
            MirOp::Alloca(MirType { data_type: DataType::Unknown }),
            loc,
        );
        let init = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(init),
            MirOp::Call(
                MirValue::Global("rt_list_create".to_string()),
                vec![
                    MirValue::Const(MirConst::Int(4)),
                    MirValue::Const(MirConst::Int(8)),
                ],
                MirType { data_type: DataType::Unknown },
            ),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(init)),
            loc,
        );

        let i_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(i_ptr),
            MirOp::Alloca(MirType { data_type: DataType::I64 }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::Const(MirConst::Int(0))),
            loc,
        );
        let elem_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(elem_ptr),
            MirOp::Alloca(MirType { data_type: DataType::I64 }),
            loc,
        );

        let pre_block = self.current_block;
        let cond_block = self.new_block("filter_cond");
        let body_block = self.new_block("filter_body");
        let keep_block = self.new_block("filter_keep");
        let inc_block = self.new_block("filter_inc");
        let end_block = self.new_block("filter_end");
        self.func.blocks[pre_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = cond_block;
        let i_loaded = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(i_loaded),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let len_val = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(len_val),
            MirOp::Call(
                MirValue::Global("rt_list_len".to_string()),
                vec![list_val.clone()],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        let cond = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(cond),
            MirOp::ICmp(MirCmp::Lt, MirValue::temp(i_loaded), MirValue::temp(len_val)),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let elem_raw = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem_raw),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(elem_ptr), MirValue::temp(elem_raw)),
            loc,
        );
        let keep = self.new_temp();
        self.func.blocks[body_block].push(
            Some(keep),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(elem_raw)],
                MirType { data_type: DataType::Bool },
            ),
            loc,
        );
        self.func.blocks[body_block].terminator =
            MirTerminator::BrCond(MirValue::temp(keep), keep_block, inc_block);

        self.current_block = keep_block;
        let elem = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(elem),
            MirOp::Load(MirValue::temp(elem_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let loaded_result = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(loaded_result),
            MirOp::Load(MirValue::temp(result_ptr), MirType { data_type: DataType::Unknown }),
            loc,
        );
        let pushed = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(pushed),
            MirOp::Call(
                MirValue::Global("rt_list_push_i64".to_string()),
                vec![MirValue::temp(loaded_result), MirValue::temp(elem)],
                MirType { data_type: DataType::Unknown },
            ),
            loc,
        );
        self.func.blocks[keep_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(pushed)),
            loc,
        );
        self.func.blocks[keep_block].terminator = MirTerminator::Br(inc_block);

        self.current_block = inc_block;
        let old_i = self.new_temp();
        self.func.blocks[inc_block].push(
            Some(old_i),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let new_i = self.new_temp();
        self.func.blocks[inc_block].push(
            Some(new_i),
            MirOp::Add(MirValue::temp(old_i), MirValue::Const(MirConst::Int(1))),
            loc,
        );
        self.func.blocks[inc_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::temp(new_i)),
            loc,
        );
        self.func.blocks[inc_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = end_block;
        let final_result = self.new_temp();
        self.func.blocks[end_block].push(
            Some(final_result),
            MirOp::Load(MirValue::temp(result_ptr), MirType { data_type: DataType::Unknown }),
            loc,
        );
        MirValue::temp(final_result)
    }

    fn lower_lists_fold(&mut self, args: &[Expression]) -> MirValue {
        let loc = (0, 0);
        let acc_init = self.lower_expression(&args[0]);
        let closure_val = self.lower_expression(&args[1]);
        let list_val = self.lower_expression(&args[2]);

        let acc_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(acc_ptr),
            MirOp::Alloca(MirType { data_type: DataType::I64 }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(acc_ptr), acc_init),
            loc,
        );

        let i_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(i_ptr),
            MirOp::Alloca(MirType { data_type: DataType::I64 }),
            loc,
        );
        self.func.blocks[self.current_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::Const(MirConst::Int(0))),
            loc,
        );

        let pre_block = self.current_block;
        let cond_block = self.new_block("fold_cond");
        let body_block = self.new_block("fold_body");
        let end_block = self.new_block("fold_end");
        self.func.blocks[pre_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = cond_block;
        let i_loaded = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(i_loaded),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let len_val = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(len_val),
            MirOp::Call(
                MirValue::Global("rt_list_len".to_string()),
                vec![list_val.clone()],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        let cond = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(cond),
            MirOp::ICmp(MirCmp::Lt, MirValue::temp(i_loaded), MirValue::temp(len_val)),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let elem = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        let acc_loaded = self.new_temp();
        self.func.blocks[body_block].push(
            Some(acc_loaded),
            MirOp::Load(MirValue::temp(acc_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let new_acc = self.new_temp();
        self.func.blocks[body_block].push(
            Some(new_acc),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(acc_loaded), MirValue::temp(elem)],
                MirType { data_type: DataType::I64 },
            ),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(acc_ptr), MirValue::temp(new_acc)),
            loc,
        );
        let old_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(old_i),
            MirOp::Load(MirValue::temp(i_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        let new_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(new_i),
            MirOp::Add(MirValue::temp(old_i), MirValue::Const(MirConst::Int(1))),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(i_ptr), MirValue::temp(new_i)),
            loc,
        );
        self.func.blocks[body_block].terminator = MirTerminator::Br(cond_block);

        self.current_block = end_block;
        let final_result = self.new_temp();
        self.func.blocks[end_block].push(
            Some(final_result),
            MirOp::Load(MirValue::temp(acc_ptr), MirType { data_type: DataType::I64 }),
            loc,
        );
        MirValue::temp(final_result)
    }

    fn get_target_elem_type(&self, target: &Expression) -> String {
        if let Expression::Identifier(id) = target {
            if let Some(ty) = self.var_types.get(&id.name) {
                match ty {
                    DataType::Array { element_type, .. }
                    | DataType::Vector { element_type, .. }
                    | DataType::Slice { element_type, .. } => {
                        return llvm_elem_type_str(element_type);
                    }
                    _ => {}
                }
            }
        }
        "i64".to_string()
    }

    fn get_struct_name(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Identifier(id) => {
                self.var_types.get(&id.name).and_then(|t| match t {
                    DataType::StructNamed(name) => Some(name.clone()),
                    _ => None,
                })
            }
            _ => None,
        }
    }

    fn lower_literal(&self, lit: &Literal) -> MirValue {
        match lit {
            Literal::Int(v) => MirValue::Const(MirConst::Int(*v)),
            Literal::Float(v) => MirValue::Const(MirConst::Float(*v)),
            Literal::Bool(v) => MirValue::Const(MirConst::Bool(*v)),
            Literal::Char(v) => {
                MirValue::Const(MirConst::Char(char::from_u32(*v).unwrap_or('\0')))
            }
            Literal::Str(v) => MirValue::Const(MirConst::Str(v.clone())),
            Literal::None => MirValue::Const(MirConst::None),
            _ => MirValue::Const(MirConst::None),
        }
    }

    fn emit_convert(&mut self, src_val: MirValue, src_type: &DataType, target_type: &DataType, loc: (usize, usize)) -> MirValue {
        if src_type == target_type || *target_type == DataType::Unknown {
            return src_val;
        }
        let op = match (src_type, target_type) {
            (DataType::I64 | DataType::I32 | DataType::I16 | DataType::I8, DataType::F64 | DataType::F32) => {
                MirOp::Sitofp(src_val, MirType { data_type: target_type.clone() })
            }
            (DataType::F64 | DataType::F32, DataType::I64 | DataType::I32 | DataType::I16 | DataType::I8) => {
                MirOp::Fptosi(src_val, MirType { data_type: target_type.clone() })
            }
            _ => MirOp::Sitofp(src_val, MirType { data_type: target_type.clone() }),
        };
        let result = self.new_temp();
        let last = self.current_block;
        self.func.blocks[last].push(Some(result), op, loc);
        MirValue::temp(result)
    }
}

fn llvm_elem_type_str(dt: &DataType) -> String {
    match dt {
        DataType::I64 | DataType::Char | DataType::U64 => "i64".to_string(),
        DataType::I32 | DataType::U32 => "i32".to_string(),
        DataType::I16 | DataType::U16 => "i16".to_string(),
        DataType::I8 | DataType::U8 => "i8".to_string(),
        DataType::F32 => "float".to_string(),
        DataType::F64 => "double".to_string(),
        DataType::Bool => "i1".to_string(),
        DataType::None => "i64".to_string(),
        DataType::StructNamed(name) => format!("struct:{}", name),
        _ => "i64".to_string(),
    }
}
