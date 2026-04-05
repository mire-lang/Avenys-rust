use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Protected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Int,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Float,
    F32,
    F64,
    Str,
    Bool,
    None,
    List,
    Vector {
        element_type: Box<DataType>,
        dynamic: bool,
    },
    Dict,
    Map {
        key_type: Box<DataType>,
        value_type: Box<DataType>,
    },
    Anything,
    Function,
    Db,
    Tuple,
    Set,
    Datetime,
    Unknown,
    Ref,
    RefMut,
    Box,
    Enum,
    DynTrait {
        trait_name: String,
    },
    Array {
        element_type: Box<DataType>,
        size: usize,
    },
    Slice {
        element_type: Box<DataType>,
    },
    /// Represents an operation that may succeed with `ok` or fail with an error
    /// string. Used by Fs and Env operations to signal recoverable failures
    /// without panicking the program. Future `try`/`catch` blocks will unwrap
    /// this type automatically.
    Result {
        ok: Box<DataType>,
    },
}

impl DataType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "int" => DataType::Int,
            "i8" => DataType::I8,
            "i16" => DataType::I16,
            "i32" => DataType::I32,
            "i64" => DataType::I64,
            "u8" => DataType::U8,
            "u16" => DataType::U16,
            "u32" => DataType::U32,
            "u64" => DataType::U64,
            "float" => DataType::Float,
            "f32" => DataType::F32,
            "f64" => DataType::F64,
            "str" => DataType::Str,
            "bool" => DataType::Bool,
            "none" => DataType::None,
            "list" => DataType::List,
            "vec" => DataType::Vector {
                element_type: Box::new(DataType::Unknown),
                dynamic: false,
            },
            "dict" => DataType::Dict,
            "map" => DataType::Map {
                key_type: Box::new(DataType::Unknown),
                value_type: Box::new(DataType::Unknown),
            },
            "anything" => DataType::Anything,
            "function" => DataType::Function,
            "db" => DataType::Db,
            "tuple" => DataType::Tuple,
            "set" => DataType::Set,
            "datetime" => DataType::Datetime,
            "box" => DataType::Box,
            _ => DataType::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identifier {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitMethodSig {
    pub name: String,
    pub params: Vec<(String, DataType)>,
    pub return_type: DataType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    Literal(Literal),
    Identifier(Identifier),
    BinaryOp {
        operator: String,
        left: Box<Expression>,
        right: Box<Expression>,
        data_type: DataType,
    },
    UnaryOp {
        operator: String,
        operand: Box<Expression>,
        data_type: DataType,
    },
    NamedArg {
        name: String,
        value: Box<Expression>,
        data_type: DataType,
    },
    Call {
        name: String,
        args: Vec<Expression>,
        data_type: DataType,
    },
    List {
        elements: Vec<Expression>,
        element_type: DataType,
        data_type: DataType,
    },
    Dict {
        entries: Vec<(Expression, Expression)>,
        data_type: DataType,
    },
    Tuple {
        elements: Vec<Expression>,
        data_type: DataType,
    },
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
        data_type: DataType,
    },
    MemberAccess {
        target: Box<Expression>,
        member: String,
        data_type: DataType,
    },
    Closure {
        params: Vec<(String, DataType)>,
        body: Vec<Statement>,
        return_type: DataType,
        capture: Vec<(String, MireValue)>,
    },
    Reference {
        expr: Box<Expression>,
        is_mutable: bool,
        data_type: DataType,
    },
    Dereference {
        expr: Box<Expression>,
        data_type: DataType,
    },
    Box {
        value: Box<Expression>,
        data_type: DataType,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    List(Vec<Expression>),
    Dict(Vec<((Expression, Expression), DataType)>),
    Tuple(Vec<Expression>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MireFloat(pub f64);

impl MireFloat {
    pub fn new(value: f64) -> Self {
        MireFloat(value)
    }

    pub fn to_bits(self) -> u64 {
        self.0.to_bits()
    }
}

impl Deref for MireFloat {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq for MireFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for MireFloat {}

impl Hash for MireFloat {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for MireFloat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl From<f64> for MireFloat {
    fn from(value: f64) -> Self {
        MireFloat(value)
    }
}

impl From<MireFloat> for f64 {
    fn from(value: MireFloat) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MireFloat32(pub f32);

impl MireFloat32 {
    pub fn new(value: f32) -> Self {
        MireFloat32(value)
    }
}

impl Deref for MireFloat32 {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq for MireFloat32 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for MireFloat32 {}

impl Hash for MireFloat32 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for MireFloat32 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl From<f32> for MireFloat32 {
    fn from(value: f32) -> Self {
        MireFloat32(value)
    }
}

impl From<MireFloat32> for f32 {
    fn from(value: MireFloat32) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Statement {
    Let {
        name: String,
        data_type: DataType,
        value: Option<Expression>,
        is_constant: bool,
        is_static: bool,
        visibility: Visibility,
    },
    Assignment {
        target: String,
        value: Expression,
        is_mutable: bool,
    },
    Function {
        name: String,
        params: Vec<(String, DataType)>,
        body: Vec<Statement>,
        return_type: DataType,
        visibility: Visibility,
        is_method: bool,
    },
    Return(Option<Expression>),
    If {
        condition: Expression,
        then_branch: Vec<Statement>,
        else_branch: Option<Vec<Statement>>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    For {
        variable: String,
        iterable: Expression,
        body: Vec<Statement>,
    },
    Expression(Expression),
    Break,
    Continue,
    Find {
        variable: String,
        iterable: Expression,
        body: Vec<Statement>,
    },
    Match {
        value: Expression,
        cases: Vec<(Expression, Vec<Statement>)>,
        default: Vec<Statement>,
    },
    Type {
        name: String,
        parent: Option<String>,
        fields: Vec<Statement>,
    },
    Skill {
        name: String,
        methods: Vec<TraitMethodSig>,
    },
    Code {
        trait_name: String,
        type_name: String,
        methods: Vec<Statement>,
    },
    Class {
        name: String,
        parent: Option<String>,
        methods: Vec<Statement>,
    },
    Trait {
        name: String,
        methods: Vec<TraitMethodSig>,
    },
    Impl {
        trait_name: Option<String>,
        type_name: String,
        methods: Vec<Statement>,
    },
    ExternLib {
        name: String,
        path: String,
    },
    ExternFunction {
        name: String,
        lib_name: String,
        params: Vec<(String, DataType)>,
        return_type: DataType,
    },
    Unsafe {
        body: Vec<Statement>,
    },
    Asm {
        instructions: Vec<(String, Expression)>,
    },
    AddLib {
        path: String,
    },
    Use {
        path: String,
    },
    Module {
        name: String,
        body: Vec<Statement>,
    },
    Drop {
        value: Expression,
    },
    Move {
        target: String,
        value: Expression,
    },
    Enum {
        name: String,
        variants: Vec<(String, Vec<DataType>)>,
    },
    DmireTable {
        name: String,
        columns: Vec<String>,
        body: Vec<Statement>,
    },
    DmireColumn {
        name: String,
        col_type: Option<String>,
        body: Vec<Statement>,
    },
    DmireDlist {
        index: usize,
        data: Vec<Expression>,
    },
    Query {
        table: String,
        bindings: Vec<QueryBinding>,
        ops: Vec<QueryOp>,
        joins: Vec<QueryJoin>,
        group_by: Option<QueryGroup>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryBinding {
    pub target: String,
    pub alias: String,
    pub column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryGet {
    pub target: String,
    pub condition: Expression,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryJoin {
    pub right_table: String,
    pub left_column: String,
    pub right_column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryGroup {
    pub column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryOp {
    Insert {
        assigns: Vec<(String, Expression)>,
    },
    Update {
        condition: Expression,
        assigns: Vec<(String, Expression)>,
    },
    Delete {
        condition: Expression,
    },
    Get(QueryGet),
    Export {
        path: String,
    },
    Import {
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariantDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariantDef {
    pub name: String,
    pub data_types: Vec<DataType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MireValue {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Float(MireFloat),
    F32(MireFloat32),
    F64(f64),
    Str(String),
    Bool(bool),
    None,
    List(Vec<MireValue>),
    Dict(Vec<((MireValue, MireValue), DataType)>),
    Tuple(Vec<MireValue>),
    Function(FunctionDef),
    Builtinfn(String),
    Object {
        class_name: String,
        parent: Option<String>,
        fields: Vec<((String, MireValue), DataType)>,
        methods: Vec<FunctionDef>,
    },
    Trait {
        name: String,
        methods: Vec<TraitMethodSig>,
    },
    Instance {
        class_name: String,
        fields: Vec<((String, MireValue), DataType)>,
        methods: Vec<FunctionDef>,
    },
    Ref {
        value: Box<MireValue>,
        is_mutable: bool,
    },
    Box {
        value: Box<MireValue>,
    },
    Array {
        elements: Vec<MireValue>,
        size: usize,
    },
    Slice {
        elements: Vec<MireValue>,
    },
    EnumVariant {
        enum_name: String,
        variant_name: String,
        data: Option<Box<MireValue>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<(String, DataType)>,
    pub body: Arc<Vec<Statement>>,
    pub return_type: DataType,
    pub is_method: bool,
    pub capture: Vec<(String, MireValue)>,
}

impl MireValue {
    pub fn eq(&self, other: &MireValue) -> bool {
        match (self, other) {
            (MireValue::I8(a), MireValue::I8(b)) => a == b,
            (MireValue::I16(a), MireValue::I16(b)) => a == b,
            (MireValue::I32(a), MireValue::I32(b)) => a == b,
            (MireValue::I64(a), MireValue::I64(b)) => a == b,
            (MireValue::U8(a), MireValue::U8(b)) => a == b,
            (MireValue::U16(a), MireValue::U16(b)) => a == b,
            (MireValue::U32(a), MireValue::U32(b)) => a == b,
            (MireValue::U64(a), MireValue::U64(b)) => a == b,
            (MireValue::Float(a), MireValue::Float(b)) => a == b,
            (MireValue::F32(a), MireValue::F32(b)) => a == b,
            (MireValue::F64(a), MireValue::F64(b)) => a == b,
            (MireValue::Str(a), MireValue::Str(b)) => a == b,
            (MireValue::Bool(a), MireValue::Bool(b)) => a == b,
            (MireValue::None, MireValue::None) => true,
            (MireValue::List(a), MireValue::List(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for (x, y) in a.iter().zip(b.iter()) {
                    if !x.eq(y) {
                        return false;
                    }
                }
                true
            }
            (MireValue::Tuple(a), MireValue::Tuple(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for (x, y) in a.iter().zip(b.iter()) {
                    if !x.eq(y) {
                        return false;
                    }
                }
                true
            }
            (MireValue::Dict(a), MireValue::Dict(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for ((k, v), _) in a {
                    let mut found = false;
                    for ((ok, ov), _) in b {
                        if k.eq(ok) && v.eq(ov) {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                }
                true
            }
            (MireValue::Function(_), MireValue::Function(_)) => false,
            (MireValue::Builtinfn(a), MireValue::Builtinfn(b)) => a == b,
            (MireValue::Object { .. }, MireValue::Object { .. }) => false,
            (MireValue::Instance { .. }, MireValue::Instance { .. }) => false,
            (MireValue::Ref { .. }, MireValue::Ref { .. }) => false,
            (MireValue::Box { .. }, MireValue::Box { .. }) => false,
            (
                MireValue::Array {
                    elements: a,
                    size: _,
                },
                MireValue::Array {
                    elements: b,
                    size: _,
                },
            ) => {
                if a.len() != b.len() {
                    return false;
                }
                for (x, y) in a.iter().zip(b.iter()) {
                    if !x.eq(y) {
                        return false;
                    }
                }
                true
            }
            (MireValue::Slice { elements: a }, MireValue::Slice { elements: b }) => {
                if a.len() != b.len() {
                    return false;
                }
                for (x, y) in a.iter().zip(b.iter()) {
                    if !x.eq(y) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub fn ne(&self, other: &MireValue) -> bool {
        !self.eq(other)
    }
}
