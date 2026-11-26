use std::collections::HashMap;
use std::fmt;

use crate::ast::{
    Access, BinaryOp, Block, Expr, ExprKind, FnDecl, Iterable, Program, Stmt, Type, UnaryOp,
};
use crate::lexer::Pos;

#[derive(Debug, Clone, PartialEq)]
pub struct CheckError {
    pub kind: CheckErrorKind,
    pub pos: Pos,
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckErrorKind {
    UndefinedVariable(String),
    UndefinedFunction(String),
    AssignToImmutable(String),
    DuplicateFunction(String),
    Mismatch {
        expected: Type,
        found: Type,
    },
    NotAFunction(Type),
    ArgumentCount {
        name: String,
        expected: usize,
        found: usize,
    },
    CannotCompare(Type),
    EmptyArray,
    NotIndexable(Type),
    NotSliceable(Type),
    AssignToSlice,
    NoLength(Type),
    NotArray(Type),
    PushToValue,
    NotIterable(Type),
    UnknownType(String),
    BuiltinType(String),
    DuplicateStruct(String),
    DuplicateField(String),
    RecursiveStruct(String),
    MissingField {
        field: String,
        name: String,
    },
    UnknownField {
        field: String,
        name: String,
    },
    NoFields(Type),
}

impl CheckErrorKind {
    fn at(self, pos: Pos) -> CheckError {
        CheckError { kind: self, pos }
    }
}

impl fmt::Display for CheckErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CheckErrorKind::UndefinedVariable(name) => write!(f, "undefined variable: {name}"),
            CheckErrorKind::UndefinedFunction(name) => write!(f, "undefined function: {name}"),
            CheckErrorKind::AssignToImmutable(name) => {
                write!(f, "cannot assign to immutable variable: {name}")
            }
            CheckErrorKind::DuplicateFunction(name) => write!(f, "duplicate function: {name}"),
            CheckErrorKind::Mismatch { expected, found } => {
                write!(f, "expected {expected}, found {found}")
            }
            CheckErrorKind::NotAFunction(found) => write!(f, "expected function, found {found}"),
            CheckErrorKind::ArgumentCount {
                name,
                expected,
                found,
            } => {
                let s = if *expected == 1 { "" } else { "s" };
                write!(f, "{name} expects {expected} argument{s}, got {found}")
            }
            CheckErrorKind::CannotCompare(ty) => write!(f, "cannot compare values of type {ty}"),
            CheckErrorKind::EmptyArray => write!(f, "cannot infer the type of an empty array"),
            CheckErrorKind::NotIndexable(ty) => write!(f, "cannot index a value of type {ty}"),
            CheckErrorKind::NotSliceable(ty) => write!(f, "cannot slice a value of type {ty}"),
            CheckErrorKind::AssignToSlice => write!(f, "cannot assign to a slice"),
            CheckErrorKind::NoLength(ty) => write!(f, "expected str or array, found {ty}"),
            CheckErrorKind::NotArray(ty) => write!(f, "expected array, found {ty}"),
            CheckErrorKind::PushToValue => {
                write!(f, "can only push to a variable or an element of one")
            }
            CheckErrorKind::NotIterable(ty) => {
                write!(f, "cannot iterate over a value of type {ty}")
            }
            CheckErrorKind::UnknownType(name) => write!(f, "unknown type: {name}"),
            CheckErrorKind::BuiltinType(name) => write!(f, "{name} is a built-in type"),
            CheckErrorKind::DuplicateStruct(name) => write!(f, "duplicate struct: {name}"),
            CheckErrorKind::DuplicateField(field) => write!(f, "duplicate field: {field}"),
            CheckErrorKind::RecursiveStruct(name) => write!(f, "recursive struct: {name}"),
            CheckErrorKind::MissingField { field, name } => {
                write!(f, "missing field {field} in {name}")
            }
            CheckErrorKind::UnknownField { field, name } => {
                write!(f, "unknown field {field} in {name}")
            }
            CheckErrorKind::NoFields(ty) => write!(f, "{ty} has no fields"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub kind: WarningKind,
    pub pos: Pos,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WarningKind {
    UnusedVariable(String),
    UnusedMut(String),
}

impl WarningKind {
    fn at(self, pos: Pos) -> Warning {
        Warning { kind: self, pos }
    }
}

impl fmt::Display for WarningKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WarningKind::UnusedVariable(name) => write!(f, "unused variable: {name}"),
            WarningKind::UnusedMut(name) => {
                write!(f, "variable does not need to be mutable: {name}")
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env {
    locals: Vec<Local>,
    functions: Vec<(String, Type)>,
    structs: Vec<StructInfo>,
}

#[derive(Debug, Clone)]
struct StructInfo {
    name: String,
    fields: Vec<(String, Type)>,
    pos: Pos,
}

#[derive(Debug, Clone)]
struct Local {
    name: String,
    ty: Type,
    mutable: bool,
    assigned: bool,
    used: bool,
    pos: Pos,
}

impl Local {
    fn new(name: &str, ty: Type, mutable: bool, pos: Pos) -> Self {
        Self {
            name: name.to_string(),
            ty,
            mutable,
            assigned: false,
            used: false,
            pos,
        }
    }

    fn warnings(&self) -> Vec<Warning> {
        let mut warnings = Vec::new();
        if !self.used && !self.name.starts_with('_') {
            warnings.push(WarningKind::UnusedVariable(self.name.clone()).at(self.pos));
        }
        if self.mutable && !self.assigned {
            warnings.push(WarningKind::UnusedMut(self.name.clone()).at(self.pos));
        }
        warnings
    }
}

// The place of each field read in its struct, by the position of the field
// name. Only the checker knows which struct `p` in `p.x` is.
pub type Fields = HashMap<Pos, usize>;

#[derive(Debug)]
pub struct Checked {
    pub env: Env,
    pub errors: Vec<CheckError>,
    pub warnings: Vec<Warning>,
    pub fields: Fields,
}

pub fn check(program: &Program, env: &Env) -> Checked {
    let (structs, struct_errors) = declare_structs(program, env);
    let decls: Vec<&FnDecl> = program
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Fn(decl) => Some(decl),
            _ => None,
        })
        .collect();
    let mut functions = env.functions.clone();
    functions.extend(
        decls
            .iter()
            .map(|decl| (decl.name.clone(), resolve(&decl.ty(), &structs))),
    );

    let mut checker = Checker::new(&functions, &structs, env.locals.clone(), Type::Unit);
    checker.errors.extend(struct_errors);
    for (i, decl) in decls.iter().enumerate() {
        if decls[..i].iter().any(|other| other.name == decl.name) {
            checker.error(
                CheckErrorKind::DuplicateFunction(decl.name.clone()),
                decl.pos,
            );
        }
        checker.function(decl);
    }
    checker.stmts(&program.stmts);
    if let Some(expr) = &program.expr {
        checker.expr(expr);
    }

    let Checker {
        locals,
        mut errors,
        warnings,
        fields,
        ..
    } = checker;
    errors.sort_by_key(|e| e.pos);
    Checked {
        env: Env {
            locals,
            functions,
            structs,
        },
        errors,
        warnings,
        fields,
    }
}

// Adds the structs declared in `program` to those in `env`. A struct can be
// used before its declaration, so names in types are resolved only after
// all of them are known.
fn declare_structs(program: &Program, env: &Env) -> (Vec<StructInfo>, Vec<CheckError>) {
    let mut structs = env.structs.clone();
    let mut errors = Vec::new();
    for stmt in &program.stmts {
        let Stmt::Struct(decl) = stmt else {
            continue;
        };
        for (i, field) in decl.fields.iter().enumerate() {
            if decl.fields[..i]
                .iter()
                .any(|other| other.name == field.name)
            {
                errors.push(CheckErrorKind::DuplicateField(field.name.clone()).at(field.pos));
            }
        }
        let name = decl.name.clone();
        if ["int", "bool", "str"].contains(&name.as_str()) {
            errors.push(CheckErrorKind::BuiltinType(name).at(decl.pos));
        } else if structs.iter().any(|s| s.name == name) {
            errors.push(CheckErrorKind::DuplicateStruct(name).at(decl.pos));
        } else {
            let fields = decl
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.ty.clone()))
                .collect();
            structs.push(StructInfo {
                name,
                fields,
                pos: decl.pos,
            });
        }
    }
    for (name, pos) in &program.type_names {
        if !structs.iter().any(|s| &s.name == name) {
            errors.push(CheckErrorKind::UnknownType(name.clone()).at(*pos));
        }
    }
    let new = env.structs.len();
    for i in new..structs.len() {
        let fields = structs[i]
            .fields
            .iter()
            .map(|(name, ty)| (name.clone(), resolve(ty, &structs)))
            .collect();
        structs[i].fields = fields;
    }
    for info in &structs[new..] {
        let mut seen = Vec::new();
        let recursive = info
            .fields
            .iter()
            .any(|(_, ty)| holds(ty, &info.name, &structs, &mut seen));
        if recursive {
            errors.push(CheckErrorKind::RecursiveStruct(info.name.clone()).at(info.pos));
        }
    }
    (structs, errors)
}

