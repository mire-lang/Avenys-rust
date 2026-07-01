use super::*;
use std::collections::HashMap;

pub(super) fn algebraic_simplify(func: &mut MirFunction) -> usize {
    let mut count = 0;
    for block in &mut func.blocks {
        let mut i = 0;
        while i < block.insts.len() {
            let simplified = try_simplify(&block.insts[i]);
            if let Some(simple_op) = simplified {
                block.insts[i].op = simple_op;
                count += 1;
            }
            i += 1;
        }
    }
    count
}

fn try_simplify(inst: &MirInst) -> Option<MirOp> {
    use MirOp::*;
    match &inst.op {
        Add(l, r) => match (l, r) {
            (_, MirValue::Const(MirConst::Int(0))) => Some(Copy(l.clone())),
            (MirValue::Const(MirConst::Int(0)), _) => Some(Copy(r.clone())),
            _ => None,
        },
        Sub(l, r) => match (l, r) {
            (_, MirValue::Const(MirConst::Int(0))) => Some(Copy(l.clone())),
            (MirValue::Temp(a), MirValue::Temp(b)) if a == b => {
                Some(Copy(MirValue::Const(MirConst::Int(0))))
            }
            _ => None,
        },
        Mul(l, r) => match (l, r) {
            (_, MirValue::Const(MirConst::Int(1))) => Some(Copy(l.clone())),
            (MirValue::Const(MirConst::Int(1)), _) => Some(Copy(r.clone())),
            (_, MirValue::Const(MirConst::Int(0))) => Some(Copy(MirValue::Const(MirConst::Int(0)))),
            (MirValue::Const(MirConst::Int(0)), _) => Some(Copy(MirValue::Const(MirConst::Int(0)))),
            _ => None,
        },
        SDiv(l, r) => match (l, r) {
            (_, MirValue::Const(MirConst::Int(1))) => Some(Copy(l.clone())),
            _ => None,
        },
        ICmp(cmp, l, r) => match (cmp, l, r) {
            (MirCmp::Eq, MirValue::Temp(a), MirValue::Temp(b)) if a == b => {
                Some(Copy(MirValue::Const(MirConst::Bool(true))))
            }
            (MirCmp::Ne, MirValue::Temp(a), MirValue::Temp(b)) if a == b => {
                Some(Copy(MirValue::Const(MirConst::Bool(false))))
            }
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn strength_reduce(func: &mut MirFunction) -> usize {
    let mut count = 0;
    for block in &mut func.blocks {
        let mut i = 0;
        while i < block.insts.len() {
            let reduced = try_strength_reduce(&block.insts[i]);
            if let Some(new_op) = reduced {
                block.insts[i].op = new_op;
                count += 1;
            }
            i += 1;
        }
    }
    count
}

fn try_strength_reduce(inst: &MirInst) -> Option<MirOp> {
    use MirOp::*;
    match &inst.op {
        Mul(MirValue::Temp(t), MirValue::Const(MirConst::Int(c)))
        | Mul(MirValue::Const(MirConst::Int(c)), MirValue::Temp(t)) => {
            let uc = *c as u64;
            if *c > 0 && uc.is_power_of_two() {
                let k = uc.trailing_zeros() as i64;
                Some(Shl(MirValue::Temp(*t), MirValue::Const(MirConst::Int(k))))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn copy_propagate(func: &mut MirFunction) -> usize {
    let mut copies: HashMap<usize, MirValue> = HashMap::new();

    for block in &func.blocks {
        for inst in &block.insts {
            if let MirOp::Copy(ref v) = inst.op
                && let Some(id) = inst.result
            {
                let resolved = resolve_copy(v, &copies);
                copies.insert(id, resolved);
            }
        }
    }

    if copies.is_empty() {
        return 0;
    }

    let mut count = 0;
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            count += replace_value_in_op(&mut inst.op, &copies);
        }
        count += replace_value_in_terminator(&mut block.terminator, &copies);
    }
    count
}

fn resolve_copy(v: &MirValue, copies: &HashMap<usize, MirValue>) -> MirValue {
    match v {
        MirValue::Temp(id) => copies.get(id).cloned().unwrap_or_else(|| v.clone()),
        _ => v.clone(),
    }
}

fn replace_value_in_op(op: &mut MirOp, copies: &HashMap<usize, MirValue>) -> usize {
    let mut count = 0;
    fn replace(v: &mut MirValue, copies: &HashMap<usize, MirValue>, count: &mut usize) {
        if let MirValue::Temp(id) = v
            && let Some(replacement) = copies.get(id)
        {
            *v = replacement.clone();
            *count += 1;
        }
    }
    match op {
        MirOp::Load(v, _)
        | MirOp::PtrToInt(v, _)
        | MirOp::IntToPtr(v, _)
        | MirOp::BitCast(v, _)
        | MirOp::ZExt(v, _)
        | MirOp::Trunc(v, _)
        | MirOp::Sitofp(v, _)
        | MirOp::Fptosi(v, _)
        | MirOp::Copy(v) => replace(v, copies, &mut count),
        MirOp::Store(dst, src) => {
            replace(dst, copies, &mut count);
            replace(src, copies, &mut count);
        }
        MirOp::Add(l, r)
        | MirOp::Sub(l, r)
        | MirOp::Mul(l, r)
        | MirOp::SDiv(l, r)
        | MirOp::SRem(l, r)
        | MirOp::Shl(l, r)
        | MirOp::And(l, r)
        | MirOp::Or(l, r)
        | MirOp::ICmp(_, l, r)
        | MirOp::FCmp(_, l, r) => {
            replace(l, copies, &mut count);
            replace(r, copies, &mut count);
        }
        MirOp::Call(_, args, _) => {
            for a in args {
                replace(a, copies, &mut count);
            }
        }
        MirOp::Gep(base, indices, _name) => {
            replace(base, copies, &mut count);
            for i in indices {
                replace(i, copies, &mut count);
            }
        }
        MirOp::Phi(vals, _) => {
            for (v, _) in vals {
                replace(v, copies, &mut count);
            }
        }
        MirOp::Select(c, t, f) => {
            replace(c, copies, &mut count);
            replace(t, copies, &mut count);
            replace(f, copies, &mut count);
        }
        MirOp::Alloca(_) => {}
    }
    count
}

fn replace_value_in_terminator(
    term: &mut MirTerminator,
    copies: &HashMap<usize, MirValue>,
) -> usize {
    let mut count = 0;
    fn replace(v: &mut MirValue, copies: &HashMap<usize, MirValue>, count: &mut usize) {
        if let MirValue::Temp(id) = v
            && let Some(replacement) = copies.get(id)
        {
            *v = replacement.clone();
            *count += 1;
        }
    }
    match term {
        MirTerminator::BrCond(v, _, _) | MirTerminator::Ret(Some(v)) => {
            replace(v, copies, &mut count);
        }
        _ => {}
    }
    count
}

pub(super) fn merge_blocks(func: &mut MirFunction) -> usize {
    if func.blocks.len() < 2 {
        return 0;
    }

    let pred_counts = compute_predecessor_counts(func);
    let mut count = 0;
    let mut i = 0;
    while i + 1 < func.blocks.len() {
        let can_merge = {
            let term_is_br = matches!(
                func.blocks[i].terminator,
                MirTerminator::Br(target) if target == i + 1
            );
            let next_has_one_pred = pred_counts.get(&(i + 1)).copied().unwrap_or(0) == 1;
            term_is_br && next_has_one_pred
        };

        if can_merge {
            let next = func.blocks.remove(i + 1);
            let prev = &mut func.blocks[i];
            prev.insts.extend(next.insts);
            prev.terminator = next.terminator;
            count += 1;
        } else {
            i += 1;
        }
    }

    fix_block_ids(func);
    count
}

pub(super) fn compute_predecessor_counts(func: &MirFunction) -> HashMap<usize, usize> {
    let mut counts = HashMap::new();
    for block in &func.blocks {
        match &block.terminator {
            MirTerminator::Br(t) => *counts.entry(*t).or_insert(0) += 1,
            MirTerminator::BrCond(_, t, f) => {
                *counts.entry(*t).or_insert(0) += 1;
                *counts.entry(*f).or_insert(0) += 1;
            }
            _ => {}
        }
    }
    counts
}

pub(super) fn fix_block_ids(func: &mut MirFunction) {
    let old_to_new: HashMap<usize, usize> = func
        .blocks
        .iter()
        .enumerate()
        .map(|(new_id, block)| (block.id, new_id))
        .collect();

    for (new_id, block) in func.blocks.iter_mut().enumerate() {
        block.id = new_id;
        block.terminator = match &block.terminator {
            MirTerminator::Br(t) => MirTerminator::Br(*old_to_new.get(t).unwrap_or(t)),
            MirTerminator::BrCond(c, t, f) => MirTerminator::BrCond(
                c.clone(),
                *old_to_new.get(t).unwrap_or(t),
                *old_to_new.get(f).unwrap_or(f),
            ),
            other => other.clone(),
        };
    }
}
