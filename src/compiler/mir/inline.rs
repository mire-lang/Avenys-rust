use super::*;
use std::collections::HashSet;

fn callee_instr_count(func: &MirFunction) -> usize {
    func.blocks.iter().map(|b| b.insts.len()).sum()
}

fn has_call_to(func: &MirFunction, callee: &str) -> bool {
    func.blocks.iter().any(|b| b.insts.iter().any(|inst| matches!(&inst.op, MirOp::Call(MirValue::FunctionRef { name, .. }, _, _) if name == callee)))
}

fn max_temp_in_value(value: &MirValue, max: &mut usize) {
    if let MirValue::Temp(id) = value {
        *max = (*max).max(id + 1);
    }
}

fn max_temp_in_op(op: &MirOp, max: &mut usize) {
    match op {
        MirOp::Alloca(_) => {}
        MirOp::Load(v, _) | MirOp::PtrToInt(v, _) | MirOp::IntToPtr(v, _) | MirOp::BitCast(v, _) | MirOp::ZExt(v, _) | MirOp::Trunc(v, _) | MirOp::Sitofp(v, _) | MirOp::Fptosi(v, _) | MirOp::Copy(v) => {
            max_temp_in_value(v, max);
        }
        MirOp::Store(d, s)
        | MirOp::Add(d, s)
        | MirOp::Sub(d, s)
        | MirOp::Mul(d, s)
        | MirOp::SDiv(d, s)
        | MirOp::SRem(d, s)
        | MirOp::Shl(d, s)
        | MirOp::And(d, s)
        | MirOp::Or(d, s)
        | MirOp::ICmp(_, d, s)
        | MirOp::FCmp(_, d, s) => {
            max_temp_in_value(d, max);
            max_temp_in_value(s, max);
        }
        MirOp::Call(_, args, _) => {
            for arg in args {
                max_temp_in_value(arg, max);
            }
        }
        MirOp::Gep(base, args, _name) => {
            max_temp_in_value(base, max);
            for arg in args {
                max_temp_in_value(arg, max);
            }
        }
        MirOp::Phi(pairs, _) => {
            for (v, _) in pairs {
                max_temp_in_value(v, max);
            }
        }
        MirOp::Select(c, t, f) => {
            max_temp_in_value(c, max);
            max_temp_in_value(t, max);
            max_temp_in_value(f, max);
        }
    }
}

fn next_available_temp(func: &MirFunction) -> usize {
    let mut max = 0;
    for block in &func.blocks {
        for inst in &block.insts {
            if let Some(result) = inst.result {
                max = max.max(result + 1);
            }
            max_temp_in_op(&inst.op, &mut max);
        }
        match &block.terminator {
            MirTerminator::Br(_) => {}
            MirTerminator::BrCond(v, _, _) | MirTerminator::Ret(Some(v)) => {
                max_temp_in_value(v, &mut max);
            }
            MirTerminator::Ret(None) | MirTerminator::Unreachable => {}
        }
    }
    max
}

pub fn inlining(program: &mut MirProgram) -> usize {
    let mut total = 0;
    let mut tried: HashSet<usize> = HashSet::new();

    loop {
        let candidate = program.functions.iter().enumerate().find(|(i, f)| {
            !tried.contains(i) && f.name != "main" && !f.blocks.is_empty() && callee_instr_count(f) <= 5
        });
        let Some((callee_idx, _)) = candidate else { break };
        tried.insert(callee_idx);

        let callee_name = program.functions[callee_idx].name.clone();

        let caller_idxs: Vec<usize> = program
            .functions
            .iter()
            .enumerate()
            .filter(|(i, f)| *i != callee_idx && has_call_to(f, &callee_name))
            .map(|(i, _)| i)
            .collect();

        if caller_idxs.is_empty() {
            continue;
        }

        let callee = program.functions[callee_idx].clone();
        let mut all_inlined = true;

        for &cidx in &caller_idxs {
            if inline_into(&mut program.functions[cidx], &callee) {
                total += 1;
            } else {
                all_inlined = false;
            }
        }

        if all_inlined {
            program.functions.remove(callee_idx);
        }
    }

    total
}

