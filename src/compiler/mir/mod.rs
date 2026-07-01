pub mod codegen;
pub mod inline;
pub mod lower;
pub mod optimize;

use crate::parser::ast::DataType;
use std::collections::HashMap;

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv_hash(bytes: &[u8]) -> u64 {
    let mut state = FNV_OFFSET_BASIS;
    for &b in bytes {
        state = state.wrapping_mul(FNV_PRIME) ^ (b as u64);
    }
    state
}

fn hash_data_type(dt: &DataType, buf: &mut Vec<u8>) {
    buf.extend_from_slice(format!("{:?}", dt).as_bytes());
}

fn hash_value(value: &MirValue, buf: &mut Vec<u8>) {
    match value {
        MirValue::Const(c) => {
            buf.push(0);
            match c {
                MirConst::Int(v) => {
                    buf.push(0);
                    buf.extend_from_slice(&v.to_le_bytes());
                }
                MirConst::Float(v) => {
                    buf.push(1);
                    buf.extend_from_slice(&v.to_bits().to_le_bytes());
                }
                MirConst::Bool(v) => {
                    buf.push(2);
                    buf.push(*v as u8);
                }
                MirConst::Char(v) => {
                    buf.push(3);
                    buf.extend_from_slice(&(*v as u32).to_le_bytes());
                }
                MirConst::Str(v) => {
                    buf.push(4);
                    buf.extend_from_slice(v.as_bytes());
                }
                MirConst::None => buf.push(5),
            }
        }
        MirValue::Temp(id) => {
            buf.push(1);
            buf.extend_from_slice(&id.to_le_bytes());
        }
        MirValue::Param(name) => {
            buf.push(2);
            buf.extend_from_slice(name.as_bytes());
        }
        MirValue::Global(name) => {
            buf.push(3);
            buf.extend_from_slice(name.as_bytes());
        }
        MirValue::EnvPtr => {
            buf.push(4);
        }
        MirValue::FunctionRef { name, env } => {
            buf.push(5);
            buf.extend_from_slice(name.as_bytes());
            hash_value(env, buf);
        }
    }
}

