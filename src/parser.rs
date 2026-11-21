use std::fmt;

use crate::ast::{
    BinaryOp, Block, Expr, ExprKind, FnDecl, Index, Iterable, LogicalOp, Param, Program, Stmt,
    Type, UnaryOp,
};
use crate::lexer::{Located, Pos, Token};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub pos: Pos,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    Expected { what: &'static str, found: Token },
    Unexpected(Token),
    IntegerTooLarge,
    UnknownType(String),
    NestedFn,
    ReturnOutsideFn,
    BreakOutsideLoop,
    ContinueOutsideLoop,
    TooDeep,
}

impl ParseErrorKind {
    fn at(self, pos: Pos) -> ParseError {
        ParseError { kind: self, pos }
    }
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseErrorKind::Expected {
                what,
                found: Token::Eof,
            } => {
                write!(f, "expected {what}, found end of input")
            }
            ParseErrorKind::Expected { what, found } => {
                write!(f, "expected {what}, found '{found}'")
            }
            ParseErrorKind::Unexpected(token) => write!(f, "unexpected '{token}'"),
            ParseErrorKind::IntegerTooLarge => write!(f, "integer literal is too large"),
            ParseErrorKind::UnknownType(name) => write!(f, "unknown type: {name}"),
            ParseErrorKind::NestedFn => {
                write!(f, "functions can only be declared at the top level")
            }
            ParseErrorKind::ReturnOutsideFn => write!(f, "'return' outside of a function"),
            ParseErrorKind::BreakOutsideLoop => write!(f, "'break' outside of a loop"),
            ParseErrorKind::ContinueOutsideLoop => write!(f, "'continue' outside of a loop"),
            ParseErrorKind::TooDeep => {
                write!(f, "nesting is too deep, the limit is {MAX_DEPTH}")
            }
        }
    }
}

// The parser, checker and compiler handle nesting recursively, so it is limited.
const MAX_DEPTH: usize = 256;

