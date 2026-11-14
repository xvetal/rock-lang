use std::fmt;

use crate::ast::{BinaryOp, Block, Expr, LogicalOp, Program, Stmt, UnaryOp};
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
        }
    }
}

pub struct Parser {
    tokens: Vec<Located>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Located>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let (stmts, expr) = self.sequence()?;
        if self.peek() != &Token::Eof {
            return Err(ParseErrorKind::Unexpected(self.peek().clone()).at(self.peek_pos()));
        }
        Ok(Program { stmts, expr })
    }

    fn block(&mut self) -> Result<Block, ParseError> {
        let (stmts, expr) = self.sequence()?;
        self.expect(Token::RBrace, "'}'")?;
        Ok(Block {
            stmts,
            expr: expr.map(Box::new),
        })
    }

    fn sequence(&mut self) -> Result<(Vec<Stmt>, Option<Expr>), ParseError> {
        let mut stmts = Vec::new();
        loop {
            match self.peek() {
                Token::Eof | Token::RBrace => return Ok((stmts, None)),
                Token::Let => stmts.push(self.let_stmt()?),
                _ => {
                    let value = self.expr()?;
                    if self.peek() == &Token::Semicolon {
                        self.advance();
                        stmts.push(Stmt::Expr(value));
                    } else if matches!(value, Expr::Block(_) | Expr::If { .. })
                        && !matches!(self.peek(), Token::Eof | Token::RBrace)
                    {
                        stmts.push(Stmt::Expr(value));
                    } else {
                        return Ok((stmts, Some(value)));
                    }
                }
            }
        }
    }

    fn let_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let pos = self.peek_pos();
        let name = match self.advance() {
            Token::Ident(name) => name,
            other => {
                return Err(ParseErrorKind::Expected {
                    what: "variable name after 'let'",
                    found: other,
                }
                .at(pos));
            }
        };
        self.expect(Token::Eq, "'='")?;
        let value = self.expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Let { name, value })
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.and()?;
        while self.peek() == &Token::OrOr {
            self.advance();
            let right = self.and()?;
            left = Expr::Logical {
                op: LogicalOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.comparison()?;
        while self.peek() == &Token::AndAnd {
            self.advance();
            let right = self.comparison()?;
            left = Expr::Logical {
                op: LogicalOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
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
        self.advance();
        let right = self.additive()?;
        Ok(Expr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.term()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.peek() == &Token::Bang {
            self.advance();
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            });
        }
        if self.peek() == &Token::Minus {
            self.advance();
            if let &Token::Int(n) = self.peek()
                && n == i64::MIN.unsigned_abs()
            {
                self.advance();
                return Ok(Expr::Int(i64::MIN));
            }
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(operand),
            });
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let pos = self.peek_pos();
        match self.advance() {
            Token::Int(n) => {
                let value =
                    i64::try_from(n).map_err(|_| ParseErrorKind::IntegerTooLarge.at(pos))?;
                Ok(Expr::Int(value))
            }
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Ident(name) => {
                if self.peek() == &Token::LParen {
                    self.advance();
                    let args = self.args()?;
                    Ok(Expr::Call { name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Token::LBrace => Ok(Expr::Block(self.block()?)),
            Token::If => self.if_expr(),
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

    fn if_expr(&mut self) -> Result<Expr, ParseError> {
        let cond = self.expr()?;
        self.expect(Token::LBrace, "'{'")?;
        let then_branch = self.block()?;
        let else_branch = if self.peek() == &Token::Else {
            self.advance();
            let pos = self.peek_pos();
            let branch = match self.advance() {
                Token::LBrace => Expr::Block(self.block()?),
                Token::If => self.if_expr()?,
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
        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch,
            else_branch,
        })
    }

    fn args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if self.peek() == &Token::RParen {
            self.advance();
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            let pos = self.peek_pos();
            match self.advance() {
                Token::Comma => {}
                Token::RParen => return Ok(args),
                other => {
                    return Err(ParseErrorKind::Expected {
                        what: "',' or ')'",
                        found: other,
                    }
                    .at(pos));
                }
            }
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use BinaryOp::{Add, Div, Eq, Ge, Gt, Le, Lt, Mul, Ne, Sub};
    use LogicalOp::{And, Or};

    fn parse_program(input: &str) -> Result<Program, ParseError> {
        Parser::new(tokenize(input).unwrap()).parse()
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
        Expr::Int(n)
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string())
    }

    fn let_stmt(name: &str, value: Expr) -> Stmt {
        Stmt::Let {
            name: name.to_string(),
            value,
        }
    }

    fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            name: name.to_string(),
            args,
        }
    }

    fn bin(op: BinaryOp, left: Expr, right: Expr) -> Expr {
        Expr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    fn neg(operand: Expr) -> Expr {
        Expr::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(operand),
        }
    }

    fn not(operand: Expr) -> Expr {
        Expr::Unary {
            op: UnaryOp::Not,
            operand: Box::new(operand),
        }
    }

    fn logical(op: LogicalOp, left: Expr, right: Expr) -> Expr {
        Expr::Logical {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    fn block(stmts: Vec<Stmt>, expr: Option<Expr>) -> Expr {
        Expr::Block(Block {
            stmts,
            expr: expr.map(Box::new),
        })
    }

    fn if_expr(cond: Expr, then_branch: Expr, else_branch: Option<Expr>) -> Expr {
        let Expr::Block(then_branch) = then_branch else {
            panic!("then branch must be a block");
        };
        Expr::If {
            cond: Box::new(cond),
            then_branch,
            else_branch: else_branch.map(Box::new),
        }
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
            Ok(bin(
                Sub,
                bin(Add, int(1), int(2)),
                bin(Div, bin(Mul, int(3), int(4)), int(5))
            ))
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(
            parse("10 - 3 - 2"),
            Ok(bin(Sub, bin(Sub, int(10), int(3)), int(2)))
        );
    }

    #[test]
    fn division_is_left_associative() {
        assert_eq!(
            parse("8 / 4 / 2"),
            Ok(bin(Div, bin(Div, int(8), int(4)), int(2)))
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
        assert_eq!(parse("true"), Ok(Expr::Bool(true)));
        assert_eq!(parse("false"), Ok(Expr::Bool(false)));
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
            Ok(bin(Eq, bin(Lt, int(1), int(2)), Expr::Bool(true)))
        );
    }

    #[test]
    fn logical_not() {
        assert_eq!(parse("!true"), Ok(not(Expr::Bool(true))));
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
            Ok(logical(Or, logical(Or, var("a"), var("b")), var("c")))
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
}
