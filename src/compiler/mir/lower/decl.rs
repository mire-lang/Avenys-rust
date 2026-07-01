use super::MirLower;
use crate::compiler::mir::*;
use crate::parser::ast::{DataType, Statement};
use std::collections::HashMap;

impl MirLower {
    pub(crate) fn lower_function_body(&mut self, body: &[Statement]) {
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

    pub(crate) fn lower_closure_function(
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
        self.struct_types.extend(lower.struct_types.drain());
        self.closure_functions.extend(lower.closure_functions);
        self.closure_functions.push(lower.func);
        name
    }

    pub(crate) fn lower_capturing_closure(
        &mut self,
        params: &[(String, DataType)],
        body: &[Statement],
        return_type: &DataType,
        captures: &[(String, DataType)],
    ) -> MirValue {
        let loc = (0, 0);
        let env_struct_name = format!("closure_env_{}", self.closure_counter);
        let env_fields: Vec<(String, DataType)> = captures
            .iter()
            .map(|(n, t)| (format!("capture_{}", n), t.clone()))
            .collect();
        self.struct_types
            .insert(env_struct_name.clone(), env_fields);

        let name =
            self.lower_closure_function(params, body, return_type, captures, &env_struct_name);

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

    pub(crate) fn load_variable(&mut self, name: &str, data_type: &DataType) -> MirValue {
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
}