pub struct Parser {
    tokens: Vec<Located>,
    pos: usize,
    in_fn: bool,
    in_loop: bool,
    errors: Vec<ParseError>,
    depth: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Located>) -> Self {
        Self {
            tokens,
            pos: 0,
            in_fn: false,
            in_loop: false,
            errors: Vec::new(),
            depth: 0,
        }
    }

    // Parses the whole program. On a syntax error the parser skips to the next
    // statement and goes on, so it returns all errors, not only the first.
    pub fn parse(mut self) -> Result<Program, Vec<ParseError>> {
        let (stmts, expr) = self.sequence(true);
        if self.errors.is_empty() {
            Ok(Program { stmts, expr })
        } else {
            Err(self.errors)
        }
    }

    fn block(&mut self) -> Result<Block, ParseError> {
        let (stmts, expr) = self.sequence(false);
        self.expect(Token::RBrace, "'}'")?;
        Ok(Block {
            stmts,
            expr: expr.map(Box::new),
        })
    }

    // Stops at the end of input or, inside a block, at `}`. Errors in the
    // statements are recorded, so a block always fails only at the end of input.
    fn sequence(&mut self, top_level: bool) -> (Vec<Stmt>, Option<Expr>) {
        let mut stmts = Vec::new();
        loop {
            let start = self.pos;
            let pos = self.peek_pos();
            let stmt = match self.peek() {
                Token::Eof => return (stmts, None),
                Token::RBrace if !top_level => return (stmts, None),
                Token::RBrace => Err(ParseErrorKind::Unexpected(Token::RBrace).at(pos)),
                Token::Let => self.let_stmt(),
                Token::Fn if top_level => self.fn_decl(),
                Token::Fn => Err(ParseErrorKind::NestedFn.at(pos)),
                _ => match self.expr() {
                    Ok(value) => {
                        let pos = self.peek_pos();
                        match self.peek() {
                            Token::Eq => self.assign_stmt(value),
                            Token::Semicolon => {
                                self.advance();
                                Ok(Stmt::Expr(value))
                            }
                            Token::Eof => return (stmts, Some(value)),
                            Token::RBrace if !top_level => return (stmts, Some(value)),
                            // A stray `}` is reported on the next iteration.
                            Token::RBrace => Ok(Stmt::Expr(value)),
                            _ if matches!(
                                value.kind,
                                ExprKind::Block(_)
                                    | ExprKind::If { .. }
                                    | ExprKind::While { .. }
                                    | ExprKind::For { .. }
                            ) =>
                            {
                                Ok(Stmt::Expr(value))
                            }
                            token if top_level => {
                                Err(ParseErrorKind::Unexpected(token.clone()).at(pos))
                            }
                            token => Err(ParseErrorKind::Expected {
                                what: "'}'",
                                found: token.clone(),
                            }
                            .at(pos)),
                        }
                    }
                    Err(error) => Err(error),
                },
            };
            match stmt {
                Ok(stmt) => stmts.push(stmt),
                Err(error) => self.recover(error, start),
            }
        }
    }

    // Records the error and skips the rest of the broken statement: up to and
    // including `;`, or up to `}`, `let` or `fn`, unless they are inside
    // brackets opened while skipping.
    fn recover(&mut self, error: ParseError, start: usize) {
        // The failed rule may have consumed the token it complains about.
        // Step back to it: it may be the `;` or `}` to stop at.
        while self.peek_pos() > error.pos {
            self.pos -= 1;
        }
        // An unclosed block fails at the end of input in every enclosing
        // block too. Report it once.
        if self.errors.last().is_none_or(|last| last.pos != error.pos) {
            self.errors.push(error);
        }
        let mut depth = 0;
        // If the statement fails on its first token, skip that token,
        // otherwise the same error would repeat forever.
        let mut skip = self.pos == start;
        loop {
            match self.peek() {
                Token::Eof => return,
                Token::LParen | Token::LBrace | Token::LBracket => depth += 1,
                Token::RParen | Token::RBrace | Token::RBracket if depth > 0 => depth -= 1,
                _ if skip || depth > 0 => {}
                Token::Semicolon => {
                    self.advance();
                    return;
                }
                Token::RBrace | Token::Let | Token::Fn => return,
                _ => {}
            }
            skip = false;
            self.advance();
        }
    }

    fn let_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let mutable = self.mutable();
        let pos = self.peek_pos();
        let name = self.ident("variable name after 'let'")?;
        let ty = if self.peek() == &Token::Colon {
            self.advance();
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(Token::Eq, "'='")?;
        let value = self.expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Let {
            name,
            mutable,
            ty,
            value,
            pos,
        })
    }

    fn fn_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let pos = self.peek_pos();
        let name = self.ident("function name after 'fn'")?;
        self.expect(Token::LParen, "'('")?;
        let params = self.list(Token::RParen, Self::param)?;
        let ret = self.ret_type()?;
        self.expect(Token::LBrace, "'{'")?;
        self.in_fn = true;
        let body = self.block()?;
        self.in_fn = false;
        Ok(Stmt::Fn(FnDecl {
            name,
            pos,
            params,
            ret,
            body,
        }))
    }

    fn param(&mut self) -> Result<Param, ParseError> {
        let mutable = self.mutable();
        let pos = self.peek_pos();
        let name = self.ident("parameter name")?;
        self.expect(Token::Colon, "':'")?;
        let ty = self.ty()?;
        Ok(Param {
            name,
            mutable,
            ty,
            pos,
        })
    }

    fn ret_type(&mut self) -> Result<Type, ParseError> {
        if self.peek() == &Token::Arrow {
            self.advance();
            self.ty()
        } else {
            Ok(Type::Unit)
        }
    }

    fn ty(&mut self) -> Result<Type, ParseError> {
        let pos = self.peek_pos();
        match self.advance() {
            Token::Ident(name) => match name.as_str() {
                "int" => Ok(Type::Int),
                "bool" => Ok(Type::Bool),
                "str" => Ok(Type::Str),
                _ => Err(ParseErrorKind::UnknownType(name).at(pos)),
            },
            Token::LParen => {
                self.expect(Token::RParen, "')'")?;
                Ok(Type::Unit)
            }
            Token::LBracket => self.nested(Self::array_type),
            Token::Fn => self.nested(Self::fn_type),
            other => Err(ParseErrorKind::Expected {
                what: "type",
                found: other,
            }
            .at(pos)),
        }
    }

    fn array_type(&mut self) -> Result<Type, ParseError> {
        let element = self.ty()?;
        self.expect(Token::RBracket, "']'")?;
        Ok(Type::Array(Box::new(element)))
    }

    fn fn_type(&mut self) -> Result<Type, ParseError> {
        self.expect(Token::LParen, "'('")?;
        let params = self.list(Token::RParen, Self::ty)?;
        let ret = Box::new(self.ret_type()?);
        Ok(Type::Fn { params, ret })
    }

    // Only a variable, possibly indexed, can be assigned to. A write into
    // any other value, such as `f()[0]`, would be lost with that value.
    fn assign_stmt(&mut self, target: Expr) -> Result<Stmt, ParseError> {
        let eq_pos = self.peek_pos();
        self.advance();
        let (var, indexes) = match target.kind {
            ExprKind::Index { target, indexes } => (*target, indexes),
            _ => (target, Vec::new()),
        };
        let ExprKind::Var(name) = var.kind else {
            return Err(ParseErrorKind::Unexpected(Token::Eq).at(eq_pos));
        };
        let value = self.expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Assign {
            name,
            indexes,
            value,
            pos: var.pos,
        })
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        let left = self.and()?;
        let mut rest = Vec::new();
        while self.peek() == &Token::OrOr {
            let pos = self.peek_pos();
            self.advance();
            rest.push((LogicalOp::Or, pos, self.and()?));
        }
        Ok(logical_node(left, rest))
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let left = self.comparison()?;
        let mut rest = Vec::new();
        while self.peek() == &Token::AndAnd {
            let pos = self.peek_pos();
            self.advance();
            rest.push((LogicalOp::And, pos, self.comparison()?));
        }
        Ok(logical_node(left, rest))
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let left = self.additive()?;
        let op = match self.peek() {
            Token::EqEq => BinaryOp::Eq,
            Token::NotEq => BinaryOp::Ne,
            Token::Lt => BinaryOp::Lt,
            Token::LtEq => BinaryOp::Le,
            Token::Gt => BinaryOp::Gt,
            Token::GtEq => BinaryOp::Ge,
            _ => return Ok(left),
        };
        let pos = self.peek_pos();
        self.advance();
        let rest = vec![(op, pos, self.additive()?)];
        Ok(binary_node(left, rest))
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        let left = self.term()?;
        let mut rest = Vec::new();
        loop {
            let op = match self.peek() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            let pos = self.peek_pos();
            self.advance();
            rest.push((op, pos, self.term()?));
        }
        Ok(binary_node(left, rest))
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let left = self.unary()?;
        let mut rest = Vec::new();
        loop {
            let op = match self.peek() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => break,
            };
            let pos = self.peek_pos();
            self.advance();
            rest.push((op, pos, self.unary()?));
        }
        Ok(binary_node(left, rest))
    }

    // All recursion in expressions passes through here, so one counter limits it.
    fn unary(&mut self) -> Result<Expr, ParseError> {
        self.nested(Self::prefixed)
    }

    fn prefixed(&mut self) -> Result<Expr, ParseError> {
        let pos = self.peek_pos();
        if self.peek() == &Token::Bang {
            self.advance();
            let operand = self.unary()?;
            return Ok(ExprKind::Unary {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            }
            .at(pos));
        }
        if self.peek() == &Token::Minus {
            self.advance();
            if let &Token::Int(n) = self.peek()
                && n == i64::MIN.unsigned_abs()
            {
                self.advance();
                return Ok(ExprKind::Int(i64::MIN).at(pos));
            }
            let operand = self.unary()?;
            return Ok(ExprKind::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(operand),
            }
            .at(pos));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        // `if c { }` followed by `[1]` on the next line is two expressions, as
        // in a statement. Parentheses allow indexing: `(if c { a } else { b })[0]`.
        let block_like = matches!(
            self.peek(),
            Token::LBrace | Token::If | Token::While | Token::For
        );
        let target = self.primary()?;
        if block_like {
            return Ok(target);
        }
        let mut indexes = Vec::new();
        while self.peek() == &Token::LBracket {
            let pos = self.peek_pos();
            self.advance();
            indexes.push((pos, self.index()?));
            self.expect(Token::RBracket, "']'")?;
        }
        Ok(match indexes.last() {
            Some(&(pos, _)) => ExprKind::Index {
                target: Box::new(target),
                indexes,
            }
            .at(pos),
            None => target,
        })
    }

    fn index(&mut self) -> Result<Index, ParseError> {
        let start = match self.peek() {
            Token::DotDot => None,
            _ => {
                let index = self.expr()?;
                if self.peek() != &Token::DotDot {
                    return Ok(Index::At(index));
                }
                Some(index)
            }
        };
        self.advance();
        let end = match self.peek() {
            Token::RBracket => None,
            _ => Some(self.expr()?),
        };
        Ok(Index::Slice { start, end })
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let pos = self.peek_pos();
        match self.advance() {
            Token::Int(n) => {
                let value =
                    i64::try_from(n).map_err(|_| ParseErrorKind::IntegerTooLarge.at(pos))?;
                Ok(ExprKind::Int(value).at(pos))
            }
            Token::Str(s) => Ok(ExprKind::Str(s).at(pos)),
            Token::True => Ok(ExprKind::Bool(true).at(pos)),
            Token::False => Ok(ExprKind::Bool(false).at(pos)),
            Token::Ident(name) => {
                if self.peek() == &Token::LParen {
                    self.advance();
                    let args = self.list(Token::RParen, Self::expr)?;
                    Ok(ExprKind::Call { name, args }.at(pos))
                } else {
                    Ok(ExprKind::Var(name).at(pos))
                }
            }
            Token::LBrace => Ok(ExprKind::Block(self.block()?).at(pos)),
            Token::LBracket => {
                let items = self.list(Token::RBracket, Self::expr)?;
                Ok(ExprKind::Array(items).at(pos))
            }
            Token::If => self.if_expr(pos),
            Token::While => self.while_expr(pos),
            Token::For => self.for_expr(pos),
            Token::Return if self.in_fn => self.return_expr(pos),
            Token::Return => Err(ParseErrorKind::ReturnOutsideFn.at(pos)),
            Token::Break if self.in_loop => Ok(ExprKind::Break.at(pos)),
            Token::Break => Err(ParseErrorKind::BreakOutsideLoop.at(pos)),
            Token::Continue if self.in_loop => Ok(ExprKind::Continue.at(pos)),
            Token::Continue => Err(ParseErrorKind::ContinueOutsideLoop.at(pos)),
            Token::LParen => {
                let inner = self.expr()?;
                self.expect(Token::RParen, "')'")?;
                Ok(inner)
            }
            other => Err(ParseErrorKind::Expected {
                what: "expression",
                found: other,
            }
            .at(pos)),
        }
    }

    fn if_expr(&mut self, pos: Pos) -> Result<Expr, ParseError> {
        let cond = self.expr()?;
        self.expect(Token::LBrace, "'{'")?;
        let then_branch = self.block()?;
        let else_branch = if self.peek() == &Token::Else {
            self.advance();
            let pos = self.peek_pos();
            let branch = match self.advance() {
                Token::LBrace => ExprKind::Block(self.block()?).at(pos),
                Token::If => self.if_expr(pos)?,
                other => {
                    return Err(ParseErrorKind::Expected {
                        what: "'{' or 'if' after 'else'",
                        found: other,
                    }
                    .at(pos));
                }
            };
            Some(Box::new(branch))
        } else {
            None
        };
        Ok(ExprKind::If {
            cond: Box::new(cond),
            then_branch,
            else_branch,
        }
        .at(pos))
    }

    fn while_expr(&mut self, pos: Pos) -> Result<Expr, ParseError> {
        let cond = self.expr()?;
        let body = self.loop_body()?;
        Ok(ExprKind::While {
            cond: Box::new(cond),
            body,
        }
        .at(pos))
    }

    fn for_expr(&mut self, pos: Pos) -> Result<Expr, ParseError> {
        let var_pos = self.peek_pos();
        let var = self.ident("variable name after 'for'")?;
        self.expect(Token::In, "'in'")?;
        let iter = self.iterable()?;
        let body = self.loop_body()?;
        Ok(ExprKind::For {
            var,
            var_pos,
            iter,
            body,
        }
        .at(pos))
    }

    fn iterable(&mut self) -> Result<Iterable, ParseError> {
        let start = self.expr()?;
        if self.peek() != &Token::DotDot {
            return Ok(Iterable::Array(Box::new(start)));
        }
        self.advance();
        // In `for i in 0.. {}` the body would otherwise be taken for the end.
        if self.peek() == &Token::LBrace {
            return Err(ParseErrorKind::Expected {
                what: "end of range",
                found: Token::LBrace,
            }
            .at(self.peek_pos()));
        }
        let end = self.expr()?;
        Ok(Iterable::Range {
            start: Box::new(start),
            end: Box::new(end),
        })
    }

    fn loop_body(&mut self) -> Result<Block, ParseError> {
        self.expect(Token::LBrace, "'{'")?;
        let outer = self.in_loop;
        self.in_loop = true;
        let body = self.block();
        self.in_loop = outer;
        body
    }

    fn return_expr(&mut self, pos: Pos) -> Result<Expr, ParseError> {
        let value = match self.peek() {
            Token::Semicolon | Token::RBrace | Token::Eof => None,
            _ => Some(Box::new(self.expr()?)),
        };
        Ok(ExprKind::Return(value).at(pos))
    }

    // Parses items separated by commas up to `close`, the opening bracket
    // already consumed.
    fn list<T>(
        &mut self,
        close: Token,
        mut item: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        let mut items = Vec::new();
        if self.peek() == &close {
            self.advance();
            return Ok(items);
        }
        loop {
            items.push(item(self)?);
            let pos = self.peek_pos();
            match self.advance() {
                Token::Comma => {}
                token if token == close => return Ok(items),
                other => {
                    let what = match close {
                        Token::RBracket => "',' or ']'",
                        _ => "',' or ')'",
                    };
                    return Err(ParseErrorKind::Expected { what, found: other }.at(pos));
                }
            }
        }
    }

    fn nested<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        if self.depth == MAX_DEPTH {
            return Err(ParseErrorKind::TooDeep.at(self.peek_pos()));
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn peek_pos(&self) -> Pos {
        self.tokens[self.pos].pos
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if token != Token::Eof {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, expected: Token, what: &'static str) -> Result<(), ParseError> {
        let pos = self.peek_pos();
        match self.advance() {
            token if token == expected => Ok(()),
            other => Err(ParseErrorKind::Expected { what, found: other }.at(pos)),
        }
    }

    fn ident(&mut self, what: &'static str) -> Result<String, ParseError> {
        let pos = self.peek_pos();
        match self.advance() {
            Token::Ident(name) => Ok(name),
            other => Err(ParseErrorKind::Expected { what, found: other }.at(pos)),
        }
    }

    // Skips an optional `mut` and reports whether it was there.
    fn mutable(&mut self) -> bool {
        let mutable = self.peek() == &Token::Mut;
        if mutable {
            self.advance();
        }
        mutable
    }
}

// A chain is located at its last operator, which computes its value.
fn binary_node(left: Expr, rest: Vec<(BinaryOp, Pos, Expr)>) -> Expr {
    match rest.last() {
        Some(&(_, pos, _)) => ExprKind::Binary {
            left: Box::new(left),
            rest,
        }
        .at(pos),
        None => left,
    }
}

fn logical_node(left: Expr, rest: Vec<(LogicalOp, Pos, Expr)>) -> Expr {
    match rest.last() {
        Some(&(_, pos, _)) => ExprKind::Logical {
            left: Box::new(left),
            rest,
        }
        .at(pos),
        None => left,
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use crate::lexer::tokenize;
    use BinaryOp::{Add, Div, Eq, Ge, Gt, Le, Lt, Mul, Ne, Sub};
    use LogicalOp::{And, Or};

    const NO_POS: Pos = Pos { line: 0, col: 0 };

    // Requires exactly one error if parsing fails: the old error tests also
    // check that recovery does not report extra errors.
    fn parse_program(input: &str) -> Result<Program, ParseError> {
        let mut program = Parser::new(tokenize(input).unwrap())
            .parse()
            .map_err(|errors| match <[ParseError; 1]>::try_from(errors) {
                Ok([error]) => error,
                Err(errors) => panic!("{input}: expected one error, got {errors:?}"),
            })?;
        program.stmts.iter_mut().for_each(strip_stmt);
        program.expr.iter_mut().for_each(strip);
        Ok(program)
    }

    fn parse_located(input: &str) -> Expr {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        program.expr.expect("program should end with an expression")
    }

    fn strip(expr: &mut Expr) {
        expr.pos = NO_POS;
        match &mut expr.kind {
            ExprKind::Int(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Var(_)
            | ExprKind::Return(None)
            | ExprKind::Break
            | ExprKind::Continue => {}
            ExprKind::Call { args, .. } | ExprKind::Array(args) => args.iter_mut().for_each(strip),
            ExprKind::Index { target, indexes } => {
                strip(target);
                strip_indexes(indexes);
            }
            ExprKind::Binary { left, rest } => {
                strip(left);
                for (_, pos, right) in rest {
                    *pos = NO_POS;
                    strip(right);
                }
            }
            ExprKind::Logical { left, rest } => {
                strip(left);
                for (_, pos, right) in rest {
                    *pos = NO_POS;
                    strip(right);
                }
            }
            ExprKind::Unary { operand, .. } | ExprKind::Return(Some(operand)) => strip(operand),
            ExprKind::Block(block) => strip_block(block),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                strip(cond);
                strip_block(then_branch);
                else_branch.iter_mut().for_each(|e| strip(e));
            }
            ExprKind::While { cond, body } => {
                strip(cond);
                strip_block(body);
            }
            ExprKind::For {
                var_pos,
                iter,
                body,
                ..
            } => {
                *var_pos = NO_POS;
                match iter {
                    Iterable::Range { start, end } => {
                        strip(start);
                        strip(end);
                    }
                    Iterable::Array(array) => strip(array),
                }
                strip_block(body);
            }
        }
    }

    fn strip_indexes(indexes: &mut [(Pos, Index)]) {
        for (pos, index) in indexes {
            *pos = NO_POS;
            match index {
                Index::At(index) => strip(index),
                Index::Slice { start, end } => start.iter_mut().chain(end).for_each(strip),
            }
        }
    }

    fn strip_block(block: &mut Block) {
        block.stmts.iter_mut().for_each(strip_stmt);
        block.expr.iter_mut().for_each(|e| strip(e));
    }

    fn strip_stmt(stmt: &mut Stmt) {
        match stmt {
            Stmt::Expr(value) => strip(value),
            Stmt::Let { value, pos, .. } => {
                *pos = NO_POS;
                strip(value);
            }
            Stmt::Assign {
                indexes,
                value,
                pos,
                ..
            } => {
                *pos = NO_POS;
                strip_indexes(indexes);
                strip(value);
            }
            Stmt::Fn(decl) => {
                decl.pos = NO_POS;
                decl.params.iter_mut().for_each(|p| p.pos = NO_POS);
                strip_block(&mut decl.body);
            }
        }
    }

    fn errors(input: &str) -> Vec<String> {
        let errors = Parser::new(tokenize(input).unwrap()).parse().unwrap_err();
        errors.iter().map(ToString::to_string).collect()
    }

    fn parse(input: &str) -> Result<Expr, ParseError> {
        parse_program(input)
            .map(|program| program.expr.expect("program should end with an expression"))
    }

    fn expected(what: &'static str, found: Token) -> ParseErrorKind {
        ParseErrorKind::Expected { what, found }
    }

    fn at(kind: ParseErrorKind, line: usize, col: usize) -> ParseError {
        kind.at(Pos { line, col })
    }

    fn int(n: i64) -> Expr {
        ExprKind::Int(n).at(NO_POS)
    }

    fn var(name: &str) -> Expr {
        ExprKind::Var(name.to_string()).at(NO_POS)
    }

    fn boolean(b: bool) -> Expr {
        ExprKind::Bool(b).at(NO_POS)
    }

    fn let_stmt(name: &str, value: Expr) -> Stmt {
        Stmt::Let {
            name: name.to_string(),
            mutable: false,
            ty: None,
            value,
            pos: NO_POS,
        }
    }

    fn call(name: &str, args: Vec<Expr>) -> Expr {
        ExprKind::Call {
            name: name.to_string(),
            args,
        }
        .at(NO_POS)
    }

    fn bin(op: BinaryOp, left: Expr, right: Expr) -> Expr {
        bin_chain(left, vec![(op, right)])
    }

    fn bin_chain(left: Expr, rest: Vec<(BinaryOp, Expr)>) -> Expr {
        let rest = rest.into_iter().map(|(op, e)| (op, NO_POS, e)).collect();
        ExprKind::Binary {
            left: Box::new(left),
            rest,
        }
        .at(NO_POS)
    }

    fn array(items: Vec<Expr>) -> Expr {
        ExprKind::Array(items).at(NO_POS)
    }

    fn index(target: Expr, indexes: Vec<Expr>) -> Expr {
        indexed(target, indexes.into_iter().map(Index::At).collect())
    }

    fn indexed(target: Expr, indexes: Vec<Index>) -> Expr {
        ExprKind::Index {
            target: Box::new(target),
            indexes: indexes.into_iter().map(|i| (NO_POS, i)).collect(),
        }
        .at(NO_POS)
    }

    fn slice(start: Option<Expr>, end: Option<Expr>) -> Index {
        Index::Slice { start, end }
    }

    fn neg(operand: Expr) -> Expr {
        ExprKind::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(operand),
        }
        .at(NO_POS)
    }

    fn not(operand: Expr) -> Expr {
        ExprKind::Unary {
            op: UnaryOp::Not,
            operand: Box::new(operand),
        }
        .at(NO_POS)
    }

    fn logical(op: LogicalOp, left: Expr, right: Expr) -> Expr {
        logical_chain(left, vec![(op, right)])
    }

    fn logical_chain(left: Expr, rest: Vec<(LogicalOp, Expr)>) -> Expr {
        let rest = rest.into_iter().map(|(op, e)| (op, NO_POS, e)).collect();
        ExprKind::Logical {
            left: Box::new(left),
            rest,
        }
        .at(NO_POS)
    }

    fn block(stmts: Vec<Stmt>, expr: Option<Expr>) -> Expr {
        ExprKind::Block(Block {
            stmts,
            expr: expr.map(Box::new),
        })
        .at(NO_POS)
    }

    fn if_expr(cond: Expr, then_branch: Expr, else_branch: Option<Expr>) -> Expr {
        let ExprKind::Block(then_branch) = then_branch.kind else {
            panic!("then branch must be a block");
        };
        ExprKind::If {
            cond: Box::new(cond),
            then_branch,
            else_branch: else_branch.map(Box::new),
        }
        .at(NO_POS)
    }

    fn fn_decl(name: &str, params: &[(&str, Type)], ret: Type, body: Expr) -> Stmt {
        let ExprKind::Block(body) = body.kind else {
            panic!("function body must be a block");
        };
        Stmt::Fn(FnDecl {
            name: name.to_string(),
            pos: NO_POS,
            params: params
                .iter()
                .map(|(name, ty)| Param {
                    name: name.to_string(),
                    mutable: false,
                    ty: ty.clone(),
                    pos: NO_POS,
                })
                .collect(),
            ret,
            body,
        })
    }

    fn parse_type(input: &str) -> Result<Type, ParseError> {
        Parser::new(tokenize(input).unwrap()).ty()
    }

    fn while_expr(cond: Expr, body: Expr) -> Expr {
        let ExprKind::Block(body) = body.kind else {
            panic!("loop body must be a block");
        };
        ExprKind::While {
            cond: Box::new(cond),
            body,
        }
        .at(NO_POS)
    }

    fn for_expr(var: &str, start: Expr, end: Expr, body: Expr) -> Expr {
        let iter = Iterable::Range {
            start: Box::new(start),
            end: Box::new(end),
        };
        for_node(var, iter, body)
    }

    fn for_in(var: &str, array: Expr, body: Expr) -> Expr {
        for_node(var, Iterable::Array(Box::new(array)), body)
    }

    fn for_node(var: &str, iter: Iterable, body: Expr) -> Expr {
        let ExprKind::Block(body) = body.kind else {
            panic!("loop body must be a block");
        };
        ExprKind::For {
            var: var.to_string(),
            var_pos: NO_POS,
            iter,
            body,
        }
        .at(NO_POS)
    }

    fn ret(value: Option<Expr>) -> Expr {
        ExprKind::Return(value.map(Box::new)).at(NO_POS)
    }

    fn brk() -> Expr {
        ExprKind::Break.at(NO_POS)
    }

    fn cont() -> Expr {
        ExprKind::Continue.at(NO_POS)
    }

    // Test threads have a 2 MB stack, too small for debug builds at the limit.
    fn on_big_stack(test: fn()) {
        thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(test)
            .unwrap()
            .join()
            .unwrap();
    }

    // Takes `n + 1` levels: the outer expression is one too.
    fn parens(n: usize) -> String {
        format!("{}1{}", "(".repeat(n), ")".repeat(n))
    }

    #[test]
    fn integer() {
        assert_eq!(parse("42"), Ok(int(42)));
    }

    #[test]
    fn parenthesized() {
        assert_eq!(parse("(42)"), Ok(int(42)));
    }

    #[test]
    fn nested_parens() {
        assert_eq!(parse("((42))"), Ok(int(42)));
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            parse("1 + 2 * 3"),
            Ok(bin(Add, int(1), bin(Mul, int(2), int(3))))
        );
    }

    #[test]
    fn all_operators() {
        assert_eq!(
            parse("1 + 2 - 3 * 4 / 5"),
            Ok(bin_chain(
                int(1),
                vec![
                    (Add, int(2)),
                    (Sub, bin_chain(int(3), vec![(Mul, int(4)), (Div, int(5))]))
                ]
            ))
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(
            parse("10 - 3 - 2"),
            Ok(bin_chain(int(10), vec![(Sub, int(3)), (Sub, int(2))]))
        );
    }

    #[test]
    fn division_is_left_associative() {
        assert_eq!(
            parse("8 / 4 / 2"),
            Ok(bin_chain(int(8), vec![(Div, int(4)), (Div, int(2))]))
        );
    }

    #[test]
    fn parens_override_precedence() {
        assert_eq!(
            parse("(1 + 2) * 3"),
            Ok(bin(Mul, bin(Add, int(1), int(2)), int(3)))
        );
    }

    #[test]
    fn unary_minus() {
        assert_eq!(parse("-5"), Ok(neg(int(5))));
    }

    #[test]
    fn double_unary_minus() {
        assert_eq!(parse("--5"), Ok(neg(neg(int(5)))));
    }

    #[test]
    fn unary_minus_binds_tighter_than_multiplication() {
        assert_eq!(parse("-2 * 3"), Ok(bin(Mul, neg(int(2)), int(3))));
    }

    #[test]
    fn unary_minus_after_binary_operator() {
        assert_eq!(parse("1 - -2"), Ok(bin(Sub, int(1), neg(int(2)))));
    }

    #[test]
    fn unary_minus_on_parenthesized_expr() {
        assert_eq!(parse("-(1 + 2)"), Ok(neg(bin(Add, int(1), int(2)))));
    }

    #[test]
    fn min_value_literal() {
        assert_eq!(parse("-9223372036854775808"), Ok(int(i64::MIN)));
    }

    #[test]
    fn bool_literals() {
        assert_eq!(parse("true"), Ok(boolean(true)));
        assert_eq!(parse("false"), Ok(boolean(false)));
    }

    #[test]
    fn string_literal() {
        assert_eq!(
            parse(r#"f("hi")"#),
            Ok(call("f", vec![ExprKind::Str("hi".to_string()).at(NO_POS)]))
        );
    }

    #[test]
    fn array_literals() {
        assert_eq!(parse("[]"), Ok(array(vec![])));
        assert_eq!(
            parse("[1, x + 1, [2]]"),
            Ok(array(vec![
                int(1),
                bin(Add, var("x"), int(1)),
                array(vec![int(2)])
            ]))
        );
    }

    #[test]
    fn indexing() {
        assert_eq!(parse("a[0]"), Ok(index(var("a"), vec![int(0)])));
        assert_eq!(
            parse("a[i + 1][0]"),
            Ok(index(var("a"), vec![bin(Add, var("i"), int(1)), int(0)]))
        );
        assert_eq!(
            parse("[1, 2][0]"),
            Ok(index(array(vec![int(1), int(2)]), vec![int(0)]))
        );
    }

    #[test]
    fn slicing() {
        assert_eq!(
            parse("s[1..3]"),
            Ok(indexed(var("s"), vec![slice(Some(int(1)), Some(int(3)))]))
        );
        assert_eq!(
            parse("s[i + 1..]"),
            Ok(indexed(
                var("s"),
                vec![slice(Some(bin(Add, var("i"), int(1))), None)]
            ))
        );
        assert_eq!(
            parse("s[..len(s)]"),
            Ok(indexed(
                var("s"),
                vec![slice(None, Some(call("len", vec![var("s")])))]
            ))
        );
        assert_eq!(
            parse("s[..]"),
            Ok(indexed(var("s"), vec![slice(None, None)]))
        );
        assert_eq!(
            parse("a[1..][0]"),
            Ok(indexed(
                var("a"),
                vec![slice(Some(int(1)), None), Index::At(int(0))]
            ))
        );
    }

    #[test]
    fn slice_assignment() {
        assert_eq!(
            parse_program("s[1..] = t;"),
            Ok(Program {
                stmts: vec![Stmt::Assign {
                    name: "s".to_string(),
                    indexes: vec![(NO_POS, slice(Some(int(1)), None))],
                    value: var("t"),
                    pos: NO_POS
                }],
                expr: None
            })
        );
    }

    #[test]
    fn block_like_expressions_are_not_indexed() {
        assert_eq!(
            parse_program("if x { }\n[1]"),
            Ok(Program {
                stmts: vec![Stmt::Expr(if_expr(var("x"), block(vec![], None), None))],
                expr: Some(array(vec![int(1)]))
            })
        );
        assert_eq!(
            parse("(if x { [1] } else { [2] })[0]"),
            Ok(index(
                if_expr(
                    var("x"),
                    block(vec![], Some(array(vec![int(1)]))),
                    Some(block(vec![], Some(array(vec![int(2)]))))
                ),
                vec![int(0)]
            ))
        );
    }

    #[test]
    fn indexing_binds_tighter_than_unary_minus() {
        assert_eq!(parse("-a[0]"), Ok(neg(index(var("a"), vec![int(0)]))));
    }

    #[test]
    fn comparison_binds_looser_than_arithmetic() {
        assert_eq!(
            parse("1 + 2 < 3 * 4"),
            Ok(bin(Lt, bin(Add, int(1), int(2)), bin(Mul, int(3), int(4))))
        );
    }

    #[test]
    fn all_comparison_operators() {
        assert_eq!(parse("1 == 2"), Ok(bin(Eq, int(1), int(2))));
        assert_eq!(parse("1 != 2"), Ok(bin(Ne, int(1), int(2))));
        assert_eq!(parse("1 < 2"), Ok(bin(Lt, int(1), int(2))));
        assert_eq!(parse("1 <= 2"), Ok(bin(Le, int(1), int(2))));
        assert_eq!(parse("1 > 2"), Ok(bin(Gt, int(1), int(2))));
        assert_eq!(parse("1 >= 2"), Ok(bin(Ge, int(1), int(2))));
    }

    #[test]
    fn parenthesized_comparison() {
        assert_eq!(
            parse("(1 < 2) == true"),
            Ok(bin(Eq, bin(Lt, int(1), int(2)), boolean(true)))
        );
    }

    #[test]
    fn logical_not() {
        assert_eq!(parse("!true"), Ok(not(boolean(true))));
        assert_eq!(parse("!!x"), Ok(not(not(var("x")))));
    }

    #[test]
    fn not_binds_tighter_than_comparison() {
        assert_eq!(parse("!a == b"), Ok(bin(Eq, not(var("a")), var("b"))));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(
            parse("a || b && c"),
            Ok(logical(Or, var("a"), logical(And, var("b"), var("c"))))
        );
        assert_eq!(
            parse("a && b || c"),
            Ok(logical(Or, logical(And, var("a"), var("b")), var("c")))
        );
    }

    #[test]
    fn logical_operators_are_left_associative() {
        assert_eq!(
            parse("a || b || c"),
            Ok(logical_chain(
                var("a"),
                vec![(Or, var("b")), (Or, var("c"))]
            ))
        );
    }

    #[test]
    fn comparison_binds_tighter_than_and() {
        assert_eq!(
            parse("1 < x && x < 3"),
            Ok(logical(
                And,
                bin(Lt, int(1), var("x")),
                bin(Lt, var("x"), int(3))
            ))
        );
    }

    #[test]
    fn variable() {
        assert_eq!(parse("x"), Ok(var("x")));
    }

    #[test]
    fn variable_in_expression() {
        assert_eq!(
            parse("x * (y + 1)"),
            Ok(bin(Mul, var("x"), bin(Add, var("y"), int(1))))
        );
    }

    #[test]
    fn call_without_arguments() {
        assert_eq!(parse("f()"), Ok(call("f", vec![])));
    }

    #[test]
    fn call_with_arguments() {
        assert_eq!(
            parse("f(1, x + 2)"),
            Ok(call("f", vec![int(1), bin(Add, var("x"), int(2))]))
        );
    }

    #[test]
    fn call_in_expression() {
        assert_eq!(
            parse("1 + f(g(2)) * 3"),
            Ok(bin(
                Add,
                int(1),
                bin(Mul, call("f", vec![call("g", vec![int(2)])]), int(3))
            ))
        );
    }

    #[test]
    fn block_expression() {
        assert_eq!(
            parse("{ let x = 1; x + 1 }"),
            Ok(block(
                vec![let_stmt("x", int(1))],
                Some(bin(Add, var("x"), int(1)))
            ))
        );
    }

    #[test]
    fn empty_block() {
        assert_eq!(parse("{}"), Ok(block(vec![], None)));
    }

    #[test]
    fn block_in_expression() {
        assert_eq!(
            parse("1 + { 2 }"),
            Ok(bin(Add, int(1), block(vec![], Some(int(2)))))
        );
    }

    #[test]
    fn nested_blocks() {
        assert_eq!(
            parse("{ { 1 } }"),
            Ok(block(vec![], Some(block(vec![], Some(int(1))))))
        );
    }

    #[test]
    fn block_statement_without_semicolon() {
        assert_eq!(
            parse_program("{ print(1); } 2"),
            Ok(Program {
                stmts: vec![Stmt::Expr(block(
                    vec![Stmt::Expr(call("print", vec![int(1)]))],
                    None
                ))],
                expr: Some(int(2))
            })
        );
    }

    #[test]
    fn if_without_else() {
        assert_eq!(
            parse("if x { 1 }"),
            Ok(if_expr(var("x"), block(vec![], Some(int(1))), None))
        );
    }

    #[test]
    fn if_else() {
        assert_eq!(
            parse("if a < b { a } else { b }"),
            Ok(if_expr(
                bin(Lt, var("a"), var("b")),
                block(vec![], Some(var("a"))),
                Some(block(vec![], Some(var("b"))))
            ))
        );
    }

    #[test]
    fn else_if_chain() {
        assert_eq!(
            parse("if a { 1 } else if b { 2 } else { 3 }"),
            Ok(if_expr(
                var("a"),
                block(vec![], Some(int(1))),
                Some(if_expr(
                    var("b"),
                    block(vec![], Some(int(2))),
                    Some(block(vec![], Some(int(3))))
                ))
            ))
        );
    }

    #[test]
    fn if_in_expression() {
        assert_eq!(
            parse("1 + if c { 2 } else { 3 }"),
            Ok(bin(
                Add,
                int(1),
                if_expr(
                    var("c"),
                    block(vec![], Some(int(2))),
                    Some(block(vec![], Some(int(3))))
                )
            ))
        );
    }

    #[test]
    fn if_statement_without_semicolon() {
        assert_eq!(
            parse_program("if c { print(1); } 2"),
            Ok(Program {
                stmts: vec![Stmt::Expr(if_expr(
                    var("c"),
                    block(vec![Stmt::Expr(call("print", vec![int(1)]))], None),
                    None
                ))],
                expr: Some(int(2))
            })
        );
    }

    #[test]
    fn assignment() {
        assert_eq!(
            parse_program("x = x + 1;"),
            Ok(Program {
                stmts: vec![Stmt::Assign {
                    name: "x".to_string(),
                    indexes: vec![],
                    value: bin(Add, var("x"), int(1)),
                    pos: NO_POS
                }],
                expr: None
            })
        );
    }

    #[test]
    fn index_assignment() {
        assert_eq!(
            parse_program("a[0][i] = 1;"),
            Ok(Program {
                stmts: vec![Stmt::Assign {
                    name: "a".to_string(),
                    indexes: vec![(NO_POS, Index::At(int(0))), (NO_POS, Index::At(var("i")))],
                    value: int(1),
                    pos: NO_POS
                }],
                expr: None
            })
        );
    }

    #[test]
    fn equality_is_not_assignment() {
        assert_eq!(parse("x == 1"), Ok(bin(Eq, var("x"), int(1))));
    }

    #[test]
    fn while_loop() {
        assert_eq!(
            parse("while i < 3 { i }"),
            Ok(while_expr(
                bin(Lt, var("i"), int(3)),
                block(vec![], Some(var("i")))
            ))
        );
    }

    #[test]
    fn while_statement_without_semicolon() {
        assert_eq!(
            parse_program("while c {} 1"),
            Ok(Program {
                stmts: vec![Stmt::Expr(while_expr(var("c"), block(vec![], None)))],
                expr: Some(int(1))
            })
        );
    }

    #[test]
    fn for_loop() {
        assert_eq!(
            parse("for i in 0..n { i }"),
            Ok(for_expr(
                "i",
                int(0),
                var("n"),
                block(vec![], Some(var("i")))
            ))
        );
    }

    #[test]
    fn for_bounds_are_expressions() {
        assert_eq!(
            parse("for i in a + 1..len(s) { continue; }"),
            Ok(for_expr(
                "i",
                bin(Add, var("a"), int(1)),
                call("len", vec![var("s")]),
                block(vec![Stmt::Expr(cont())], None)
            ))
        );
    }

    #[test]
    fn for_over_array() {
        assert_eq!(
            parse("for x in a { x }"),
            Ok(for_in("x", var("a"), block(vec![], Some(var("x")))))
        );
        assert_eq!(
            parse("for x in f(a)[1..] {}"),
            Ok(for_in(
                "x",
                indexed(call("f", vec![var("a")]), vec![slice(Some(int(1)), None)]),
                block(vec![], None)
            ))
        );
        assert_eq!(
            parse("for x in 0 {}"),
            Ok(for_in("x", int(0), block(vec![], None)))
        );
    }

    #[test]
    fn for_statement_without_semicolon() {
        assert_eq!(
            parse_program("for _ in 0..3 {} 1"),
            Ok(Program {
                stmts: vec![Stmt::Expr(for_expr(
                    "_",
                    int(0),
                    int(3),
                    block(vec![], None)
                ))],
                expr: Some(int(1))
            })
        );
    }

    #[test]
    fn function_declaration() {
        assert_eq!(
            parse_program("fn add(a: int, b: int) -> int { a + b } add(1, 2)"),
            Ok(Program {
                stmts: vec![fn_decl(
                    "add",
                    &[("a", Type::Int), ("b", Type::Int)],
                    Type::Int,
                    block(vec![], Some(bin(Add, var("a"), var("b"))))
                )],
                expr: Some(call("add", vec![int(1), int(2)]))
            })
        );
    }

    #[test]
    fn function_without_parameters() {
        assert_eq!(
            parse_program("fn zero() -> int { 0 }"),
            Ok(Program {
                stmts: vec![fn_decl("zero", &[], Type::Int, block(vec![], Some(int(0))))],
                expr: None
            })
        );
    }

    #[test]
    fn return_statement() {
        assert_eq!(
            parse_program("fn f() -> int { return 1; }"),
            Ok(Program {
                stmts: vec![fn_decl(
                    "f",
                    &[],
                    Type::Int,
                    block(vec![Stmt::Expr(ret(Some(int(1))))], None)
                )],
                expr: None
            })
        );
    }

    #[test]
    fn return_without_value() {
        assert_eq!(
            parse_program("fn f() { return }"),
            Ok(Program {
                stmts: vec![fn_decl(
                    "f",
                    &[],
                    Type::Unit,
                    block(vec![], Some(ret(None)))
                )],
                expr: None
            })
        );
    }

    #[test]
    fn return_takes_whole_expression() {
        assert_eq!(
            parse_program("fn f() -> int { return 1 + 2; }"),
            Ok(Program {
                stmts: vec![fn_decl(
                    "f",
                    &[],
                    Type::Int,
                    block(vec![Stmt::Expr(ret(Some(bin(Add, int(1), int(2)))))], None)
                )],
                expr: None
            })
        );
    }

    #[test]
    fn types() {
        assert_eq!(parse_type("int"), Ok(Type::Int));
        assert_eq!(parse_type("bool"), Ok(Type::Bool));
        assert_eq!(parse_type("str"), Ok(Type::Str));
        assert_eq!(parse_type("()"), Ok(Type::Unit));
        assert_eq!(
            parse_type("[[int]]"),
            Ok(Type::Array(Box::new(Type::Array(Box::new(Type::Int)))))
        );
        assert_eq!(
            parse_type("fn(int, bool) -> int"),
            Ok(Type::Fn {
                params: vec![Type::Int, Type::Bool],
                ret: Box::new(Type::Int)
            })
        );
        assert_eq!(
            parse_type("fn()"),
            Ok(Type::Fn {
                params: vec![],
                ret: Box::new(Type::Unit)
            })
        );
    }

    #[test]
    fn type_display() {
        for source in [
            "int",
            "bool",
            "str",
            "()",
            "fn()",
            "fn(int, fn(int) -> bool) -> fn() -> int",
            "[[str]]",
            "fn([int]) -> [bool]",
        ] {
            assert_eq!(parse_type(source).unwrap().to_string(), source);
        }
        assert_eq!(Type::Never.to_string(), "!");
    }

    #[test]
    fn function_with_types() {
        assert_eq!(
            parse_program("fn apply(f: fn(int) -> int, x: int) -> int { f(x) }"),
            Ok(Program {
                stmts: vec![fn_decl(
                    "apply",
                    &[
                        (
                            "f",
                            Type::Fn {
                                params: vec![Type::Int],
                                ret: Box::new(Type::Int)
                            }
                        ),
                        ("x", Type::Int)
                    ],
                    Type::Int,
                    block(vec![], Some(call("f", vec![var("x")])))
                )],
                expr: None
            })
        );
    }

    #[test]
    fn mut_parameter() {
        let Stmt::Fn(mut expected) = fn_decl(
            "f",
            &[("n", Type::Int), ("m", Type::Int)],
            Type::Unit,
            block(vec![], None),
        ) else {
            unreachable!()
        };
        expected.params[0].mutable = true;
        assert_eq!(
            parse_program("fn f(mut n: int, m: int) {}"),
            Ok(Program {
                stmts: vec![Stmt::Fn(expected)],
                expr: None
            })
        );
    }

    #[test]
    fn break_and_continue() {
        assert_eq!(
            parse("while x { if y { break; } continue }"),
            Ok(while_expr(
                var("x"),
                block(
                    vec![Stmt::Expr(if_expr(
                        var("y"),
                        block(vec![Stmt::Expr(brk())], None),
                        None
                    ))],
                    Some(cont())
                )
            ))
        );
    }

    #[test]
    fn break_in_nested_loop() {
        assert_eq!(
            parse("while x { while y { break; } break; }"),
            Ok(while_expr(
                var("x"),
                block(
                    vec![
                        Stmt::Expr(while_expr(var("y"), block(vec![Stmt::Expr(brk())], None))),
                        Stmt::Expr(brk())
                    ],
                    None
                )
            ))
        );
    }

    #[test]
    fn infix_expressions_are_located_at_their_operator() {
        let expr = parse_located("a * (b +\n  c)");
        assert_eq!(expr.pos, Pos { line: 1, col: 3 });
        let ExprKind::Binary { left, rest } = expr.kind else {
            panic!("expected a binary expression");
        };
        assert_eq!(left.pos, Pos { line: 1, col: 1 });
        let [(_, _, right)] = &rest[..] else {
            panic!("expected one operator");
        };
        assert_eq!(right.pos, Pos { line: 1, col: 8 });
        let ExprKind::Binary { rest, .. } = &right.kind else {
            panic!("expected a binary expression");
        };
        assert_eq!(rest[0].2.pos, Pos { line: 2, col: 3 });
    }

    #[test]
    fn index_is_located_at_its_bracket() {
        let expr = parse_located("a[0]\n  [1]");
        assert_eq!(expr.pos, Pos { line: 2, col: 3 });
        let ExprKind::Index { indexes, .. } = expr.kind else {
            panic!("expected an index expression");
        };
        let positions: Vec<Pos> = indexes.iter().map(|(pos, _)| *pos).collect();
        assert_eq!(
            positions,
            [Pos { line: 1, col: 2 }, Pos { line: 2, col: 3 }]
        );
    }

    #[test]
    fn chain_is_located_at_its_last_operator() {
        let expr = parse_located("a - b\n  + c");
        assert_eq!(expr.pos, Pos { line: 2, col: 3 });
        let ExprKind::Binary { rest, .. } = expr.kind else {
            panic!("expected a binary expression");
        };
        let positions: Vec<Pos> = rest.iter().map(|(_, pos, _)| *pos).collect();
        assert_eq!(
            positions,
            [Pos { line: 1, col: 3 }, Pos { line: 2, col: 3 }]
        );
    }

    #[test]
    fn other_expressions_are_located_at_their_first_token() {
        let expr = parse_located("if x {\n  -f(y)\n} else {}");
        assert_eq!(expr.pos, Pos { line: 1, col: 1 });
        let ExprKind::If {
            cond,
            then_branch,
            else_branch: Some(else_branch),
        } = expr.kind
        else {
            panic!("expected an if expression with else");
        };
        assert_eq!(cond.pos, Pos { line: 1, col: 4 });
        assert_eq!(else_branch.pos, Pos { line: 3, col: 8 });
        let neg = then_branch
            .expr
            .expect("then branch should end with an expression");
        assert_eq!(neg.pos, Pos { line: 2, col: 3 });
        let ExprKind::Unary { operand: call, .. } = neg.kind else {
            panic!("expected a unary expression");
        };
        assert_eq!(call.pos, Pos { line: 2, col: 4 });
    }

    #[test]
    fn for_is_located_at_its_keyword() {
        let expr = parse_located("for\n  i in 0..3 {}");
        assert_eq!(expr.pos, Pos { line: 1, col: 1 });
        let ExprKind::For { var_pos, .. } = expr.kind else {
            panic!("expected a for loop");
        };
        assert_eq!(var_pos, Pos { line: 2, col: 3 });
    }

    #[test]
    fn empty_program() {
        assert_eq!(
            parse_program(""),
            Ok(Program {
                stmts: vec![],
                expr: None
            })
        );
    }

    #[test]
    fn let_statement() {
        assert_eq!(
            parse_program("let x = 1 + 2;"),
            Ok(Program {
                stmts: vec![let_stmt("x", bin(Add, int(1), int(2)))],
                expr: None
            })
        );
    }

    #[test]
    fn let_mut_statement() {
        assert_eq!(
            parse_program("let mut x = 1;"),
            Ok(Program {
                stmts: vec![Stmt::Let {
                    name: "x".to_string(),
                    mutable: true,
                    ty: None,
                    value: int(1),
                    pos: NO_POS
                }],
                expr: None
            })
        );
    }

    #[test]
    fn let_with_type() {
        assert_eq!(
            parse_program("let a: [int] = [];"),
            Ok(Program {
                stmts: vec![Stmt::Let {
                    name: "a".to_string(),
                    mutable: false,
                    ty: Some(Type::Array(Box::new(Type::Int))),
                    value: array(vec![]),
                    pos: NO_POS
                }],
                expr: None
            })
        );
    }

    #[test]
    fn statements_then_expression() {
        assert_eq!(
            parse_program("let x = 1; let y = x; y"),
            Ok(Program {
                stmts: vec![let_stmt("x", int(1)), let_stmt("y", var("x"))],
                expr: Some(var("y"))
            })
        );
    }

    #[test]
    fn expression_statement() {
        assert_eq!(
            parse_program("1 + 2;"),
            Ok(Program {
                stmts: vec![Stmt::Expr(bin(Add, int(1), int(2)))],
                expr: None
            })
        );
    }

    #[test]
    fn mixed_statements() {
        assert_eq!(
            parse_program("1; let x = 2; x; x + 1"),
            Ok(Program {
                stmts: vec![
                    Stmt::Expr(int(1)),
                    let_stmt("x", int(2)),
                    Stmt::Expr(var("x")),
                ],
                expr: Some(bin(Add, var("x"), int(1)))
            })
        );
    }

    #[test]
    fn error_messages() {
        assert_eq!(
            expected("')'", Token::Int(42)).to_string(),
            "expected ')', found '42'"
        );
        assert_eq!(
            expected("expression", Token::Eof).to_string(),
            "expected expression, found end of input"
        );
        assert_eq!(
            ParseErrorKind::Unexpected(Token::Let).to_string(),
            "unexpected 'let'"
        );
        assert_eq!(
            ParseErrorKind::IntegerTooLarge.to_string(),
            "integer literal is too large"
        );
        assert_eq!(
            ParseErrorKind::NestedFn.to_string(),
            "functions can only be declared at the top level"
        );
        assert_eq!(
            ParseErrorKind::ReturnOutsideFn.to_string(),
            "'return' outside of a function"
        );
        assert_eq!(
            ParseErrorKind::BreakOutsideLoop.to_string(),
            "'break' outside of a loop"
        );
        assert_eq!(
            ParseErrorKind::ContinueOutsideLoop.to_string(),
            "'continue' outside of a loop"
        );
        assert_eq!(
            ParseErrorKind::TooDeep.to_string(),
            "nesting is too deep, the limit is 256"
        );
        assert_eq!(
            at(expected("expression", Token::Eof), 1, 4).to_string(),
            "1:4: expected expression, found end of input"
        );
    }

    #[test]
    fn unclosed_paren() {
        assert_eq!(parse("(42"), Err(at(expected("')'", Token::Eof), 1, 4)));
    }

    #[test]
    fn wrong_token_instead_of_closing_paren() {
        assert_eq!(
            parse("(42 42"),
            Err(at(expected("')'", Token::Int(42)), 1, 5))
        );
    }

    #[test]
    fn extra_closing_paren() {
        assert_eq!(
            parse("42)"),
            Err(at(ParseErrorKind::Unexpected(Token::RParen), 1, 3))
        );
    }

    #[test]
    fn trailing_tokens() {
        assert_eq!(
            parse("1 2"),
            Err(at(ParseErrorKind::Unexpected(Token::Int(2)), 1, 3))
        );
    }

    #[test]
    fn empty_parens() {
        assert_eq!(
            parse("()"),
            Err(at(expected("expression", Token::RParen), 1, 2))
        );
    }

    #[test]
    fn missing_right_operand() {
        assert_eq!(
            parse("1 +"),
            Err(at(expected("expression", Token::Eof), 1, 4))
        );
    }

    #[test]
    fn missing_left_operand() {
        assert_eq!(
            parse("* 2"),
            Err(at(expected("expression", Token::Star), 1, 1))
        );
    }

    #[test]
    fn unary_minus_without_operand() {
        assert_eq!(
            parse("-"),
            Err(at(expected("expression", Token::Eof), 1, 2))
        );
    }

    #[test]
    fn literal_too_large() {
        assert_eq!(
            parse("9223372036854775808"),
            Err(at(ParseErrorKind::IntegerTooLarge, 1, 1))
        );
    }

    #[test]
    fn min_value_literal_in_parens_is_too_large() {
        assert_eq!(
            parse("-(9223372036854775808)"),
            Err(at(ParseErrorKind::IntegerTooLarge, 1, 3))
        );
    }

    #[test]
    fn unclosed_call() {
        assert_eq!(
            parse("f(1"),
            Err(at(expected("',' or ')'", Token::Eof), 1, 4))
        );
    }

    #[test]
    fn call_arguments_without_comma() {
        assert_eq!(
            parse("f(1 2)"),
            Err(at(expected("',' or ')'", Token::Int(2)), 1, 5))
        );
    }

    #[test]
    fn call_trailing_comma() {
        assert_eq!(
            parse("f(1,)"),
            Err(at(expected("expression", Token::RParen), 1, 5))
        );
    }

    #[test]
    fn let_without_name() {
        assert_eq!(
            parse_program("let = 1;"),
            Err(at(expected("variable name after 'let'", Token::Eq), 1, 5))
        );
        assert_eq!(
            parse_program("let mut = 1;"),
            Err(at(expected("variable name after 'let'", Token::Eq), 1, 9))
        );
    }

    #[test]
    fn missing_semicolon_between_expressions() {
        assert_eq!(
            parse_program("1 2;"),
            Err(at(ParseErrorKind::Unexpected(Token::Int(2)), 1, 3))
        );
    }

    #[test]
    fn let_without_eq() {
        assert_eq!(
            parse_program("let x 1;"),
            Err(at(expected("'='", Token::Int(1)), 1, 7))
        );
    }

    #[test]
    fn let_without_semicolon() {
        assert_eq!(
            parse_program("let x = 1"),
            Err(at(expected("';'", Token::Eof), 1, 10))
        );
    }

    #[test]
    fn let_after_expression() {
        assert_eq!(
            parse_program("1 let x = 2;"),
            Err(at(ParseErrorKind::Unexpected(Token::Let), 1, 3))
        );
    }

    #[test]
    fn chained_comparison() {
        assert_eq!(
            parse("1 < 2 < 3"),
            Err(at(ParseErrorKind::Unexpected(Token::Lt), 1, 7))
        );
    }

    #[test]
    fn missing_right_operand_of_or() {
        assert_eq!(
            parse("true ||"),
            Err(at(expected("expression", Token::Eof), 1, 8))
        );
    }

    #[test]
    fn unclosed_block() {
        assert_eq!(parse("{ 1"), Err(at(expected("'}'", Token::Eof), 1, 4)));
    }

    #[test]
    fn block_with_two_expressions() {
        assert_eq!(
            parse("{ 1 2 }"),
            Err(at(expected("'}'", Token::Int(2)), 1, 5))
        );
    }

    #[test]
    fn stray_closing_brace() {
        assert_eq!(
            parse_program("1; }"),
            Err(at(ParseErrorKind::Unexpected(Token::RBrace), 1, 4))
        );
    }

    #[test]
    fn if_without_braces() {
        assert_eq!(
            parse("if true 1"),
            Err(at(expected("'{'", Token::Int(1)), 1, 9))
        );
    }

    #[test]
    fn else_without_braces() {
        assert_eq!(
            parse("if true { 1 } else 2"),
            Err(at(
                expected("'{' or 'if' after 'else'", Token::Int(2)),
                1,
                20
            ))
        );
    }

    #[test]
    fn if_without_condition() {
        assert_eq!(
            parse("if { 1 }"),
            Err(at(expected("'{'", Token::Eof), 1, 9))
        );
    }

    #[test]
    fn assignment_without_semicolon() {
        assert_eq!(
            parse_program("x = 1"),
            Err(at(expected("';'", Token::Eof), 1, 6))
        );
    }

    #[test]
    fn assignment_to_non_variable() {
        assert_eq!(
            parse_program("1 = 2;"),
            Err(at(ParseErrorKind::Unexpected(Token::Eq), 1, 3))
        );
        assert_eq!(
            parse_program("[1, 2][0] = 3;"),
            Err(at(ParseErrorKind::Unexpected(Token::Eq), 1, 11))
        );
        assert_eq!(
            parse_program("f()[0] = 1;"),
            Err(at(ParseErrorKind::Unexpected(Token::Eq), 1, 8))
        );
        assert_eq!(
            parse_program("{ 1 = 2; }"),
            Err(at(ParseErrorKind::Unexpected(Token::Eq), 1, 5))
        );
    }

    #[test]
    fn assignment_is_not_an_expression() {
        assert_eq!(
            parse_program("let y = x = 1;"),
            Err(at(expected("';'", Token::Eq), 1, 11))
        );
    }

    #[test]
    fn while_without_braces() {
        assert_eq!(
            parse("while true 1"),
            Err(at(expected("'{'", Token::Int(1)), 1, 12))
        );
    }

    #[test]
    fn nested_function() {
        assert_eq!(
            parse_program("{ fn f() {} }"),
            Err(at(ParseErrorKind::NestedFn, 1, 3))
        );
    }

    #[test]
    fn function_without_name() {
        assert_eq!(
            parse_program("fn (a: int) {}"),
            Err(at(
                expected("function name after 'fn'", Token::LParen),
                1,
                4
            ))
        );
    }

    #[test]
    fn parameter_must_be_name() {
        assert_eq!(
            parse_program("fn f(1) {}"),
            Err(at(expected("parameter name", Token::Int(1)), 1, 6))
        );
    }

    #[test]
    fn function_without_braces() {
        assert_eq!(
            parse_program("fn f() 1"),
            Err(at(expected("'{'", Token::Int(1)), 1, 8))
        );
    }

    #[test]
    fn return_outside_function() {
        assert_eq!(
            parse_program("return 1"),
            Err(at(ParseErrorKind::ReturnOutsideFn, 1, 1))
        );
    }

    #[test]
    fn return_in_top_level_block() {
        assert_eq!(
            parse_program("fn f() {} { return; }"),
            Err(at(ParseErrorKind::ReturnOutsideFn, 1, 13))
        );
    }

    #[test]
    fn break_outside_loop() {
        assert_eq!(
            parse_program("break;"),
            Err(at(ParseErrorKind::BreakOutsideLoop, 1, 1))
        );
        assert_eq!(
            parse_program("while x {} { continue; }"),
            Err(at(ParseErrorKind::ContinueOutsideLoop, 1, 14))
        );
    }

    #[test]
    fn break_in_function_outside_loop() {
        assert_eq!(
            parse_program("fn f() { break; }"),
            Err(at(ParseErrorKind::BreakOutsideLoop, 1, 10))
        );
    }

    #[test]
    fn parameter_needs_type() {
        assert_eq!(
            parse_program("fn f(a) {}"),
            Err(at(expected("':'", Token::RParen), 1, 7))
        );
        assert_eq!(
            parse_program("fn f(a: 1) {}"),
            Err(at(expected("type", Token::Int(1)), 1, 9))
        );
    }

    #[test]
    fn mut_parameter_needs_name() {
        assert_eq!(
            parse_program("fn f(mut: int) {}"),
            Err(at(expected("parameter name", Token::Colon), 1, 9))
        );
    }

    #[test]
    fn unknown_type() {
        assert_eq!(
            parse_program("fn f(a: string) {}"),
            Err(at(ParseErrorKind::UnknownType("string".to_string()), 1, 9))
        );
        assert_eq!(
            ParseErrorKind::UnknownType("string".to_string()).to_string(),
            "unknown type: string"
        );
    }

    #[test]
    fn return_type_after_arrow() {
        assert_eq!(
            parse_program("fn f() -> {}"),
            Err(at(expected("type", Token::LBrace), 1, 11))
        );
    }

    #[test]
    fn several_errors() {
        assert_eq!(
            errors("let x = ;\nlet y = 1 +;\nx = (2;"),
            [
                "1:9: expected expression, found ';'",
                "2:12: expected expression, found ';'",
                "3:7: expected ')', found ';'"
            ]
        );
        assert_eq!(
            errors("fn f(a) { 1 }\nfn g() -> int { 1 2 }\nf(1"),
            [
                "1:7: expected ':', found ')'",
                "2:19: expected '}', found '2'",
                "3:4: expected ',' or ')', found end of input"
            ]
        );
    }

    #[test]
    fn errors_inside_blocks() {
        assert_eq!(
            errors("fn f() { let a = ; return 1; }\nwhile true { 1 + ; break; }\nbreak;"),
            [
                "1:18: expected expression, found ';'",
                "2:18: expected expression, found ';'",
                "3:1: 'break' outside of a loop"
            ]
        );
    }

    #[test]
    fn recovery_skips_nested_braces() {
        assert_eq!(
            errors("fn (a: int) { if a { 1 } }\nlet x 2;\n) ) 1;"),
            [
                "1:4: expected function name after 'fn', found '('",
                "2:7: expected '=', found '2'",
                "3:1: expected expression, found ')'"
            ]
        );
    }

    #[test]
    fn unclosed_block_is_reported_once() {
        assert_eq!(
            errors("fn f() { while true { if x { 1"),
            ["1:31: expected '}', found end of input"]
        );
    }

    #[test]
    fn nesting_up_to_the_limit() {
        on_big_stack(|| {
            assert_eq!(parse(&parens(MAX_DEPTH - 1)), Ok(int(1)));
            assert!(parse_program(&format!("{}1", "-".repeat(MAX_DEPTH - 1))).is_ok());
            assert!(
                parse_program(&format!(
                    "{}{}",
                    "{".repeat(MAX_DEPTH),
                    "}".repeat(MAX_DEPTH)
                ))
                .is_ok()
            );
            let ty = format!("{}int{}", "fn(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
            assert!(parse_type(&ty).is_ok());
        });
    }

    #[test]
    fn nesting_too_deep() {
        on_big_stack(|| {
            assert_eq!(
                parse(&parens(MAX_DEPTH)),
                Err(at(ParseErrorKind::TooDeep, 1, 257))
            );
            assert_eq!(
                parse_program(&format!(
                    "{}{}",
                    "{".repeat(MAX_DEPTH + 1),
                    "}".repeat(MAX_DEPTH + 1)
                )),
                Err(at(ParseErrorKind::TooDeep, 1, 257))
            );
            let ty = format!(
                "{}int{}",
                "fn(".repeat(MAX_DEPTH + 1),
                ")".repeat(MAX_DEPTH + 1)
            );
            assert_eq!(parse_type(&ty), Err(at(ParseErrorKind::TooDeep, 1, 771)));
        });
    }

    #[test]
    fn recovery_after_too_deep() {
        on_big_stack(|| {
            assert_eq!(
                errors(&format!("{};\nlet x = ;\n(1)", parens(1000))),
                [
                    "1:257: nesting is too deep, the limit is 256",
                    "2:9: expected expression, found ';'"
                ]
            );
            let ty = format!("{}int{}", "fn(".repeat(1000), ")".repeat(1000));
            assert_eq!(
                errors(&format!("fn f(a: {ty}) {{}}\nlet x = ;")),
                [
                    "1:779: nesting is too deep, the limit is 256",
                    "2:9: expected expression, found ';'"
                ]
            );
        });
    }

    #[test]
    fn unclosed_array() {
        assert_eq!(
            parse("[1, 2"),
            Err(at(expected("',' or ']'", Token::Eof), 1, 6))
        );
        assert_eq!(
            parse("[1 2]"),
            Err(at(expected("',' or ']'", Token::Int(2)), 1, 4))
        );
        assert_eq!(parse("a[0"), Err(at(expected("']'", Token::Eof), 1, 4)));
        assert_eq!(
            parse_type("[int"),
            Err(at(expected("']'", Token::Eof), 1, 5))
        );
    }

    #[test]
    fn recovery_skips_brackets() {
        assert_eq!(
            errors("let a = ) [1; 2];\nlet b = ;"),
            [
                "1:9: expected expression, found ')'",
                "2:9: expected expression, found ';'"
            ]
        );
    }

    #[test]
    fn let_with_bad_type() {
        assert_eq!(
            parse_program("let a: = 1;"),
            Err(at(expected("type", Token::Eq), 1, 8))
        );
        assert_eq!(
            parse_program("let a: int 1;"),
            Err(at(expected("'='", Token::Int(1)), 1, 12))
        );
    }

    #[test]
    fn range_outside_brackets() {
        assert_eq!(
            parse("1..2"),
            Err(at(ParseErrorKind::Unexpected(Token::DotDot), 1, 2))
        );
        assert_eq!(
            parse_program("let r = 0..n;"),
            Err(at(expected("';'", Token::DotDot), 1, 10))
        );
        assert_eq!(
            parse("[1..2]"),
            Err(at(expected("',' or ']'", Token::DotDot), 1, 3))
        );
    }

    #[test]
    fn bad_slices() {
        assert_eq!(
            parse("a[1..2..3]"),
            Err(at(expected("']'", Token::DotDot), 1, 7))
        );
        assert_eq!(
            parse("a[1.."),
            Err(at(expected("expression", Token::Eof), 1, 6))
        );
    }

    #[test]
    fn for_needs_both_bounds() {
        assert_eq!(
            parse("for i in ..3 {}"),
            Err(at(expected("expression", Token::DotDot), 1, 10))
        );
        assert_eq!(
            parse("for i in 0.. {}"),
            Err(at(expected("end of range", Token::LBrace), 1, 14))
        );
    }

    #[test]
    fn bad_for_header() {
        assert_eq!(
            parse("for 0..3 {}"),
            Err(at(
                expected("variable name after 'for'", Token::Int(0)),
                1,
                5
            ))
        );
        assert_eq!(
            parse("for i 0..3 {}"),
            Err(at(expected("'in'", Token::Int(0)), 1, 7))
        );
        assert_eq!(
            parse("for i in 0..3"),
            Err(at(expected("'{'", Token::Eof), 1, 14))
        );
        assert_eq!(
            parse_program("let in = 1;"),
            Err(at(expected("variable name after 'let'", Token::In), 1, 5))
        );
    }

    #[test]
    fn break_in_for_bounds_is_outside_the_loop() {
        assert_eq!(
            parse_program("for i in 0..break {}"),
            Err(at(ParseErrorKind::BreakOutsideLoop, 1, 13))
        );
    }
}
