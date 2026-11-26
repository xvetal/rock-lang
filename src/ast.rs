use std::fmt;

use crate::lexer::Pos;

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
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
    // `Point { x: 1, y: 2 }`, located at the struct name. Each field keeps
    // the position of its name.
    Struct {
        name: String,
        fields: Vec<(String, Pos, Expr)>,
    },
    // `a[i].x` is one node, for the same reason as `Binary`. Each step keeps
    // the position of its `[` or of the field name.
    Access {
        target: Box<Expr>,
        path: Vec<(Pos, Access)>,
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
    For {
        var: String,
        var_pos: Pos,
        iter: Iterable,
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

// What stands after `in` in `for x in ...`.
#[derive(Debug, Clone, PartialEq)]
pub enum Iterable {
    Range { start: Box<Expr>, end: Box<Expr> },
    Array(Box<Expr>),
}

// One step of `a[i][j..].x`.
#[derive(Debug, Clone, PartialEq)]
pub enum Access {
    Index(Expr),
    // `a[start..end]`, where either bound may be left out.
    Slice {
        start: Option<Expr>,
        end: Option<Expr>,
    },
    Field(String),
}

impl Access {
    pub fn exprs(&self) -> impl Iterator<Item = &Expr> {
        let (first, second) = match self {
            Access::Index(index) => (Some(index), None),
            Access::Slice { start, end } => (start.as_ref(), end.as_ref()),
            Access::Field(_) => (None, None),
        };
        first.into_iter().chain(second)
    }
}

// A variable, possibly indexed, which can be changed in place.
pub struct Place<'a> {
    pub name: &'a str,
    pub pos: Pos,
    pub path: &'a [(Pos, Access)],
}

impl Expr {
    pub fn as_place(&self) -> Option<Place<'_>> {
        let (var, path) = match &self.kind {
            ExprKind::Access { target, path } => (&**target, &path[..]),
            _ => (self, &[][..]),
        };
        if path
            .iter()
            .any(|(_, step)| matches!(step, Access::Field(_)))
        {
            return None;
        }
        match &var.kind {
            ExprKind::Var(name) => Some(Place {
                name,
                pos: var.pos,
                path,
            }),
            _ => None,
        }
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
    Struct(String),
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
            Type::Struct(name) => write!(f, "{name}"),
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
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub pos: Pos,
    pub fields: Vec<Field>,
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
        path: Vec<(Pos, Access)>,
        // The operator of a compound assignment such as `x += 1`.
        op: Option<(BinaryOp, Pos)>,
        value: Expr,
        pos: Pos,
    },
    Fn(FnDecl),
    Struct(StructDecl),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub expr: Option<Expr>,
    // Each struct name written in a type, such as `Point` in `[Point]`, so
    // that the checker can report an unknown one where it is written.
    pub type_names: Vec<(String, Pos)>,
}
