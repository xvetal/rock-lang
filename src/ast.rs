use std::fmt;

use crate::lexer::Pos;

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Bool(bool),
    Str(String),
    Var(String),
    Call {
        name: String,
        args: Vec<Expr>,
    },
    // `a - b + c` is one node with `rest` holding `- b` and `+ c`, evaluated
    // left to right. Nesting each operator in the left operand instead would
    // make the tree as deep as the chain is long.
    Binary {
        left: Box<Expr>,
        rest: Vec<(BinaryOp, Pos, Expr)>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Logical {
        left: Box<Expr>,
        rest: Vec<(LogicalOp, Pos, Expr)>,
    },
    Array(Vec<Expr>),
    // `a[i][j]` is one node, for the same reason as `Binary`. Each index keeps
    // the position of its `[`.
    Index {
        target: Box<Expr>,
        indexes: Vec<(Pos, Expr)>,
    },
    Block(Block),
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
    },
    While {
        cond: Box<Expr>,
        body: Block,
    },
    Return(Option<Box<Expr>>),
    Break,
    Continue,
}

impl ExprKind {
    pub fn at(self, pos: Pos) -> Expr {
        Expr { kind: self, pos }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub expr: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Bool,
    Str,
    Unit,
    // The type of `return`, `break` and `continue`; programs cannot write it.
    Never,
    // The type of an expression that failed to check. It fits any type,
    // so one mistake is reported once.
    Error,
    Array(Box<Type>),
    Fn { params: Vec<Type>, ret: Box<Type> },
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "str"),
            Type::Unit => write!(f, "()"),
            Type::Never => write!(f, "!"),
            Type::Error => write!(f, "{{error}}"),
            Type::Array(element) => write!(f, "[{element}]"),
            Type::Fn { params, ret } => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, ")")?;
                if **ret != Type::Unit {
                    write!(f, " -> {ret}")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub mutable: bool,
    pub ty: Type,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub pos: Pos,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Block,
}

impl FnDecl {
    pub fn ty(&self) -> Type {
        Type::Fn {
            params: self.params.iter().map(|p| p.ty.clone()).collect(),
            ret: Box::new(self.ret.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        value: Expr,
        pos: Pos,
    },
    Assign {
        name: String,
        indexes: Vec<(Pos, Expr)>,
        value: Expr,
        pos: Pos,
    },
    Fn(FnDecl),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub expr: Option<Expr>,
}