fn hash_op(op: &MirOp, buf: &mut Vec<u8>) {
    match op {
        MirOp::Alloca(ty) => {
            buf.push(0);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::Load(v, ty) => {
            buf.push(1);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::Store(d, s) => {
            buf.push(2);
            hash_value(d, buf);
            hash_value(s, buf);
        }
        MirOp::Add(l, r) => {
            buf.push(3);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::Sub(l, r) => {
            buf.push(4);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::Mul(l, r) => {
            buf.push(5);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::SDiv(l, r) => {
            buf.push(6);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::SRem(l, r) => {
            buf.push(7);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::Shl(l, r) => {
            buf.push(8);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::And(l, r) => {
            buf.push(9);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::Or(l, r) => {
            buf.push(10);
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::ICmp(cmp, l, r) => {
            buf.push(11);
            buf.push(cmp.discriminant());
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::FCmp(cmp, l, r) => {
            buf.push(12);
            buf.push(cmp.discriminant());
            hash_value(l, buf);
            hash_value(r, buf);
        }
        MirOp::Call(callee, args, ret) => {
            buf.push(13);
            hash_value(callee, buf);
            for a in args {
                hash_value(a, buf);
            }
            hash_data_type(&ret.data_type, buf);
        }
        MirOp::Gep(base, indices, name) => {
            buf.push(14);
            hash_value(base, buf);
            for i in indices {
                hash_value(i, buf);
            }
            buf.extend_from_slice(name.as_bytes());
        }
        MirOp::PtrToInt(v, ty) => {
            buf.push(15);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::IntToPtr(v, ty) => {
            buf.push(16);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::BitCast(v, ty) => {
            buf.push(17);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::ZExt(v, ty) => {
            buf.push(18);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::Trunc(v, ty) => {
            buf.push(19);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::Sitofp(v, ty) => {
            buf.push(20);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::Fptosi(v, ty) => {
            buf.push(21);
            hash_value(v, buf);
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::Phi(pairs, ty) => {
            buf.push(22);
            for (v, bb) in pairs {
                hash_value(v, buf);
                buf.extend_from_slice(&bb.to_le_bytes());
            }
            hash_data_type(&ty.data_type, buf);
        }
        MirOp::Select(c, t, f) => {
            buf.push(23);
            hash_value(c, buf);
            hash_value(t, buf);
            hash_value(f, buf);
        }
        MirOp::Copy(v) => {
            buf.push(24);
            hash_value(v, buf);
        }
    }
}

#[derive(Debug, Clone)]
pub struct MirProgram {
    pub functions: Vec<MirFunction>,
    pub entry_point: Option<String>,
    pub extern_functions: Vec<MirExternFunction>,
    pub extern_libs: Vec<(String, String)>,
    pub struct_types: HashMap<String, Vec<(String, DataType)>>,
}

#[derive(Debug, Clone)]
pub struct MirExternFunction {
    pub name: String,
    pub lib_name: String,
    pub params: Vec<DataType>,
    pub return_type: DataType,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<MirParam>,
    pub ret_type: DataType,
    pub blocks: Vec<MirBlock>,
    pub body_hash: u64,
    pub noinline: bool,
}

#[derive(Debug, Clone)]
pub struct MirParam {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone)]
pub struct MirBlock {
    pub id: usize,
    pub label: String,
    pub insts: Vec<MirInst>,
    pub terminator: MirTerminator,
}

#[derive(Clone, Debug)]
pub enum MirValue {
    Const(MirConst),
    Temp(usize),
    Param(String),
    Global(String),
    /// Reference to the implicit environment pointer of the current mire
    /// function. Used by closure bodies to load captured values.
    EnvPtr,
    /// Reference to a mire function together with the value that provides its
    /// environment pointer. For top-level functions and non-capturing closures
    /// this is `Const(None)` (i.e. `ptr null`). For capturing closures it is a
    /// temp holding the allocated environment struct.
    FunctionRef {
        name: String,
        env: Box<MirValue>,
    },
}

#[derive(Clone, Debug)]
pub enum MirConst {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(String),
    None,
}

#[derive(Clone, Debug)]
pub struct MirType {
    pub data_type: DataType,
}

#[derive(Debug, Clone)]
pub struct MirInst {
    pub result: Option<usize>,
    pub op: MirOp,
    pub loc: (usize, usize),
}

#[derive(Debug, Clone)]
pub enum MirOp {
    Alloca(MirType),
    Load(MirValue, MirType),
    Store(MirValue, MirValue),
    Add(MirValue, MirValue),
    Sub(MirValue, MirValue),
    Mul(MirValue, MirValue),
    SDiv(MirValue, MirValue),
    SRem(MirValue, MirValue),
    Shl(MirValue, MirValue),
    And(MirValue, MirValue),
    Or(MirValue, MirValue),
    ICmp(MirCmp, MirValue, MirValue),
    FCmp(MirCmp, MirValue, MirValue),
    Call(MirValue, Vec<MirValue>, MirType),
    Gep(MirValue, Vec<MirValue>, String),
    PtrToInt(MirValue, MirType),
    IntToPtr(MirValue, MirType),
    BitCast(MirValue, MirType),
    ZExt(MirValue, MirType),
    Trunc(MirValue, MirType),
    Sitofp(MirValue, MirType),
    Fptosi(MirValue, MirType),
    Phi(Vec<(MirValue, usize)>, MirType),
    Select(MirValue, MirValue, MirValue),
    Copy(MirValue),
}

#[derive(Debug, Clone)]
pub enum MirCmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl MirCmp {
    fn discriminant(&self) -> u8 {
        match self {
            MirCmp::Eq => 0,
            MirCmp::Ne => 1,
            MirCmp::Lt => 2,
            MirCmp::Le => 3,
            MirCmp::Gt => 4,
            MirCmp::Ge => 5,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MirTerminator {
    Br(usize),
    BrCond(MirValue, usize, usize),
    Ret(Option<MirValue>),
    Unreachable,
}

impl MirProgram {
    pub fn new(functions: Vec<MirFunction>, entry_point: Option<String>) -> Self {
        Self {
            functions,
            entry_point,
            extern_functions: Vec::new(),
            extern_libs: Vec::new(),
            struct_types: HashMap::new(),
        }
    }
}

impl MirFunction {
    pub fn new(name: String, params: Vec<MirParam>, ret_type: DataType) -> Self {
        Self {
            name,
            params,
            ret_type,
            blocks: Vec::new(),
            body_hash: 0,
            noinline: false,
        }
    }

    pub fn next_temp(&self) -> usize {
        self.blocks
            .iter()
            .flat_map(|b| b.insts.iter())
            .filter_map(|inst| inst.result)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0)
    }

    pub fn compute_hash(&self) -> u64 {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.name.as_bytes());
        for param in &self.params {
            buf.extend_from_slice(param.name.as_bytes());
            hash_data_type(&param.data_type, &mut buf);
        }
        hash_data_type(&self.ret_type, &mut buf);
        for block in &self.blocks {
            buf.extend_from_slice(block.label.as_bytes());
            buf.extend_from_slice(&block.id.to_le_bytes());
            for inst in &block.insts {
                buf.push(inst.result.unwrap_or(255) as u8);
                hash_op(&inst.op, &mut buf);
            }
            match &block.terminator {
                MirTerminator::Br(t) => {
                    buf.extend_from_slice(&(*t as u64).to_le_bytes());
                    buf.push(0);
                }
                MirTerminator::BrCond(v, t, f) => {
                    hash_value(v, &mut buf);
                    buf.extend_from_slice(&(*t as u64).to_le_bytes());
                    buf.extend_from_slice(&(*f as u64).to_le_bytes());
                    buf.push(1);
                }
                MirTerminator::Ret(Some(v)) => {
                    buf.push(2);
                    hash_value(v, &mut buf);
                }
                MirTerminator::Ret(None) => buf.push(3),
                MirTerminator::Unreachable => buf.push(4),
            }
        }
        fnv_hash(&buf)
    }

    pub fn push_block(&mut self, label: String) -> usize {
        let id = self.blocks.len();
        self.blocks.push(MirBlock {
            id,
            label,
            insts: Vec::new(),
            terminator: MirTerminator::Unreachable,
        });
        id
    }
}

impl MirBlock {
    pub fn push(&mut self, result: Option<usize>, op: MirOp, loc: (usize, usize)) {
        self.insts.push(MirInst { result, op, loc });
    }
}

impl MirValue {
    pub fn temp(id: usize) -> Self {
        MirValue::Temp(id)
    }
}