// An unknown struct name is reported where it is written, so here it
// becomes `Error`, which fits any type.
fn resolve(ty: &Type, structs: &[StructInfo]) -> Type {
    match ty {
        Type::Struct(name) if !structs.iter().any(|s| &s.name == name) => Type::Error,
        Type::Array(element) => Type::Array(Box::new(resolve(element, structs))),
        Type::Fn { params, ret } => Type::Fn {
            params: params.iter().map(|p| resolve(p, structs)).collect(),
            ret: Box::new(resolve(ret, structs)),
        },
        ty => ty.clone(),
    }
}

// Whether a value of type `ty` contains a `target` struct other than through
// an array. Such a struct could never be built. `seen` keeps a cycle that
// does not pass through `target` from repeating forever.
fn holds(ty: &Type, target: &str, structs: &[StructInfo], seen: &mut Vec<String>) -> bool {
    let Type::Struct(name) = ty else {
        return false;
    };
    if name == target {
        return true;
    }
    if seen.contains(name) {
        return false;
    }
    seen.push(name.clone());
    structs
        .iter()
        .find(|s| &s.name == name)
        .is_some_and(|info| {
            info.fields
                .iter()
                .any(|(_, ty)| holds(ty, target, structs, seen))
        })
}

// Warnings for top-level variables, once no more code can use them.
pub fn finish(env: &Env) -> Vec<Warning> {
    env.locals.iter().flat_map(Local::warnings).collect()
}

fn value_pos(expr: &Expr) -> Pos {
    match &expr.kind {
        ExprKind::Block(block) => block_value_pos(block, expr.pos),
        _ => expr.pos,
    }
}

fn block_value_pos(block: &Block, fallback: Pos) -> Pos {
    block.expr.as_deref().map_or(fallback, value_pos)
}

fn fits(found: &Type, expected: &Type) -> bool {
    match (found, expected) {
        (Type::Array(found), Type::Array(expected)) => fits(found, expected),
        _ => {
            found == expected
                || matches!(found, Type::Never | Type::Error)
                || *expected == Type::Error
        }
    }
}

// `[]` has the type `[!]`, which fits any array type, as `!` fits any type.
// A variable needs the element type, so it cannot take `[!]` from its value.
fn has_empty_array(ty: &Type) -> bool {
    match ty {
        Type::Array(element) => **element == Type::Never || has_empty_array(element),
        _ => false,
    }
}

// `seen` holds the structs already being checked, as a struct can contain
// itself through an array.
fn comparable(ty: &Type, structs: &[StructInfo], seen: &mut Vec<String>) -> bool {
    match ty {
        Type::Fn { .. } => false,
        Type::Array(element) => comparable(element, structs, seen),
        Type::Struct(name) if !seen.contains(name) => {
            seen.push(name.clone());
            structs.iter().find(|s| &s.name == name).is_none_or(|info| {
                info.fields
                    .iter()
                    .all(|(_, ty)| comparable(ty, structs, seen))
            })
        }
        _ => true,
    }
}

// Built-in functions take arguments of several types, which a function type
// cannot express, so they are not values and are checked one by one.
const BUILTINS: [&str; 4] = ["print", "len", "push", "bytes"];

struct Checker<'a> {
    functions: &'a [(String, Type)],
    structs: &'a [StructInfo],
    locals: Vec<Local>,
    ret: Type,
    diverges: bool,
    errors: Vec<CheckError>,
    warnings: Vec<Warning>,
    fields: Fields,
}

impl<'a> Checker<'a> {
    fn new(
        functions: &'a [(String, Type)],
        structs: &'a [StructInfo],
        locals: Vec<Local>,
        ret: Type,
    ) -> Self {
        Self {
            functions,
            structs,
            locals,
            ret,
            diverges: false,
            errors: Vec::new(),
            warnings: Vec::new(),
            fields: Fields::new(),
        }
    }

    fn error(&mut self, kind: CheckErrorKind, pos: Pos) -> Type {
        self.errors.push(kind.at(pos));
        Type::Error
    }

    fn expect(&mut self, expected: &Type, found: &Type, pos: Pos) {
        if !fits(found, expected) {
            let kind = CheckErrorKind::Mismatch {
                expected: expected.clone(),
                found: found.clone(),
            };
            self.error(kind, pos);
        }
    }

    fn join(&mut self, first: Type, second: Type, pos: Pos) -> Type {
        let arrays = matches!((&first, &second), (Type::Array(_), Type::Array(_)));
        if matches!(first, Type::Never | Type::Error) || arrays && fits(&first, &second) {
            return second;
        }
        self.expect(&first, &second, pos);
        first
    }

