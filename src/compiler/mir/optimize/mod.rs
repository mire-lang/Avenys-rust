pub mod const_fold;
pub mod dce;
pub mod simplify;

use super::inline::inlining;
use super::*;
use const_fold::*;
use dce::*;
use simplify::*;

pub fn optimize(program: &mut MirProgram) -> usize {
    let mut total = 0;
    total += inlining(program);
    for func in &mut program.functions {
        total += optimize_function(func);
    }
    total
}

pub fn optimize_function(func: &mut MirFunction) -> usize {
    let mut total = 0;
    loop {
        let c = constant_fold_function(func)
            + algebraic_simplify(func)
            + strength_reduce(func)
            + copy_propagate(func)
            + fold_constant_branches(func)
            + dce_function(func)
            + dead_block_elim(func)
            + merge_blocks(func);
        if c == 0 {
            break;
        }
        total += c;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::DataType;

    fn make_func(name: &str, insts: Vec<MirInst>, terminator: MirTerminator) -> MirFunction {
        let mut f = MirFunction {
            name: name.to_string(),
            params: vec![],
            ret_type: DataType::I64,
            blocks: vec![MirBlock {
                id: 0,
                label: "entry".to_string(),
                insts,
                terminator,
            }],
            body_hash: 0,
            noinline: false,
        };
        f.body_hash = f.compute_hash();
        f
    }

    fn inst(result: usize, op: MirOp) -> MirInst {
        MirInst {
            result: Some(result),
            op,
            loc: (0, 0),
        }
    }

    fn void_inst(op: MirOp) -> MirInst {
        MirInst {
            result: None,
            op,
            loc: (0, 0),
        }
    }

    fn t(id: usize) -> MirValue {
        MirValue::Temp(id)
    }

    fn i64(v: i64) -> MirValue {
        MirValue::Const(MirConst::Int(v))
    }

    // ── Algebraic simplification ──

    #[test]
    fn alg_x_plus_0() {
        let mut f = make_func("f", vec![inst(1, MirOp::Add(t(0), i64(0)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Temp(0))));
    }

    #[test]
    fn alg_0_plus_x() {
        let mut f = make_func("f", vec![inst(1, MirOp::Add(i64(0), t(0)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Temp(0))));
    }

    #[test]
    fn alg_x_mul_1() {
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(1)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Temp(0))));
    }

    #[test]
    fn alg_1_mul_x() {
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(i64(1), t(0)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Temp(0))));
    }

    #[test]
    fn alg_x_mul_0() {
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(0)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Const(MirConst::Int(0)))));
    }

    #[test]
    fn alg_0_mul_x() {
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(i64(0), t(0)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Const(MirConst::Int(0)))));
    }

    #[test]
    fn alg_x_minus_0() {
        let mut f = make_func("f", vec![inst(1, MirOp::Sub(t(0), i64(0)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Temp(0))));
    }

    #[test]
    fn alg_x_minus_x() {
        let mut f = make_func("f", vec![inst(1, MirOp::Sub(t(0), t(0)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Const(MirConst::Int(0)))));
    }

    #[test]
    fn alg_x_div_1() {
        let mut f = make_func("f", vec![inst(1, MirOp::SDiv(t(0), i64(1)))], MirTerminator::Ret(None));
        assert_eq!(algebraic_simplify(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Copy(MirValue::Temp(0))));
    }

    // ── Copy propagation ──

    #[test]
    fn copy_prop_simple() {
        // t1 = Copy(x); t2 = Add(t1, 1)  →  t2 = Add(x, 1)
        let mut f = make_func(
            "f",
            vec![
                inst(1, MirOp::Copy(t(0))),
                inst(2, MirOp::Add(t(1), i64(1))),
            ],
            MirTerminator::Ret(Some(t(2))),
        );
        assert_eq!(copy_propagate(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[1].op, MirOp::Add(MirValue::Temp(0), _)));
    }

    #[test]
    fn copy_prop_transitive() {
        // t1 = Copy(x); t2 = Copy(t1); t3 = Add(t2, 1)
        //   →  t2 = Copy(x); t3 = Add(x, 1)  (t1 replacement in t2)
        let mut f = make_func(
            "f",
            vec![
                inst(1, MirOp::Copy(t(0))),
                inst(2, MirOp::Copy(t(1))),
                inst(3, MirOp::Add(t(2), i64(1))),
            ],
            MirTerminator::Ret(Some(t(3))),
        );
        let count = copy_propagate(&mut f);
        assert!(count >= 2, "expected at least 2 replacements, got {count}");
        // t2 = Copy(x) — already handled by first pass
        assert!(matches!(f.blocks[0].insts[2].op, MirOp::Add(MirValue::Temp(0), _)));
    }

    // ── Dead code elimination ──

    #[test]
    fn dce_removes_unused_add() {
        let mut f = make_func("f", vec![inst(1, MirOp::Add(i64(1), i64(2)))], MirTerminator::Ret(None));
        assert_eq!(dce_function(&mut f), 1);
        assert!(f.blocks[0].insts.is_empty());
    }

    #[test]
    fn dce_preserves_call() {
        let mut f = make_func(
            "f",
            vec![void_inst(MirOp::Call(MirValue::FunctionRef { name: "print".to_string(), env: Box::new(MirValue::Const(MirConst::None)) }, vec![i64(42)], MirType { data_type: DataType::None }))],
            MirTerminator::Ret(None),
        );
        assert_eq!(dce_function(&mut f), 0);
        assert_eq!(f.blocks[0].insts.len(), 1);
    }

    #[test]
    fn dce_preserves_store() {
        // Store to a temp (dst, src)
        let mut f = make_func(
            "f",
            vec![void_inst(MirOp::Store(t(0), t(1)))],
            MirTerminator::Ret(None),
        );
        assert_eq!(dce_function(&mut f), 0);
        assert_eq!(f.blocks[0].insts.len(), 1);
    }

    #[test]
    fn dce_removes_unused_but_keeps_used() {
        // t1 = Add(1,2) — unused, removed
        // t2 = Add(t0, 1) — used by Ret, kept
        let mut f = make_func(
            "f",
            vec![
                inst(1, MirOp::Add(i64(1), i64(2))),
                inst(2, MirOp::Add(t(0), i64(1))),
            ],
            MirTerminator::Ret(Some(t(2))),
        );
        assert_eq!(dce_function(&mut f), 1);
        assert_eq!(f.blocks[0].insts.len(), 1);
        assert_eq!(f.blocks[0].insts[0].result, Some(2));
    }

    #[test]
    fn dce_preserves_void_call_unused_result() {
        // result = Call(...) — has side effect, kept even if result unused
        let mut f = make_func(
            "f",
            vec![inst(1, MirOp::Call(MirValue::FunctionRef { name: "print".to_string(), env: Box::new(MirValue::Const(MirConst::None)) }, vec![i64(42)], MirType { data_type: DataType::None }))],
            MirTerminator::Ret(None),
        );
        assert_eq!(dce_function(&mut f), 0);
        assert_eq!(f.blocks[0].insts.len(), 1);
    }

    // ── Constant branch folding ──

    #[test]
    fn fold_brcond_true() {
        let mut f = make_func("f", vec![], MirTerminator::BrCond(MirValue::Const(MirConst::Bool(true)), 1, 2));
        assert_eq!(fold_constant_branches(&mut f), 1);
        assert!(matches!(f.blocks[0].terminator, MirTerminator::Br(1)));
    }

    #[test]
    fn fold_brcond_false() {
        let mut f = make_func("f", vec![], MirTerminator::BrCond(MirValue::Const(MirConst::Bool(false)), 1, 2));
        assert_eq!(fold_constant_branches(&mut f), 1);
        assert!(matches!(f.blocks[0].terminator, MirTerminator::Br(2)));
    }

    #[test]
    fn fold_brcond_skip_nonconst() {
        let mut f = make_func("f", vec![], MirTerminator::BrCond(MirValue::Temp(0), 1, 2));
        assert_eq!(fold_constant_branches(&mut f), 0);
        assert!(matches!(f.blocks[0].terminator, MirTerminator::BrCond(..)));
    }

    // ── Dead block elimination ──

    fn make_multi_block(blocks: Vec<(Vec<MirInst>, MirTerminator)>) -> MirFunction {
        let mut f = MirFunction {
            name: "f".to_string(),
            params: vec![],
            ret_type: DataType::I64,
            blocks: blocks
                .into_iter()
                .enumerate()
                .map(|(id, (insts, terminator))| MirBlock {
                    id,
                    label: format!("b{id}"),
                    insts,
                    terminator,
                })
                .collect(),
            body_hash: 0,
            noinline: false,
        };
        f.body_hash = f.compute_hash();
        f
    }

    #[test]
    fn dead_elim_simple() {
        // block 0: entry, Br(1)
        // block 1: Ret, but block 2 is unreachable
        // block 2: dead (no predecessors)
        let mut f = make_multi_block(vec![
            (vec![], MirTerminator::Br(1)),
            (vec![], MirTerminator::Ret(None)),
            (vec![inst(0, MirOp::Add(i64(1), i64(2)))], MirTerminator::Ret(None)),
        ]);
        assert_eq!(dead_block_elim(&mut f), 1);
        assert_eq!(f.blocks.len(), 2);
    }

    #[test]
    fn dead_elim_keeps_entry() {
        // block 0 alone, no predecessors — entry must be kept
        let mut f = make_multi_block(vec![
            (vec![], MirTerminator::Ret(None)),
        ]);
        assert_eq!(dead_block_elim(&mut f), 0);
        assert_eq!(f.blocks.len(), 1);
    }

    #[test]
    fn dead_elim_after_branch_folding() {
        // Full pipeline: copy_prop + fold_brcond + dead_block_elim
        // t0 = Copy(Const(Bool(true)))
        // BrCond(t0, 1, 2)
        // block 1: Ret
        // block 2: dead (unreachable after folding)
        let mut f = make_multi_block(vec![
            (
                vec![inst(0, MirOp::Copy(MirValue::Const(MirConst::Bool(true))))],
                MirTerminator::BrCond(MirValue::Temp(0), 1, 2),
            ),
            (vec![], MirTerminator::Ret(None)),
            (vec![inst(1, MirOp::Add(i64(1), i64(2)))], MirTerminator::Ret(None)),
        ]);
        // Apply passes in order
        copy_propagate(&mut f);
        fold_constant_branches(&mut f);
        let removed = dead_block_elim(&mut f);
        assert_eq!(removed, 1, "dead block 2 should be removed");
        assert_eq!(f.blocks.len(), 2);
        // block 0 should now be Br(1) after folding
        assert!(matches!(f.blocks[0].terminator, MirTerminator::Br(1)));
    }

    // ── Strength reduction ──

    #[test]
    fn sr_mul_by_2_to_shl() {
        // x * 2 → x << 1
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(2)))], MirTerminator::Ret(None));
        assert_eq!(strength_reduce(&mut f), 1);
        let inst = &f.blocks[0].insts[0];
        assert!(matches!(inst.op, MirOp::Shl(MirValue::Temp(0), MirValue::Const(MirConst::Int(1)))));
    }

    #[test]
    fn sr_mul_by_8_to_shl_3() {
        // x * 8 → x << 3
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(8)))], MirTerminator::Ret(None));
        assert_eq!(strength_reduce(&mut f), 1);
        let inst = &f.blocks[0].insts[0];
        assert!(matches!(inst.op, MirOp::Shl(MirValue::Temp(0), MirValue::Const(MirConst::Int(3)))));
    }

    #[test]
    fn sr_commutative_const_first() {
        // 4 * x → x << 2
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(i64(4), t(0)))], MirTerminator::Ret(None));
        assert_eq!(strength_reduce(&mut f), 1);
        let inst = &f.blocks[0].insts[0];
        assert!(matches!(inst.op, MirOp::Shl(MirValue::Temp(0), MirValue::Const(MirConst::Int(2)))));
    }

    #[test]
    fn sr_non_power_of_two_unchanged() {
        // x * 3 → unchanged (not a power of 2)
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(3)))], MirTerminator::Ret(None));
        assert_eq!(strength_reduce(&mut f), 0);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Mul(_, _)));
    }

    #[test]
    fn sr_zero_unchanged() {
        // x * 0 → unchanged (handled by algebraic_simplify)
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(0)))], MirTerminator::Ret(None));
        assert_eq!(strength_reduce(&mut f), 0);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Mul(_, _)));
    }

    #[test]
    fn sr_negative_unchanged() {
        // x * -2 → unchanged (negative, not a power of 2 in i64 sense)
        let mut f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(-2)))], MirTerminator::Ret(None));
        assert_eq!(strength_reduce(&mut f), 0);
        assert!(matches!(f.blocks[0].insts[0].op, MirOp::Mul(_, _)));
    }

    #[test]
    fn sr_in_pipeline() {
        // End-to-end: Mul by power of 2 should be strength-reduced in optimize()
        // Use Ret(Temp(1)) so DCE doesn't remove the instruction
        let f = make_func("f", vec![inst(1, MirOp::Mul(t(0), i64(16)))], MirTerminator::Ret(Some(MirValue::Temp(1))));
        let mut prog = MirProgram {
            functions: vec![f],
            entry_point: None,
            extern_functions: vec![],
            struct_types: HashMap::new(),
        };
        let total = optimize(&mut prog);
        assert!(total >= 1, "expected at least strength_reduction, got {total}");
        let inst = &prog.functions[0].blocks[0].insts[0];
        assert!(matches!(inst.op, MirOp::Shl(MirValue::Temp(0), MirValue::Const(MirConst::Int(4)))));
    }

    #[test]
    fn full_pipeline_dead_block() {
        // End-to-end: optimize() should fold const branch + remove dead block
        let f = make_multi_block(vec![
            (
                vec![inst(0, MirOp::Copy(MirValue::Const(MirConst::Bool(false))))],
                MirTerminator::BrCond(MirValue::Temp(0), 1, 2),
            ),
            (vec![], MirTerminator::Ret(None)),
            (vec![], MirTerminator::Ret(None)),
        ]);
        // Wrap in a program
        let mut prog = MirProgram {
            functions: vec![f],
            entry_point: None,
            extern_functions: vec![],
            struct_types: HashMap::new(),
        };
        let total = optimize(&mut prog);
        assert!(total >= 3, "expected copy_prop + fold + dead_elim + merge, got {total}");
        // After all passes, the two remaining blocks should be merged into one
        assert_eq!(prog.functions[0].blocks.len(), 1);
        assert!(matches!(prog.functions[0].blocks[0].terminator, MirTerminator::Ret(None)));
    }
}