fn inline_into(caller: &mut MirFunction, callee: &MirFunction) -> bool {
    if callee.blocks.len() != 1 {
        return false;
    }

    let callee_block = &callee.blocks[0];
    let MirTerminator::Ret(ret_val) = &callee_block.terminator else {
        return false;
    };

    let call_sites: Vec<(usize, usize)> = {
        let mut sites = Vec::new();
        for (bi, block) in caller.blocks.iter().enumerate() {
            for (ii, inst) in block.insts.iter().enumerate() {
                if let MirOp::Call(MirValue::FunctionRef { name, .. }, _, _) = &inst.op {
                    if name == &callee.name && ii + 1 == block.insts.len() {
                        sites.push((bi, ii));
                    }
                }
            }
        }
        sites
    };

    if call_sites.is_empty() {
        return false;
    }

    let mut changed = false;

    for (bi, ii) in call_sites.into_iter().rev() {
        let call_inst = caller.blocks[bi].insts[ii].clone();
        let MirOp::Call(_, args, _) = call_inst.op else {
            continue;
        };
        let call_result = call_inst.result;
        if call_result.is_some() && ret_val.is_none() {
            continue;
        }
        let old_term = caller.blocks[bi].terminator.clone();

        let temp_offset = next_available_temp(caller);
        let mut inserted: Vec<MirInst> = Vec::new();

        for inst in &callee_block.insts {
            let new_result = inst.result.map(|r| r + temp_offset);
            let new_op = remap_op(&inst.op, temp_offset, callee, &args);
            inserted.push(MirInst {
                result: new_result,
                op: new_op,
                loc: inst.loc,
            });
        }

        if let (Some(result_temp), Some(val)) = (call_result, ret_val.as_ref()) {
            inserted.push(MirInst {
                result: Some(result_temp),
                op: MirOp::Copy(remap_val(val, temp_offset, callee, &args)),
                loc: call_inst.loc,
            });
        }

        let cont_id = caller.blocks.len();
        caller.blocks.push(MirBlock {
            id: cont_id,
            label: format!("{}_cont", caller.blocks[bi].label),
            insts: Vec::new(),
            terminator: old_term,
        });
        caller.blocks[bi].terminator = MirTerminator::Br(cont_id);
        caller.blocks[bi].insts.splice(ii..=ii, inserted);
        changed = true;
    }

    changed
}

fn remap_op(op: &MirOp, temp_offset: usize, callee: &MirFunction, args: &[MirValue]) -> MirOp {
    let map = |v: &MirValue| remap_val(v, temp_offset, callee, args);
    match op {
        MirOp::Call(n, a, t) => MirOp::Call(n.clone(), a.iter().map(map).collect(), t.clone()),
        MirOp::Alloca(t) => MirOp::Alloca(t.clone()),
        MirOp::Load(v, t) => MirOp::Load(map(v), t.clone()),
        MirOp::Store(d, s) => MirOp::Store(map(d), map(s)),
        MirOp::Add(l, r) => MirOp::Add(map(l), map(r)),
        MirOp::Sub(l, r) => MirOp::Sub(map(l), map(r)),
        MirOp::Mul(l, r) => MirOp::Mul(map(l), map(r)),
        MirOp::SDiv(l, r) => MirOp::SDiv(map(l), map(r)),
        MirOp::SRem(l, r) => MirOp::SRem(map(l), map(r)),
        MirOp::Shl(l, r) => MirOp::Shl(map(l), map(r)),
        MirOp::And(l, r) => MirOp::And(map(l), map(r)),
        MirOp::Or(l, r) => MirOp::Or(map(l), map(r)),
        MirOp::ICmp(c, l, r) => MirOp::ICmp(c.clone(), map(l), map(r)),
        MirOp::FCmp(c, l, r) => MirOp::FCmp(c.clone(), map(l), map(r)),
        MirOp::Gep(v, i, n) => MirOp::Gep(map(v), i.iter().map(|x| map(x)).collect(), n.clone()),
        MirOp::PtrToInt(v, t) => MirOp::PtrToInt(map(v), t.clone()),
        MirOp::IntToPtr(v, t) => MirOp::IntToPtr(map(v), t.clone()),
        MirOp::BitCast(v, t) => MirOp::BitCast(map(v), t.clone()),
        MirOp::ZExt(v, t) => MirOp::ZExt(map(v), t.clone()),
        MirOp::Trunc(v, t) => MirOp::Trunc(map(v), t.clone()),
        MirOp::Sitofp(v, t) => MirOp::Sitofp(map(v), t.clone()),
        MirOp::Fptosi(v, t) => MirOp::Fptosi(map(v), t.clone()),
        MirOp::Phi(p, t) => MirOp::Phi(p.iter().map(|(v, b)| (map(v), *b)).collect(), t.clone()),
        MirOp::Select(c, t, f) => MirOp::Select(map(c), map(t), map(f)),
        MirOp::Copy(v) => MirOp::Copy(map(v)),
    }
}