    fn function(&mut self, decl: &FnDecl) {
        let params = decl
            .params
            .iter()
            .map(|p| Local::new(&p.name, resolve(&p.ty, self.structs), p.mutable, p.pos))
            .collect();
        let ret = resolve(&decl.ret, self.structs);
        let mut body = Checker::new(self.functions, self.structs, params, ret.clone());
        let ty = body.block(&decl.body);
        body.expect(&ret, &ty, block_value_pos(&decl.body, decl.pos));
        body.end_scope(0);
        self.errors.extend(body.errors);
        self.warnings.extend(body.warnings);
        self.fields.extend(body.fields);
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Let {
                    name,
                    mutable,
                    ty,
                    value,
                    pos,
                } => {
                    let found = self.expr(value);
                    let ty = match ty {
                        Some(ty) => {
                            let ty = resolve(ty, self.structs);
                            self.expect(&ty, &found, value_pos(value));
                            ty
                        }
                        None if has_empty_array(&found) => {
                            self.error(CheckErrorKind::EmptyArray, value_pos(value))
                        }
                        None => found,
                    };
                    self.locals.push(Local::new(name, ty, *mutable, *pos));
                }
                Stmt::Assign {
                    name,
                    path,
                    op: None,
                    value,
                    pos,
                } => {
                    let found = self.expr(value);
                    let ty = self.place(name, path, *pos);
                    self.expect(&ty, &found, value_pos(value));
                }
                Stmt::Assign {
                    name,
                    path,
                    op: Some((op, _)),
                    value,
                    pos,
                } => {
                    let ty = self.place(name, path, *pos);
                    self.compound(op, ty, *pos, value);
                }
                Stmt::Expr(expr) => {
                    self.expr(expr);
                }
                Stmt::Fn(_) | Stmt::Struct(_) => {}
            }
        }
    }

    fn block(&mut self, block: &Block) -> Type {
        let outer = self.locals.len();
        self.stmts(&block.stmts);
        let ty = match &block.expr {
            Some(value) => self.expr(value),
            None if self.diverges => Type::Never,
            None => Type::Unit,
        };
        self.end_scope(outer);
        ty
    }

    fn end_scope(&mut self, outer: usize) {
        let ended = self.locals.split_off(outer);
        self.warnings.extend(ended.iter().flat_map(Local::warnings));
    }

    fn local(&mut self, name: &str) -> Option<&mut Local> {
        self.locals.iter_mut().rfind(|local| local.name == name)
    }

    fn lookup(&mut self, name: &str) -> Option<Type> {
        if let Some(local) = self.local(name) {
            local.used = true;
            return Some(local.ty.clone());
        }
        let (_, ty) = self.functions.iter().rfind(|(n, _)| n == name)?;
        Some(ty.clone())
    }

    fn expr(&mut self, expr: &Expr) -> Type {
        let pos = expr.pos;
        match &expr.kind {
            ExprKind::Int(_) => Type::Int,
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Str(_) => Type::Str,
            ExprKind::Var(name) => self.lookup(name).unwrap_or_else(|| {
                self.error(CheckErrorKind::UndefinedVariable(name.clone()), pos)
            }),
            ExprKind::Call { name, args } => {
                let callee = match self.lookup(name) {
                    Some(ty) => ty,
                    None if BUILTINS.contains(&name.as_str()) => {
                        return self.builtin(name, args, pos);
                    }
                    None => self.error(CheckErrorKind::UndefinedFunction(name.clone()), pos),
                };
                self.call(name, callee, args, pos)
            }
            ExprKind::Unary { op, operand } => {
                let ty = match op {
                    UnaryOp::Neg => Type::Int,
                    UnaryOp::Not => Type::Bool,
                };
                let found = self.expr(operand);
                self.expect(&ty, &found, value_pos(operand));
                ty
            }
            ExprKind::Binary { left, rest } => {
                let mut ty = self.expr(left);
                let mut ty_pos = value_pos(left);
                for (op, pos, right) in rest {
                    ty = self.binary(op, ty, ty_pos, right, *pos);
                    ty_pos = *pos;
                }
                ty
            }
            ExprKind::Logical { left, rest } => {
                let found = self.expr(left);
                self.expect(&Type::Bool, &found, value_pos(left));
                let diverges = self.diverges;
                for (_, _, right) in rest {
                    let found = self.expr(right);
                    self.expect(&Type::Bool, &found, value_pos(right));
                }
                self.diverges = diverges;
                Type::Bool
            }
            ExprKind::Array(items) => {
                let Some((first, rest)) = items.split_first() else {
                    return Type::Array(Box::new(Type::Never));
                };
                let mut ty = self.expr(first);
                for item in rest {
                    let found = self.expr(item);
                    ty = self.join(ty, found, value_pos(item));
                }
                Type::Array(Box::new(ty))
            }
            ExprKind::Struct { name, fields } => self.struct_literal(name, fields, pos),
            ExprKind::Access { target, path } => {
                let ty = self.expr(target);
                self.access(ty, value_pos(target), path)
            }
            ExprKind::Block(block) => self.block(block),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let found = self.expr(cond);
                self.expect(&Type::Bool, &found, value_pos(cond));
                let diverges = self.diverges;
                let then_ty = self.block(then_branch);
                match else_branch {
                    Some(else_branch) => {
                        let then_diverges = self.diverges;
                        self.diverges = diverges;
                        let else_ty = self.expr(else_branch);
                        self.diverges = then_diverges && self.diverges;
                        self.join(then_ty, else_ty, value_pos(else_branch))
                    }
                    None => {
                        self.expect(&Type::Unit, &then_ty, block_value_pos(then_branch, pos));
                        self.diverges = diverges;
                        Type::Unit
                    }
                }
            }
            ExprKind::While { cond, body } => {
                let found = self.expr(cond);
                self.expect(&Type::Bool, &found, value_pos(cond));
                let diverges = self.diverges;
                let body_ty = self.block(body);
                self.expect(&Type::Unit, &body_ty, block_value_pos(body, pos));
                self.diverges = diverges;
                Type::Unit
            }
            ExprKind::For {
                var,
                var_pos,
                iter,
                body,
            } => {
                let ty = self.iterable(iter);
                let diverges = self.diverges;
                let outer = self.locals.len();
                self.locals.push(Local::new(var, ty, false, *var_pos));
                let body_ty = self.block(body);
                self.expect(&Type::Unit, &body_ty, block_value_pos(body, pos));
                self.end_scope(outer);
                self.diverges = diverges;
                Type::Unit
            }
            ExprKind::Return(value) => {
                let (found, pos) = match value {
                    Some(value) => (self.expr(value), value_pos(value)),
                    None => (Type::Unit, pos),
                };
                let ret = self.ret.clone();
                self.expect(&ret, &found, pos);
                self.diverges = true;
                Type::Never
            }
            ExprKind::Break | ExprKind::Continue => {
                self.diverges = true;
                Type::Never
            }
        }
    }

    fn struct_literal(&mut self, name: &str, fields: &[(String, Pos, Expr)], pos: Pos) -> Type {
        let structs = self.structs;
        let Some(info) = structs.iter().find(|s| s.name == name) else {
            self.error(CheckErrorKind::UnknownType(name.to_string()), pos);
            for (_, _, value) in fields {
                self.expr(value);
            }
            return Type::Error;
        };
        for (i, (field, field_pos, value)) in fields.iter().enumerate() {
            let found = self.expr(value);
            if fields[..i].iter().any(|(other, _, _)| other == field) {
                self.error(CheckErrorKind::DuplicateField(field.clone()), *field_pos);
            } else if let Some((_, ty)) = info.fields.iter().find(|(n, _)| n == field) {
                self.expect(ty, &found, value_pos(value));
            } else {
                let kind = CheckErrorKind::UnknownField {
                    field: field.clone(),
                    name: name.to_string(),
                };
                self.error(kind, *field_pos);
            }
        }
        for (field, _) in &info.fields {
            if !fields.iter().any(|(given, _, _)| given == field) {
                let kind = CheckErrorKind::MissingField {
                    field: field.clone(),
                    name: name.to_string(),
                };
                self.error(kind, pos);
            }
        }
        Type::Struct(name.to_string())
    }

    fn call(&mut self, name: &str, callee: Type, args: &[Expr], pos: Pos) -> Type {
        let Type::Fn { params, ret } = callee else {
            if callee != Type::Error {
                self.error(CheckErrorKind::NotAFunction(callee), pos);
            }
            self.args(args);
            return Type::Error;
        };
        if args.len() != params.len() {
            let kind = CheckErrorKind::ArgumentCount {
                name: name.to_string(),
                expected: params.len(),
                found: args.len(),
            };
            self.error(kind, pos);
            self.args(args);
            return *ret;
        }
        for (arg, param) in args.iter().zip(&params) {
            let found = self.expr(arg);
            self.expect(param, &found, value_pos(arg));
        }
        *ret
    }

    fn args(&mut self, args: &[Expr]) {
        for arg in args {
            self.expr(arg);
        }
    }

    fn binary(
        &mut self,
        op: &BinaryOp,
        left_ty: Type,
        left_pos: Pos,
        right: &Expr,
        pos: Pos,
    ) -> Type {
        let right_ty = self.expr(right);
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let ty = self.join(left_ty, right_ty, value_pos(right));
            if !comparable(&ty, self.structs, &mut Vec::new()) {
                self.error(CheckErrorKind::CannotCompare(ty), pos);
            }
            return Type::Bool;
        }
        let strings = matches!(
            op,
            BinaryOp::Add | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) && (left_ty == Type::Str || right_ty == Type::Str);
        let operand = if strings { Type::Str } else { Type::Int };
        self.expect(&operand, &left_ty, left_pos);
        self.expect(&operand, &right_ty, value_pos(right));
        match op {
            BinaryOp::Add => operand,
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => Type::Int,
            _ => Type::Bool,
        }
    }

    // Unlike in `binary`, only the place decides whether the operands are
    // strings, so `x += "a"` with an int `x` blames the value.
    fn compound(&mut self, op: &BinaryOp, ty: Type, pos: Pos, value: &Expr) {
        let operand = match (op, ty) {
            (BinaryOp::Add, Type::Str) => Type::Str,
            (_, ty) => {
                self.expect(&Type::Int, &ty, pos);
                Type::Int
            }
        };
        let found = self.expr(value);
        self.expect(&operand, &found, value_pos(value));
    }

    // `pos` locates the value of type `ty`, so an error can point at the
    // part of `a[i][j]` that is not an array.
    fn access(&mut self, mut ty: Type, mut pos: Pos, path: &[(Pos, Access)]) -> Type {
        for (step_pos, step) in path {
            ty = match (ty, step) {
                (Type::Array(element), Access::Index(_)) => *element,
                (ty @ (Type::Str | Type::Array(_)), Access::Slice { .. }) => ty,
                (Type::Struct(name), Access::Field(field)) => self.field(&name, field, *step_pos),
                (ty @ (Type::Error | Type::Never), _) => ty,
                (other, Access::Index(_)) => self.error(CheckErrorKind::NotIndexable(other), pos),
                (other, Access::Slice { .. }) => {
                    self.error(CheckErrorKind::NotSliceable(other), pos)
                }
                (other, Access::Field(_)) => self.error(CheckErrorKind::NoFields(other), *step_pos),
            };
            for bound in step.exprs() {
                let found = self.expr(bound);
                self.expect(&Type::Int, &found, value_pos(bound));
            }
            pos = *step_pos;
        }
        ty
    }

    fn field(&mut self, name: &str, field: &str, pos: Pos) -> Type {
        let structs = self.structs;
        let info = structs
            .iter()
            .find(|s| s.name == name)
            .expect("a struct type names a declared struct");
        let Some(place) = info.fields.iter().position(|(n, _)| n == field) else {
            let kind = CheckErrorKind::UnknownField {
                field: field.to_string(),
                name: name.to_string(),
            };
            return self.error(kind, pos);
        };
        self.fields.insert(pos, place);
        info.fields[place].1.clone()
    }

    // The type of a variable, possibly indexed, that is about to change.
    // A slice is a new value, so a change to it would be lost.
    fn place(&mut self, name: &str, path: &[(Pos, Access)], pos: Pos) -> Type {
        let ty = match self.local(name) {
            Some(local) => {
                local.assigned = true;
                let (mutable, ty) = (local.mutable, local.ty.clone());
                if !mutable {
                    self.error(CheckErrorKind::AssignToImmutable(name.to_string()), pos);
                }
                ty
            }
            None => self.error(CheckErrorKind::UndefinedVariable(name.to_string()), pos),
        };
        let slice = path
            .iter()
            .find(|(_, step)| matches!(step, Access::Slice { .. }));
        if let Some((bracket, _)) = slice {
            self.error(CheckErrorKind::AssignToSlice, *bracket);
        }
        self.access(ty, pos, path)
    }

    // The type of the loop variable.
    fn iterable(&mut self, iter: &Iterable) -> Type {
        let array = match iter {
            Iterable::Range { start, end } => {
                for bound in [start, end] {
                    let found = self.expr(bound);
                    self.expect(&Type::Int, &found, value_pos(bound));
                }
                return Type::Int;
            }
            Iterable::Array(array) => array,
        };
        let ty = self.expr(array);
        let pos = value_pos(array);
        if has_empty_array(&ty) {
            return self.error(CheckErrorKind::EmptyArray, pos);
        }
        match ty {
            Type::Array(element) => *element,
            ty @ (Type::Error | Type::Never) => ty,
            other => self.error(CheckErrorKind::NotIterable(other), pos),
        }
    }

    fn builtin(&mut self, name: &str, args: &[Expr], pos: Pos) -> Type {
        if name == "push" {
            return self.push(args, pos);
        }
        let ret = match name {
            "len" => Type::Int,
            "bytes" => Type::Array(Box::new(Type::Int)),
            _ => Type::Unit,
        };
        let [arg] = args else {
            let kind = CheckErrorKind::ArgumentCount {
                name: name.to_string(),
                expected: 1,
                found: args.len(),
            };
            self.error(kind, pos);
            self.args(args);
            return ret;
        };
        let ty = self.expr(arg);
        match name {
            "len" => {
                let sized = matches!(ty, Type::Str | Type::Array(_) | Type::Error | Type::Never);
                if !sized {
                    self.error(CheckErrorKind::NoLength(ty), value_pos(arg));
                }
            }
            "bytes" => self.expect(&Type::Str, &ty, value_pos(arg)),
            _ => {}
        }
        ret
    }

    fn push(&mut self, args: &[Expr], pos: Pos) -> Type {
        let [target, value] = args else {
            let kind = CheckErrorKind::ArgumentCount {
                name: "push".to_string(),
                expected: 2,
                found: args.len(),
            };
            self.error(kind, pos);
            self.args(args);
            return Type::Unit;
        };
        let Some(place) = target.as_place() else {
            self.error(CheckErrorKind::PushToValue, value_pos(target));
            self.args(args);
            return Type::Unit;
        };
        let ty = self.place(place.name, place.path, place.pos);
        let found = self.expr(value);
        match ty {
            Type::Array(element) => self.expect(&element, &found, value_pos(value)),
            Type::Error | Type::Never => {}
            other => {
                self.error(CheckErrorKind::NotArray(other), value_pos(target));
            }
        }
        Type::Unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn parse(input: &str) -> Program {
        Parser::new(tokenize(input).unwrap()).parse().unwrap()
    }

    fn check_program(input: &str) -> Checked {
        let mut checked = check(&parse(input), &Env::default());
        checked.warnings.extend(finish(&checked.env));
        checked
    }

    fn warnings(input: &str) -> Vec<String> {
        let checked = check_program(input);
        assert_eq!(checked.errors, [], "{input}");
        checked.warnings.iter().map(|w| w.to_string()).collect()
    }

    fn errors(input: &str) -> Vec<String> {
        let checked = check_program(input);
        checked.errors.iter().map(|e| e.to_string()).collect()
    }

    fn error(input: &str) -> String {
        let [error] = errors(input).try_into().unwrap_or_else(|errors| {
            panic!("expected one error in {input}, got {errors:?}");
        });
        error
    }

    #[test]
    fn well_typed_programs() {
        for input in [
            "(1 + 2 * 3 < 7) == true",
            "let mut x = 1; x = x + 1; x",
            "if true { 1 } else { 2 }",
            "if true { print(1); }",
            "print(1) == print(2)",
            "let mut i = 0; while i < 3 { i = i + 1; if i == 2 { break; } }",
            "fn f() {} let g = f; g()",
            "fn print(x: bool) -> bool { x } print(true) && true",
            "fn twice(f: fn(int) -> int, x: int) -> int { f(f(x)) }
             fn inc(x: int) -> int { x + 1 }
             twice(inc, 1)",
            "fn even(n: int) -> bool { if n == 0 { true } else { odd(n - 1) } }
             fn odd(n: int) -> bool { if n == 0 { false } else { even(n - 1) } }",
            "let mut x = 1; { let x = true; } x = 2;",
            "fn f(n: int) -> int { let mut n = n; n = n + 1; n }",
            r#"fn greet(name: str) -> str { "hi, " + name } greet("Rock") == "hi, Rock""#,
            r#"let mut s = ""; s = s + "a"; print(s); s != "b""#,
            r#""a" < "b" && "b" >= "a""#,
            r#"len("abc") + 1"#,
            "fn len(n: int) -> int { n } len(1)",
            "let a = [[1, 2], [3]]; a[1][0] + len(a) + len(a[0])",
            "fn f(xs: [str]) -> str { xs[0] } f([\"a\"]) == \"a\"",
            "[1] == [2] && [[true]] != [[false]]",
            "fn f(n: int) -> [int] { [n, return [n]] }",
            "let mut a = [[1], [2]]; a[1][0] = a[0][0]; a",
            "let mut a: [int] = []; a = []; a = [1]; a",
            "let mut a: [[int]] = [[]]; push(a, []); push(a[1], 2); a",
            "fn push(n: int) -> int { n } push(1)",
            "let a: [[str]] = [[], [\"x\"]]; a",
            "fn f(xs: [int]) -> [int] { if len(xs) == 0 { return []; } xs } f([])",
            "fn g() -> [int] { [] }",
            "let a = if true { [] } else { [1] }; a[0] + 1",
            "let a = [[1], []]; let n: int = len(a); a == [] || n == 2",
            "fn f(mut xs: [int]) -> [int] { xs[0] = 1; xs }",
            r#"let s = "hello"; let t: str = s[1..3] + s[..1] + s[3..] + s[..]; t"#,
            "let a = [[1, 2], [3]]; let b: [[int]] = a[1..]; a[..1][0][1..][0] + len(b)",
            "let mut a = [[1, 2]]; a[0] = a[0][..1]; push(a, a[0][1..]); a",
            "[][0..0] == [1]",
            r#"bytes("ab")[1] - bytes("a")[0] + len(bytes(""))"#,
            "fn bytes(n: int) -> int { n } bytes(1)",
            "let mut sum = 0; for i in 0..len([1, 2]) { if i == 1 { continue; } sum = sum + i; } sum",
            "fn root(n: int) -> int { for i in 0..n { if i * i >= n { return i; } } n }",
            "let i = true; for i in 0..1 { print(i + 1); } i && true",
            "let a = [[1], [2, 3]]; let mut n = 0; for row in a { for x in row { n = n + x; } } n",
            r#"let mut s = ""; for w in ["a", "b"] { s = s + w; } s"#,
            r#"for b in bytes("ab") { print(b + 1); }"#,
            "let a: [int] = []; for _ in a {}",
            "fn first(xs: [int]) -> int { for x in xs { return x; } 0 }",
            "let mut x = 10; x += 1; x -= 2; x *= 3; x /= 4; x %= 5; x",
            r#"let mut s = "a"; s += "b" + s; s"#,
            r#"let mut a = [["a"]]; a[0][0] += "b"; let mut n = [1]; n[0] *= len(a); n"#,
            "struct P { x: int, y: int } P { y: 1, x: 2 } == P { x: 2, y: 1 }",
            r#"let t = T { kids: [T { kids: [], name: "a" }], name: "b" }; struct T { kids: [T], name: str } t == t"#,
            "struct P { x: int } fn f(p: P) -> [P] { [p, P { x: 1 }] } f(P { x: 0 })",
            "struct E {} let e: E = E {}; if e == (E {}) { print(e); }",
            "struct A { b: B } struct B { f: fn(A) -> A } fn g(a: A) -> A { a } A { b: B { f: g } }",
            "struct P { x: int, kids: [P] } let p = P { x: 1, kids: [] };
             p.x + len(p.kids) + P { x: 2, kids: [p] }.kids[0].x",
            r#"struct S { name: str } fn f(s: S) -> str { s.name[1..] } f(S { name: "ab" })"#,
            "struct P { f: fn(int) -> int } fn g(n: int) -> int { n } let f = P { f: g }.f; f(1)",
        ] {
            assert_eq!(check_program(input).errors, [], "{input}");
        }
    }

    #[test]
    fn used_variables_are_not_warned() {
        for input in [
            "let mut x = 1; x = 2; x",
            "let mut i = 0; while i < 3 { i = i + 1; }",
            "fn f(mut n: int) -> int { n = n + 1; n }",
            "let x = 1; { let mut y = x; y = 2; y }",
            "fn f(g: fn()) { g() }",
            "let mut a = [1]; a[0] = 2; a",
            "let mut a = [1]; push(a, 2); a",
            "let mut x = 0; x += x;",
            "let mut a = [1]; a[0] += 1; a",
        ] {
            assert_eq!(warnings(input), [] as [String; 0], "{input}");
        }
    }

    #[test]
    fn unused_mut() {
        assert_eq!(
            warnings("let mut x = 1; x"),
            ["1:9: variable does not need to be mutable: x"]
        );
        assert_eq!(
            warnings("fn f(mut n: int) -> int { n }"),
            ["1:10: variable does not need to be mutable: n"]
        );
        assert_eq!(
            warnings("let mut x = 1; let mut x = x; x = 3; x"),
            ["1:9: variable does not need to be mutable: x"]
        );
        assert_eq!(
            warnings("let mut x = 1; { let mut y = x; y }"),
            [
                "1:26: variable does not need to be mutable: y",
                "1:9: variable does not need to be mutable: x"
            ]
        );
    }

    #[test]
    fn unused_variables() {
        assert_eq!(warnings("let x = 1;"), ["1:5: unused variable: x"]);
        assert_eq!(warnings("fn f(n: int) {}"), ["1:6: unused variable: n"]);
        assert_eq!(
            warnings("let mut x = 1; x = 2;"),
            ["1:9: unused variable: x"]
        );
        assert_eq!(
            warnings("let x = 1; let x = 2; x"),
            ["1:5: unused variable: x"]
        );
        assert_eq!(
            warnings("let mut x = 1;"),
            [
                "1:9: unused variable: x",
                "1:9: variable does not need to be mutable: x"
            ]
        );
    }

    #[test]
    fn compound_assignment_is_not_a_use() {
        assert_eq!(
            warnings("let mut x = 0; x += 1;"),
            ["1:9: unused variable: x"]
        );
    }

    #[test]
    fn underscore_allows_unused_variables() {
        for input in ["let _x = 1;", "let _ = 1;", "fn f(_n: int) {}"] {
            assert_eq!(warnings(input), [] as [String; 0], "{input}");
        }
        assert_eq!(
            warnings("let mut _x = 1;"),
            ["1:9: variable does not need to be mutable: _x"]
        );
    }

    #[test]
    fn for_variable_warnings() {
        assert_eq!(warnings("for i in 0..3 {}"), ["1:5: unused variable: i"]);
        assert_eq!(warnings("for _ in 0..3 {}"), [] as [String; 0]);
        assert_eq!(
            warnings("for i in 0..3 { let x = i; }"),
            ["1:21: unused variable: x"]
        );
        assert_eq!(warnings("for x in [1] {}"), ["1:5: unused variable: x"]);
    }

    #[test]
    fn top_level_warnings_wait_for_finish() {
        let checked = check(&parse("let mut x = 1; x"), &Env::default());
        assert_eq!(checked.warnings, []);
        assert_eq!(finish(&checked.env).len(), 1);
        let checked = check(&parse("x = 2;"), &checked.env);
        assert_eq!(checked.errors, []);
        assert_eq!(finish(&checked.env), []);
    }

    #[test]
    fn return_break_and_continue_fit_any_type() {
        for input in [
            "fn f(n: int) -> int { if n < 0 { return 0 } else { n } }",
            "fn f() -> int { return 1; }",
            "fn f() -> int { 1 + return 2; }",
            "fn f() -> int { { { return 1; } } }",
            "fn f() -> int { while true { return 1; } 0 }",
            "while true { 1 + break; }",
            "while true { let x = if true { continue } else { 1 }; }",
        ] {
            assert_eq!(check_program(input).errors, [], "{input}");
        }
    }

    #[test]
    fn undefined_variables() {
        assert_eq!(error("x"), "1:1: undefined variable: x");
        assert_eq!(error("y = 1;"), "1:1: undefined variable: y");
        assert_eq!(error("{ let a = 1; } a"), "1:16: undefined variable: a");
        assert_eq!(error("let x = x;"), "1:9: undefined variable: x");
        assert_eq!(
            error("let y = x; let x = 1; y"),
            "1:9: undefined variable: x"
        );
        assert_eq!(error("if false { y }"), "1:12: undefined variable: y");
    }

    #[test]
    fn functions_do_not_see_outer_variables() {
        assert_eq!(
            error("let base = 10; fn f(x: int) -> int { base + x } f(1)"),
            "1:38: undefined variable: base"
        );
        assert_eq!(
            error("fn f() -> int { y } { let y = 1; f() }"),
            "1:17: undefined variable: y"
        );
        assert_eq!(
            error("let count = 0; fn reset() { count = 0; }"),
            "1:29: undefined variable: count"
        );
    }

    #[test]
    fn function_name_errors() {
        assert_eq!(error("f()"), "1:1: undefined function: f");
        assert_eq!(error("fn f() {} fn f() {}"), "1:14: duplicate function: f");
        assert_eq!(error("fn f() {} f = 1;"), "1:11: undefined variable: f");
        assert_eq!(error("print(1, 2)"), "1:1: print expects 1 argument, got 2");
    }

    #[test]
    fn operand_types() {
        assert_eq!(error("1 +\n  true"), "2:3: expected int, found bool");
        assert_eq!(error("-false"), "1:2: expected int, found bool");
        assert_eq!(error("!1"), "1:2: expected bool, found int");
        assert_eq!(error("false < 1"), "1:1: expected int, found bool");
        assert_eq!(error("true && 1"), "1:9: expected bool, found int");
        assert_eq!(error("1 || true"), "1:1: expected bool, found int");
        assert_eq!(error("print(1) + 1"), "1:1: expected int, found ()");
        assert_eq!(
            error("let x = true;\nx + 1"),
            "2:1: expected int, found bool"
        );
        assert_eq!(error("1 + { 2; true }"), "1:10: expected int, found bool");
    }

    #[test]
    fn comparison_types() {
        assert_eq!(error("1 == true"), "1:6: expected int, found bool");
        assert_eq!(
            error("fn f() {} f == f"),
            "1:13: cannot compare values of type fn()"
        );
    }

    #[test]
    fn condition_and_branch_types() {
        assert_eq!(error("if 1 {}"), "1:4: expected bool, found int");
        assert_eq!(error("while 1 {}"), "1:7: expected bool, found int");
        assert_eq!(error("if true { 1 }"), "1:11: expected (), found int");
        assert_eq!(
            error("if true { 1 } else { false }"),
            "1:22: expected int, found bool"
        );
        assert_eq!(error("while true { 1 }"), "1:14: expected (), found int");
    }

    #[test]
    fn assignment_keeps_type() {
        assert_eq!(
            error("let mut x = 1; x = true;"),
            "1:20: expected int, found bool"
        );
    }

    #[test]
    fn assignment_needs_mut() {
        assert_eq!(
            error("let x = 1; x = 2;"),
            "1:12: cannot assign to immutable variable: x"
        );
        assert_eq!(
            error("fn f(n: int) { n = 1; }"),
            "1:16: cannot assign to immutable variable: n"
        );
        assert_eq!(
            error("fn f(mut n: int) { let n = 1; n = 2; }"),
            "1:31: cannot assign to immutable variable: n"
        );
        assert_eq!(
            error("let mut x = 1; { let x = 2; x = 3; }"),
            "1:29: cannot assign to immutable variable: x"
        );
    }

    #[test]
    fn return_types() {
        assert_eq!(
            error("fn f() -> int { true }"),
            "1:17: expected int, found bool"
        );
        assert_eq!(error("fn f() -> int {}"), "1:4: expected int, found ()");
        assert_eq!(
            error("fn f() -> int { return; }"),
            "1:17: expected int, found ()"
        );
        assert_eq!(
            error("fn f() { return 1; }"),
            "1:17: expected (), found int"
        );
        assert_eq!(
            error("fn f() -> int { while true { return 1; } }"),
            "1:17: expected int, found ()"
        );
    }

    #[test]
    fn call_errors() {
        assert_eq!(
            error("fn add(a: int, b: int) -> int { a + b } add(1)"),
            "1:41: add expects 2 arguments, got 1"
        );
        assert_eq!(
            error("fn add(a: int, b: int) -> int { a + b } add(1, true)"),
            "1:48: expected int, found bool"
        );
        assert_eq!(
            error("let f = 2; f()"),
            "1:12: expected function, found int"
        );
        assert_eq!(
            error("let print = 1; print(2)"),
            "1:16: expected function, found int"
        );
        assert_eq!(
            error(
                "fn add(a: int, b: int) -> int { a + b } fn h(f: fn(int) -> int) -> int { f(1) } h(add)"
            ),
            "1:83: expected fn(int) -> int, found fn(int, int) -> int"
        );
    }

    #[test]
    fn several_errors() {
        assert_eq!(
            errors("false < true"),
            [
                "1:1: expected int, found bool",
                "1:9: expected int, found bool"
            ]
        );
        assert_eq!(
            errors("let a = b;\nfn f() -> int { true }\na = 1;"),
            [
                "1:9: undefined variable: b",
                "2:17: expected int, found bool",
                "3:1: cannot assign to immutable variable: a"
            ]
        );
        assert_eq!(
            errors("fn f(a: int) -> int { a } f(1, y)"),
            [
                "1:27: f expects 1 argument, got 2",
                "1:32: undefined variable: y"
            ]
        );
        assert_eq!(
            errors("print(y, 1)"),
            [
                "1:1: print expects 1 argument, got 2",
                "1:7: undefined variable: y"
            ]
        );
    }

    #[test]
    fn errors_do_not_cascade() {
        for (input, expected) in [
            ("let x = y; x + 1", "1:9: undefined variable: y"),
            ("let x = y; x()", "1:9: undefined variable: y"),
            ("f(1) + 1", "1:1: undefined function: f"),
            ("let f = g; f(1, 2) == true", "1:9: undefined variable: g"),
            ("let mut x = y; x = true;", "1:13: undefined variable: y"),
            (
                "if true { y } else { 1 } + 1",
                "1:11: undefined variable: y",
            ),
        ] {
            assert_eq!(error(input), expected);
        }
    }

    #[test]
    fn warnings_come_with_errors() {
        let checked = check_program("let x = 1; y");
        let warnings: Vec<String> = checked.warnings.iter().map(|w| w.to_string()).collect();
        assert_eq!(warnings, ["1:5: unused variable: x"]);
        assert_eq!(checked.errors.len(), 1);
    }

    #[test]
    fn string_operand_types() {
        assert_eq!(error(r#""a" + 1"#), "1:7: expected str, found int");
        assert_eq!(error(r#"1 + "a""#), "1:1: expected str, found int");
        assert_eq!(error(r#""a" == 1"#), "1:8: expected str, found int");
        assert_eq!(error(r#"-"a""#), "1:2: expected int, found str");
        assert_eq!(error(r#""a" < 1"#), "1:7: expected str, found int");
        assert_eq!(error(r#""a" % 2"#), "1:1: expected int, found str");
        assert_eq!(
            errors(r#""a" - "b""#),
            [
                "1:1: expected int, found str",
                "1:7: expected int, found str"
            ]
        );
    }

    #[test]
    fn len_errors() {
        assert_eq!(error("len(1)"), "1:5: expected str or array, found int");
        assert_eq!(
            error(r#"len("a", "b")"#),
            "1:1: len expects 1 argument, got 2"
        );
        assert_eq!(error("let f = len;"), "1:9: undefined variable: len");
    }

    #[test]
    fn array_errors() {
        assert_eq!(error("[1, true]"), "1:5: expected int, found bool");
        assert_eq!(
            error("let a = [];"),
            "1:9: cannot infer the type of an empty array"
        );
        assert_eq!(
            error("let mut a = [[]]; a[0] = [1];"),
            "1:13: cannot infer the type of an empty array"
        );
        assert_eq!(error("1[0]"), "1:1: cannot index a value of type int");
        assert_eq!(error("[1][0][0]"), "1:4: cannot index a value of type int");
        assert_eq!(error("\"ab\"[0]"), "1:1: cannot index a value of type str");
        assert_eq!(error("[1][true]"), "1:5: expected int, found bool");
        assert_eq!(
            error("fn f() {} [f] == [f]"),
            "1:15: cannot compare values of type [fn()]"
        );
        assert_eq!(
            error("fn f(xs: [int]) {} f([true])"),
            "1:22: expected [int], found [bool]"
        );
        assert_eq!(
            error("let a = [y]; a[0] + 1"),
            "1:10: undefined variable: y"
        );
    }

    #[test]
    fn index_assignment_errors() {
        assert_eq!(
            error("let a = [1]; a[0] = 2; a"),
            "1:14: cannot assign to immutable variable: a"
        );
        assert_eq!(
            error("let mut a = [1]; a[0] = true; a"),
            "1:25: expected int, found bool"
        );
        assert_eq!(
            error("let mut a = [1]; a[true] = 2; a"),
            "1:20: expected int, found bool"
        );
        assert_eq!(
            error("let mut x = 1; x[0] = 2; x"),
            "1:16: cannot index a value of type int"
        );
        assert_eq!(error("b[0] = 1;"), "1:1: undefined variable: b");
    }

    #[test]
    fn let_type_errors() {
        assert_eq!(
            error("let a: int = true; a"),
            "1:14: expected int, found bool"
        );
        assert_eq!(
            error("let a: [int] = [true]; a"),
            "1:16: expected [int], found [bool]"
        );
        assert_eq!(
            error("let a: [int] = [1]; a[0] + true"),
            "1:28: expected int, found bool"
        );
    }

    #[test]
    fn push_errors() {
        assert_eq!(
            error("let a = [1]; push(a, 2); a"),
            "1:19: cannot assign to immutable variable: a"
        );
        assert_eq!(
            error("let a = [[1]]; push(a[0], 2); a"),
            "1:21: cannot assign to immutable variable: a"
        );
        assert_eq!(
            error("let mut a = [1]; push(a, true); a"),
            "1:26: expected int, found bool"
        );
        assert_eq!(
            error("let mut n = 1; push(n, 2); n"),
            "1:21: expected array, found int"
        );
        assert_eq!(
            error("push([1], 2);"),
            "1:6: can only push to a variable or an element of one"
        );
        assert_eq!(error("push(b, 1);"), "1:6: undefined variable: b");
        assert_eq!(
            error("let mut a = [1]; push(a); a"),
            "1:18: push expects 2 arguments, got 1"
        );
    }

    #[test]
    fn slice_errors() {
        assert_eq!(error("1[0..1]"), "1:1: cannot slice a value of type int");
        assert_eq!(error("[1][0][..]"), "1:4: cannot slice a value of type int");
        assert_eq!(error(r#""ab"[true..]"#), "1:6: expected int, found bool");
        assert_eq!(error(r#""ab"[..""]"#), "1:8: expected int, found str");
        assert_eq!(
            error("let a: [bool] = [1, 2][..1];"),
            "1:23: expected [bool], found [int]"
        );
    }

    #[test]
    fn slice_assignment_errors() {
        assert_eq!(
            error(r#"let mut s = "abc"; s[1..] = "x"; s"#),
            "1:21: cannot assign to a slice"
        );
        assert_eq!(
            error("let mut a = [[1]]; a[..1][0] = [2]; a"),
            "1:21: cannot assign to a slice"
        );
        assert_eq!(
            error("let mut a = [1]; push(a[0..1], 2); a"),
            "1:24: cannot assign to a slice"
        );
        assert_eq!(
            errors(r#"let s = "abc"; s[..1] = "x"; s"#),
            [
                "1:16: cannot assign to immutable variable: s",
                "1:17: cannot assign to a slice"
            ]
        );
    }

    #[test]
    fn bytes_errors() {
        assert_eq!(error("bytes(1)"), "1:7: expected str, found int");
        assert_eq!(error(r#"bytes(["a"])"#), "1:7: expected str, found [str]");
        assert_eq!(
            error(r#"bytes("a", "b")"#),
            "1:1: bytes expects 1 argument, got 2"
        );
        assert_eq!(error("let f = bytes;"), "1:9: undefined variable: bytes");
        assert_eq!(
            error(r#"let b: [str] = bytes("a");"#),
            "1:16: expected [str], found [int]"
        );
    }

    #[test]
    fn for_errors() {
        assert_eq!(
            error("for i in 0..3 { i = 1; }"),
            "1:17: cannot assign to immutable variable: i"
        );
        assert_eq!(
            error("for _ in true..3 {}"),
            "1:10: expected int, found bool"
        );
        assert_eq!(
            error(r#"for _ in 0.."3" {}"#),
            "1:13: expected int, found str"
        );
        assert_eq!(error("for i in 0..3 { i }"), "1:17: expected (), found int");
        assert_eq!(error("for _ in 0..3 {} i"), "1:18: undefined variable: i");
        assert_eq!(
            error("let x: int = for _ in 0..3 {}; x"),
            "1:14: expected int, found ()"
        );
    }

    #[test]
    fn for_array_errors() {
        assert_eq!(
            error("for x in 5 {}"),
            "1:10: cannot iterate over a value of type int"
        );
        assert_eq!(
            error(r#"for c in "ab" {}"#),
            "1:10: cannot iterate over a value of type str"
        );
        assert_eq!(
            error("for x in [] {}"),
            "1:10: cannot infer the type of an empty array"
        );
        assert_eq!(
            error("for x in [[]] {}"),
            "1:10: cannot infer the type of an empty array"
        );
        assert_eq!(
            error("for x in [1] { x = 2; }"),
            "1:16: cannot assign to immutable variable: x"
        );
        assert_eq!(
            error(r#"for x in ["a"] { x - 1; }"#),
            "1:18: expected int, found str"
        );
        assert_eq!(error("for x in y { x; }"), "1:10: undefined variable: y");
    }

    #[test]
    fn compound_assignment_errors() {
        assert_eq!(
            error(r#"let mut x = 1; x += "a";"#),
            "1:21: expected int, found str"
        );
        assert_eq!(
            error("let mut b = true; b += 1;"),
            "1:19: expected int, found bool"
        );
        assert_eq!(
            errors(r#"let mut s = "a"; s -= "b"; s"#),
            [
                "1:18: expected int, found str",
                "1:23: expected int, found str"
            ]
        );
        assert_eq!(
            error("let mut a = [true]; a[0] *= 2; a"),
            "1:21: expected int, found bool"
        );
        assert_eq!(
            error("let x = 1; x += 1;"),
            "1:12: cannot assign to immutable variable: x"
        );
        assert_eq!(
            error(r#"let mut s = "a"; s[1..] += "b"; s"#),
            "1:19: cannot assign to a slice"
        );
        assert_eq!(error("y += 1;"), "1:1: undefined variable: y");
        assert_eq!(
            error("let mut a = [1]; a[true] += 2; a"),
            "1:20: expected int, found bool"
        );
    }

    #[test]
    fn struct_declaration_errors() {
        assert_eq!(error("struct int {}"), "1:8: int is a built-in type");
        assert_eq!(
            error("struct P {} struct P { x: int }"),
            "1:20: duplicate struct: P"
        );
        assert_eq!(
            error("struct P { x: int, x: bool }"),
            "1:20: duplicate field: x"
        );
        assert_eq!(error("struct A { a: A }"), "1:8: recursive struct: A");
        assert_eq!(
            errors("struct B { c: C } struct C { b: [B], c: B }"),
            ["1:8: recursive struct: B", "1:26: recursive struct: C"]
        );
    }

    #[test]
    fn unknown_types() {
        assert_eq!(error("fn f(a: string) {}"), "1:9: unknown type: string");
        assert_eq!(
            errors("struct P { q: [Q] } let p: fn(R) -> P = f; fn f(r: R) -> P { P { q: [] } } p"),
            [
                "1:16: unknown type: Q",
                "1:31: unknown type: R",
                "1:52: unknown type: R"
            ]
        );
        assert_eq!(error("let a: Foo = 1; a"), "1:8: unknown type: Foo");
    }

    #[test]
    fn struct_literal_errors() {
        assert_eq!(
            errors(r#"struct P { x: int, y: str } P { x: "a", z: 1, x: 2 }"#),
            [
                "1:29: missing field y in P",
                "1:36: expected int, found str",
                "1:41: unknown field z in P",
                "1:47: duplicate field: x"
            ]
        );
        assert_eq!(
            errors("struct P { x: int, y: int } P {}"),
            ["1:29: missing field x in P", "1:29: missing field y in P"]
        );
        assert_eq!(
            errors("Nope { a: 1 + true }"),
            ["1:1: unknown type: Nope", "1:15: expected int, found bool"]
        );
        assert_eq!(
            error("struct P { x: int } let p: P = 1; p"),
            "1:32: expected P, found int"
        );
    }

    #[test]
    fn struct_comparison_errors() {
        assert_eq!(
            error("struct P { f: fn() } fn g() {} P { f: g } == P { f: g }"),
            "1:43: cannot compare values of type P"
        );
        assert_eq!(
            errors("struct P {} P {} < P {}"),
            ["1:13: expected int, found P", "1:20: expected int, found P"]
        );
    }

    #[test]
    fn field_errors() {
        assert_eq!(
            errors("struct P { x: int } let p = P { x: 1 }; p.y + p.x.z"),
            ["1:43: unknown field y in P", "1:51: int has no fields"]
        );
        assert_eq!(error("[1].x"), "1:5: [int] has no fields");
        assert_eq!(error("q.x"), "1:1: undefined variable: q");
        assert_eq!(
            error("struct P { x: int } let s: str = P { x: 1 }.x; s"),
            "1:45: expected str, found int"
        );
        assert_eq!(
            error("struct P { a: [int] } let mut p = P { a: [] }; push(p.a, 1); p"),
            "1:55: can only push to a variable or an element of one"
        );
    }
}
