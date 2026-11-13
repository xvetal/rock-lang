use std::fmt;

use crate::ast::{BinaryOp, Expr, Program, Stmt, UnaryOp};
use crate::lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    Expected { what: &'static str, found: Token },
    Unexpected(Token),
    IntegerTooLarge,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::Expected {
                what,
                found: Token::Eof,
            } => {
                write!(f, "expected {what}, found end of input")
            }
            ParseError::Expected { what, found } => write!(f, "expected {what}, found '{found}'"),
            ParseError::Unexpected(token) => write!(f, "unexpected '{token}'"),
            ParseError::IntegerTooLarge => write!(f, "integer literal is too large"),
        }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut stmts = Vec::new();
        let mut expr = None;
        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Let => stmts.push(self.let_stmt()?),
                _ => {
                    let value = self.expr()?;
                    if self.peek() == &Token::Semicolon {
                        self.advance();
                        stmts.push(Stmt::Expr(value));
                    } else {
                        expr = Some(value);
                        break;
                    }
                }
            }
        }
        if self.peek() != &Token::Eof {
            return Err(ParseError::Unexpected(self.peek().clone()));
        }
        Ok(Program { stmts, expr })
    }

    fn let_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let name = match self.advance() {
            Token::Ident(name) => name,
            other => {
                return Err(ParseError::Expected {
                    what: "variable name after 'let'",
                    found: other,
                });
            }
        };
        self.expect(Token::Eq, "'='")?;
        let value = self.expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Let { name, value })
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
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
        match self.advance() {
            Token::Int(n) => {
                let value = i64::try_from(n).map_err(|_| ParseError::IntegerTooLarge)?;
                Ok(Expr::Int(value))
            }
            Token::Ident(name) => {
                if self.peek() == &Token::LParen {
                    self.advance();
                    let args = self.args()?;
                    Ok(Expr::Call { name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Token::LParen => {
                let inner = self.expr()?;
                self.expect(Token::RParen, "')'")?;
                Ok(inner)
            }
            other => Err(ParseError::Expected {
                what: "expression",
                found: other,
            }),
        }
    }

    fn args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if self.peek() == &Token::RParen {
            self.advance();
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            match self.advance() {
                Token::Comma => {}
                Token::RParen => return Ok(args),
                other => {
                    return Err(ParseError::Expected {
                        what: "',' or ')'",
                        found: other,
                    });
                }
            }
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if token != Token::Eof {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, expected: Token, what: &'static str) -> Result<(), ParseError> {
        match self.advance() {
            token if token == expected => Ok(()),
            other => Err(ParseError::Expected { what, found: other }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use BinaryOp::{Add, Div, Mul, Sub};

    fn parse_program(input: &str) -> Result<Program, ParseError> {
        Parser::new(tokenize(input).unwrap()).parse()
    }

    fn parse(input: &str) -> Result<Expr, ParseError> {
        parse_program(input)
            .map(|program| program.expr.expect("program should end with an expression"))
    }

    fn expected(what: &'static str, found: Token) -> ParseError {
        ParseError::Expected { what, found }
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
            ParseError::Unexpected(Token::Let).to_string(),
            "unexpected 'let'"
        );
        assert_eq!(
            ParseError::IntegerTooLarge.to_string(),
            "integer literal is too large"
        );
    }

    #[test]
    fn unclosed_paren() {
        assert_eq!(parse("(42"), Err(expected("')'", Token::Eof)));
    }

    #[test]
    fn wrong_token_instead_of_closing_paren() {
        assert_eq!(parse("(42 42"), Err(expected("')'", Token::Int(42))));
    }

    #[test]
    fn extra_closing_paren() {
        assert_eq!(parse("42)"), Err(ParseError::Unexpected(Token::RParen)));
    }

    #[test]
    fn trailing_tokens() {
        assert_eq!(parse("1 2"), Err(ParseError::Unexpected(Token::Int(2))));
    }

    #[test]
    fn empty_parens() {
        assert_eq!(parse("()"), Err(expected("expression", Token::RParen)));
    }

    #[test]
    fn missing_right_operand() {
        assert_eq!(parse("1 +"), Err(expected("expression", Token::Eof)));
    }

    #[test]
    fn missing_left_operand() {
        assert_eq!(parse("* 2"), Err(expected("expression", Token::Star)));
    }

    #[test]
    fn unary_minus_without_operand() {
        assert_eq!(parse("-"), Err(expected("expression", Token::Eof)));
    }

    #[test]
    fn literal_too_large() {
        assert_eq!(
            parse("9223372036854775808"),
            Err(ParseError::IntegerTooLarge)
        );
    }

    #[test]
    fn min_value_literal_in_parens_is_too_large() {
        assert_eq!(
            parse("-(9223372036854775808)"),
            Err(ParseError::IntegerTooLarge)
        );
    }

    #[test]
    fn unclosed_call() {
        assert_eq!(parse("f(1"), Err(expected("',' or ')'", Token::Eof)));
    }

    #[test]
    fn call_arguments_without_comma() {
        assert_eq!(parse("f(1 2)"), Err(expected("',' or ')'", Token::Int(2))));
    }

    #[test]
    fn call_trailing_comma() {
        assert_eq!(parse("f(1,)"), Err(expected("expression", Token::RParen)));
    }

    #[test]
    fn let_without_name() {
        assert_eq!(
            parse_program("let = 1;"),
            Err(expected("variable name after 'let'", Token::Eq))
        );
    }

    #[test]
    fn missing_semicolon_between_expressions() {
        assert_eq!(
            parse_program("1 2;"),
            Err(ParseError::Unexpected(Token::Int(2)))
        );
    }

    #[test]
    fn let_without_eq() {
        assert_eq!(
            parse_program("let x 1;"),
            Err(expected("'='", Token::Int(1)))
        );
    }

    #[test]
    fn let_without_semicolon() {
        assert_eq!(parse_program("let x = 1"), Err(expected("';'", Token::Eof)));
    }

    #[test]
    fn let_after_expression() {
        assert_eq!(
            parse_program("1 let x = 2;"),
            Err(ParseError::Unexpected(Token::Let))
        );
    }
}
