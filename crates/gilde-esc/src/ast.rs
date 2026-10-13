#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    String,
    /// `byte` / `char`: signed 8-bit.
    Byte,
    Void,
}

impl Type {
    pub fn from_keyword(s: &str) -> Option<Self> {
        Some(match s {
            "int" => Type::Int,
            "float" => Type::Float,
            "string" => Type::String,
            "byte" | "char" => Type::Byte,
            "void" => Type::Void,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    BitOr,
    BitAnd,
    /// `=` inside an expression: the engine replaces the accumulator with the next
    /// operand without assigning anything (`if ((x = rnd(3)) == 1)` never sets x).
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicOp {
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum Operand {
    Int(i32),
    Str(String),
    Var(String, Option<Box<Expr>>),
    Call(String, Vec<Expr>),
    Paren(Box<Expr>),
}

/// An expression exactly as the engine evaluates it: an operand chain folded left
/// to right, optionally followed by a comparison whose right side is the rest,
/// optionally combined with further sub-expressions via `&&`/`||`.
#[derive(Debug, Clone)]
pub enum Expr {
    /// `first (op operand)*` folded left to right. A leading operator applies to 0.
    Chain(Vec<(BinOp, Operand)>),
    Cmp(Box<Expr>, CmpOp, Box<Expr>),
    Logic(Box<Expr>, LogicOp, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Decl { ty: Type, name: String, size: Option<u32>, init: Option<i32> },
    Assign { name: String, index: Option<Expr>, value: Expr },
    Incr { name: String, index: Option<Expr>, delta: i32 },
    Expr(Expr),
    If { cond: Expr, then: Vec<Stmt>, otherwise: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Mode(crate::vm::StepMode),
    Include(String),
    /// A word the engine cannot resolve: executing it logs "Unknown symbol" and
    /// stops the script (`FUN_00423be0`).
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub ret: Type,
    pub params: Vec<(Type, String)>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub globals: Vec<Stmt>,
    pub functions: Vec<Function>,
    pub includes: Vec<String>,
}

impl Program {
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
}
