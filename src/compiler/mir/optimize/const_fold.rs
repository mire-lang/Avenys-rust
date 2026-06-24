use super::*;

pub(super) fn constant_fold_function(func: &mut MirFunction) -> usize {
    let mut count = 0;
    for block in &mut func.blocks {
        let mut i = 0;
        while i < block.insts.len() {
            let result = try_fold(&block.insts[i]);
            if let Some(folded) = result {
                block.insts[i].op = MirOp::Copy(MirValue::Const(folded));
                count += 1;
            }
            i += 1;
        }
    }
    count
}

fn try_fold(inst: &MirInst) -> Option<MirConst> {
    use MirOp::*;
    match &inst.op {
        Add(MirValue::Const(a), MirValue::Const(b)) => binop_const(a, b, |x, y| x + y, |x, y| x + y),
        Sub(MirValue::Const(a), MirValue::Const(b)) => binop_const(a, b, |x, y| x - y, |x, y| x - y),
        Mul(MirValue::Const(a), MirValue::Const(b)) => binop_const(a, b, |x, y| x * y, |x, y| x * y),
        SDiv(MirValue::Const(a), MirValue::Const(b)) => {
            match (a, b) {
                (MirConst::Int(0), _) => None,
                (_, MirConst::Int(0)) => None,
                _ => binop_const(a, b, |x, y| x / y, |_, _| 0.0),
            }
        }
        SRem(MirValue::Const(a), MirValue::Const(b)) => {
            match (a, b) {
                (MirConst::Int(0), _) => None,
                (_, MirConst::Int(0)) => None,
                _ => binop_const(a, b, |x, y| x % y, |_, _| 0.0),
            }
        }
        Shl(MirValue::Const(a), MirValue::Const(b)) => {
            match (a, b) {
                (MirConst::Int(x), MirConst::Int(y)) => Some(MirConst::Int(x << y)),
                _ => None,
            }
        }
        ICmp(cmp, MirValue::Const(a), MirValue::Const(b)) => cmp_const(cmp, a, b),
        FCmp(cmp, MirValue::Const(a), MirValue::Const(b)) => fcmp_const(cmp, a, b),
        ZExt(MirValue::Const(MirConst::Bool(v)), _) => Some(MirConst::Int(if *v { 1 } else { 0 })),
        _ => None,
    }
}

fn binop_const<F: Fn(i64, i64) -> i64, G: Fn(f64, f64) -> f64>(
    a: &MirConst,
    b: &MirConst,
    int_op: F,
    float_op: G,
) -> Option<MirConst> {
    match (a, b) {
        (MirConst::Int(x), MirConst::Int(y)) => Some(MirConst::Int(int_op(*x, *y))),
        (MirConst::Float(x), MirConst::Float(y)) => Some(MirConst::Float(float_op(*x, *y))),
        _ => None,
    }
}

fn cmp_const(cmp: &MirCmp, a: &MirConst, b: &MirConst) -> Option<MirConst> {
    let result = match (a, b) {
        (MirConst::Int(x), MirConst::Int(y)) => Some(int_cmp(cmp, *x, *y)),
        (MirConst::Float(x), MirConst::Float(y)) => Some(float_cmp(cmp, *x, *y)),
        (MirConst::Bool(x), MirConst::Bool(y)) => Some(int_cmp(cmp, *x as i64, *y as i64)),
        (MirConst::Char(x), MirConst::Char(y)) => Some(int_cmp(cmp, *x as u32 as i64, *y as u32 as i64)),
        _ => None,
    };
    result.map(MirConst::Bool)
}

fn fcmp_const(cmp: &MirCmp, a: &MirConst, b: &MirConst) -> Option<MirConst> {
    match (a, b) {
        (MirConst::Float(x), MirConst::Float(y)) => Some(MirConst::Bool(float_cmp(cmp, *x, *y))),
        _ => None,
    }
}

fn int_cmp(cmp: &MirCmp, x: i64, y: i64) -> bool {
    match cmp {
        MirCmp::Eq => x == y,
        MirCmp::Ne => x != y,
        MirCmp::Lt => x < y,
        MirCmp::Le => x <= y,
        MirCmp::Gt => x > y,
        MirCmp::Ge => x >= y,
    }
}

fn float_cmp(cmp: &MirCmp, x: f64, y: f64) -> bool {
    match cmp {
        MirCmp::Eq => (x - y).abs() < 1e-12,
        MirCmp::Ne => (x - y).abs() >= 1e-12,
        MirCmp::Lt => x < y,
        MirCmp::Le => x <= y,
        MirCmp::Gt => x > y,
        MirCmp::Ge => x >= y,
    }
}

pub(super) fn fold_constant_branches(func: &mut MirFunction) -> usize {
    let mut count = 0;
    for block in &mut func.blocks {
        let folded = match &block.terminator {
            MirTerminator::BrCond(MirValue::Const(MirConst::Bool(true)), t, _) => {
                Some(MirTerminator::Br(*t))
            }
            MirTerminator::BrCond(MirValue::Const(MirConst::Bool(false)), _, f) => {
                Some(MirTerminator::Br(*f))
            }
            _ => None,
        };
        if let Some(new_term) = folded {
            block.terminator = new_term;
            count += 1;
        }
    }
    count
}