fn remap_val(val: &MirValue, temp_offset: usize, callee: &MirFunction, args: &[MirValue]) -> MirValue {
    match val {
        MirValue::Temp(id) => MirValue::Temp(*id + temp_offset),
        MirValue::Param(pname) => {
            callee.params.iter().position(|p| p.name == *pname)
                .and_then(|pi| args.get(pi).cloned())
                .unwrap_or_else(|| MirValue::Global(pname.clone()))
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::DataType;

    fn make_func(name: &str, blocks: Vec<MirBlock>) -> MirFunction {
        let mut f = MirFunction {
            name: name.to_string(),
            params: vec![],
            ret_type: DataType::I64,
            blocks,
            body_hash: 0,
            noinline: false,
        };
        f.body_hash = f.compute_hash();
        f
    }

    fn block(id: usize, insts: Vec<MirInst>, term: MirTerminator) -> MirBlock {
        MirBlock { id, label: format!("b{id}"), insts, terminator: term }
    }

    fn inst(result: usize, op: MirOp) -> MirInst {
        MirInst { result: Some(result), op, loc: (0, 0) }
    }

    #[test]
    fn inline_simple_noop() {
        // callee: returns constant 42
        let callee = make_func("add42", vec![
            block(0, vec![inst(0, MirOp::Add(MirValue::Const(MirConst::Int(40)), MirValue::Const(MirConst::Int(2))))], MirTerminator::Ret(Some(MirValue::Temp(0)))),
        ]);
        // caller: calls add42
        let caller = make_func("main", vec![
            block(0, vec![inst(0, MirOp::Call(MirValue::FunctionRef { name: "add42".into(), env: Box::new(MirValue::Const(MirConst::None)) }, vec![], MirType { data_type: DataType::I64 }))], MirTerminator::Ret(Some(MirValue::Temp(0))))
        ]);
        let mut prog = MirProgram { functions: vec![caller, callee], entry_point: None, extern_functions: vec![], struct_types: HashMap::new() };
        let count = inlining(&mut prog);
        assert_eq!(count, 1, "should have inlined add42");
        assert_eq!(prog.functions.len(), 1, "callee should be removed");
        assert_eq!(prog.functions[0].name, "main");
        // main should have more blocks than before (callee blocks inlined)
        assert!(prog.functions[0].blocks.len() > 1, "should have inlined blocks");
    }

    #[test]
    fn inline_no_callers_untouched() {
        let callee = make_func("helper", vec![
            block(0, vec![], MirTerminator::Ret(Some(MirValue::Const(MirConst::Int(1))))),
        ]);
        let caller = make_func("main", vec![
            block(0, vec![], MirTerminator::Ret(Some(MirValue::Const(MirConst::Int(0))))),
        ]);
        let mut prog = MirProgram { functions: vec![caller, callee], entry_point: None, extern_functions: vec![], struct_types: HashMap::new() };
        let count = inlining(&mut prog);
        assert_eq!(count, 0, "no inlining should happen");
        assert_eq!(prog.functions.len(), 2, "both functions remain");
    }

    #[test]
    fn inline_large_callee_unchanged() {
        // callee has 6 instructions (> 5 threshold)
        let callee = make_func("big", vec![
            block(0, vec![
                inst(0, MirOp::Add(MirValue::Const(MirConst::Int(1)), MirValue::Const(MirConst::Int(2)))),
                inst(1, MirOp::Add(MirValue::Temp(0), MirValue::Const(MirConst::Int(3)))),
                inst(2, MirOp::Add(MirValue::Temp(1), MirValue::Const(MirConst::Int(4)))),
                inst(3, MirOp::Add(MirValue::Temp(2), MirValue::Const(MirConst::Int(5)))),
                inst(4, MirOp::Add(MirValue::Temp(3), MirValue::Const(MirConst::Int(6)))),
                inst(5, MirOp::Add(MirValue::Temp(4), MirValue::Const(MirConst::Int(7)))),
            ], MirTerminator::Ret(Some(MirValue::Temp(5)))),
        ]);
        let caller = make_func("main", vec![
            block(0, vec![inst(0, MirOp::Call(MirValue::FunctionRef { name: "big".into(), env: Box::new(MirValue::Const(MirConst::None)) }, vec![], MirType { data_type: DataType::I64 }))], MirTerminator::Ret(Some(MirValue::Temp(0))))
        ]);
        let mut prog = MirProgram { functions: vec![caller, callee], entry_point: None, extern_functions: vec![], struct_types: HashMap::new() };
        let count = inlining(&mut prog);
        assert_eq!(count, 0, "big function should not be inlined");
        assert_eq!(prog.functions.len(), 2);
    }
}
