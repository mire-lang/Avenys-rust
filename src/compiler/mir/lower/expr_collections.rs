use super::MirLower;
use crate::compiler::location::{NO_POSITION, expression_location};
use crate::compiler::mir::*;
use crate::parser::ast::{DataType, Expression, Statement};

impl MirLower {
    pub(crate) fn extract_closure_expr(expr: &Expression) -> &Expression {
        if let Expression::Closure { body, .. } = expr
            && let Some(Statement::Return(Some(inner))) = body.first()
        {
            return inner;
        }
        expr
    }

    pub(crate) fn lower_lists_map(&mut self, args: &[Expression]) -> MirValue {
        let loc = args.first().map(|e| expression_location(e).to_tuple()).unwrap_or(NO_POSITION.to_tuple());
        let closure_val = self.lower_expression(&args[0]);
        let list_val = self.lower_expression(&args[1]);

        let result_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(result_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::Unknown,
            }),
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
                MirType {
                    data_type: DataType::Unknown,
                },
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
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
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

        self.current_block = cond_block;
        let i_loaded = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(i_loaded),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let len_val = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(len_val),
            MirOp::Call(
                MirValue::Global("rt_list_len".to_string()),
                vec![list_val.clone()],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let cond = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(cond),
            MirOp::ICmp(
                MirCmp::Lt,
                MirValue::temp(i_loaded),
                MirValue::temp(len_val),
            ),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let elem = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let mapped = self.new_temp();
        self.func.blocks[body_block].push(
            Some(mapped),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(elem)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let loaded_result = self.new_temp();
        self.func.blocks[body_block].push(
            Some(loaded_result),
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        let pushed = self.new_temp();
        self.func.blocks[body_block].push(
            Some(pushed),
            MirOp::Call(
                MirValue::Global("rt_list_push_i64".to_string()),
                vec![MirValue::temp(loaded_result), MirValue::temp(mapped)],
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        self.func.blocks[body_block].push(
            None,
            MirOp::Store(MirValue::temp(result_ptr), MirValue::temp(pushed)),
            loc,
        );
        let old_i = self.new_temp();
        self.func.blocks[body_block].push(
            Some(old_i),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
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
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        MirValue::temp(final_result)
    }

    pub(crate) fn lower_lists_filter(&mut self, args: &[Expression]) -> MirValue {
        let loc = args.first().map(|e| expression_location(e).to_tuple()).unwrap_or(NO_POSITION.to_tuple());
        let closure_val = self.lower_expression(&args[0]);
        let list_val = self.lower_expression(&args[1]);

        let result_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(result_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::Unknown,
            }),
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
                MirType {
                    data_type: DataType::Unknown,
                },
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
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
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
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
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
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let len_val = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(len_val),
            MirOp::Call(
                MirValue::Global("rt_list_len".to_string()),
                vec![list_val.clone()],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let cond = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(cond),
            MirOp::ICmp(
                MirCmp::Lt,
                MirValue::temp(i_loaded),
                MirValue::temp(len_val),
            ),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let elem_raw = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem_raw),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType {
                    data_type: DataType::I64,
                },
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
                MirType {
                    data_type: DataType::Bool,
                },
            ),
            loc,
        );
        self.func.blocks[body_block].terminator =
            MirTerminator::BrCond(MirValue::temp(keep), keep_block, inc_block);

        self.current_block = keep_block;
        let elem = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(elem),
            MirOp::Load(
                MirValue::temp(elem_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let loaded_result = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(loaded_result),
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        let pushed = self.new_temp();
        self.func.blocks[keep_block].push(
            Some(pushed),
            MirOp::Call(
                MirValue::Global("rt_list_push_i64".to_string()),
                vec![MirValue::temp(loaded_result), MirValue::temp(elem)],
                MirType {
                    data_type: DataType::Unknown,
                },
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
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
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
            MirOp::Load(
                MirValue::temp(result_ptr),
                MirType {
                    data_type: DataType::Unknown,
                },
            ),
            loc,
        );
        MirValue::temp(final_result)
    }

    pub(crate) fn lower_lists_fold(&mut self, args: &[Expression]) -> MirValue {
        let loc = args.first().map(|e| expression_location(e).to_tuple()).unwrap_or(NO_POSITION.to_tuple());
        let acc_init = self.lower_expression(&args[0]);
        let closure_val = self.lower_expression(&args[1]);
        let list_val = self.lower_expression(&args[2]);

        let acc_ptr = self.new_temp();
        self.func.blocks[self.current_block].push(
            Some(acc_ptr),
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
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
            MirOp::Alloca(MirType {
                data_type: DataType::I64,
            }),
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
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let len_val = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(len_val),
            MirOp::Call(
                MirValue::Global("rt_list_len".to_string()),
                vec![list_val.clone()],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let cond = self.new_temp();
        self.func.blocks[cond_block].push(
            Some(cond),
            MirOp::ICmp(
                MirCmp::Lt,
                MirValue::temp(i_loaded),
                MirValue::temp(len_val),
            ),
            loc,
        );
        self.func.blocks[cond_block].terminator =
            MirTerminator::BrCond(MirValue::temp(cond), body_block, end_block);

        self.current_block = body_block;
        let i_loaded2 = self.new_temp();
        self.func.blocks[body_block].push(
            Some(i_loaded2),
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let elem = self.new_temp();
        self.func.blocks[body_block].push(
            Some(elem),
            MirOp::Call(
                MirValue::Global("rt_lists_get_i64".to_string()),
                vec![list_val.clone(), MirValue::temp(i_loaded2)],
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let acc_loaded = self.new_temp();
        self.func.blocks[body_block].push(
            Some(acc_loaded),
            MirOp::Load(
                MirValue::temp(acc_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
            loc,
        );
        let new_acc = self.new_temp();
        self.func.blocks[body_block].push(
            Some(new_acc),
            MirOp::Call(
                closure_val.clone(),
                vec![MirValue::temp(acc_loaded), MirValue::temp(elem)],
                MirType {
                    data_type: DataType::I64,
                },
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
            MirOp::Load(
                MirValue::temp(i_ptr),
                MirType {
                    data_type: DataType::I64,
                },
            ),
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
            MirOp::Load(
            MirValue::temp(acc_ptr),
            MirType {
                data_type: DataType::I64,
            },
        ),
        loc,
    );
    MirValue::temp(final_result)
}

}
