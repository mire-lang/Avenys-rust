use super::*;
use super::simplify::{compute_predecessor_counts, fix_block_ids};
use std::collections::HashSet;

pub(super) fn dead_block_elim(func: &mut MirFunction) -> usize {
    let pred_counts = compute_predecessor_counts(func);
    let mut removed = 0;
    func.blocks.retain(|b| {
        let is_entry = b.id == 0 || b.label == "entry";
        let has_preds = pred_counts.get(&b.id).copied().unwrap_or(0) > 0;
        let keep = is_entry || has_preds;
        if !keep {
            removed += 1;
        }
        keep
    });
    if removed > 0 {
        fix_block_ids(func);
    }
    removed
}

pub(super) fn dce_function(func: &mut MirFunction) -> usize {
    let mut count = 0;
    let mut used = HashSet::new();

    for block in &func.blocks {
        for inst in &block.insts {
            collect_uses(&inst.op, &mut used);
        }
        collect_terminator_uses(&block.terminator, &mut used);
    }

    for block in &mut func.blocks {
        block.insts.retain(|inst| {
            let has_side_effect = has_side_effect(&inst.op);
            let is_used = inst.result.map_or(true, |r| used.contains(&r));
            if !has_side_effect && !is_used && inst.result.is_some() {
                count += 1;
                return false;
            }
            true
        });
    }

    count
}

fn collect_uses(op: &MirOp, used: &mut HashSet<usize>) {
    fn collect_val(v: &MirValue, used: &mut HashSet<usize>) {
        if let MirValue::Temp(id) = v {
            used.insert(*id);
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
        | MirOp::Fptosi(v, _) => collect_val(v, used),
        MirOp::Store(dst, src) => {
            collect_val(dst, used);
            collect_val(src, used);
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
            collect_val(l, used);
            collect_val(r, used);
        }
        MirOp::Call(_, args, _) => {
            for a in args {
                collect_val(a, used);
            }
        }
        MirOp::Gep(base, indices, _name) => {
            collect_val(base, used);
            for i in indices {
                collect_val(i, used);
            }
        }
        MirOp::Phi(vals, _) => {
            for (v, _) in vals {
                collect_val(v, used);
            }
        }
        MirOp::Select(c, t, f) => {
            collect_val(c, used);
            collect_val(t, used);
            collect_val(f, used);
        }
        MirOp::Copy(v) => collect_val(v, used),
        MirOp::Alloca(_) => {}
    }
}

fn collect_terminator_uses(term: &MirTerminator, used: &mut HashSet<usize>) {
    match term {
        MirTerminator::BrCond(v, _, _) | MirTerminator::Ret(Some(v)) => {
            if let MirValue::Temp(id) = v {
                used.insert(*id);
            }
        }
        _ => {}
    }
}

fn has_side_effect(op: &MirOp) -> bool {
    matches!(op, MirOp::Store(_, _) | MirOp::Call(_, _, _))
}
