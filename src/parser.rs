use crate::ast::{BinaryOp, Expr, Program, Stmt, UnaryOp};
use crate::lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut stmts = Vec::new();
        while let Some(Token::Let) = self.peek() {
            stmts.push(self.let_stmt()?);
        }
        let expr = if self.peek().is_some() {
            Some(self.expr()?)
        } else {
            None
        };
        if let Some(token) = self.peek() {
            return Err(format!("unexpected token: {token:?}"));
        }
        Ok(Program { stmts, expr })
    }

    fn let_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        let name = match self.advance() {
            Some(Token::Ident(name)) => name,
            _ => return Err("expected variable name after 'let'".to_string()),
        };
        self.expect(Token::Eq, "'='")?;
        let value = self.expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Let { name, value })
    }

    fn expr(&mut self) -> Result<Expr, String> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinaryOp::Add,
                Some(Token::Minus) => BinaryOp::Sub,
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

    fn term(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinaryOp::Mul,
                Some(Token::Slash) => BinaryOp::Div,
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

    fn unary(&mut self) -> Result<Expr, String> {
        if let Some(Token::Minus) = self.peek() {
            self.advance();
            if let Some(&Token::Int(n)) = self.peek()
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

    fn primary(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Some(Token::Int(n)) => {
                let value =
                    i64::try_from(n).map_err(|_| "integer literal is too large".to_string())?;
                Ok(Expr::Int(value))
            }
            Some(Token::Ident(name)) => Ok(Expr::Var(name)),
            Some(Token::LParen) => {
                let inner = self.expr()?;
                self.expect(Token::RParen, "')'")?;
                Ok(inner)
            }
            Some(token) => Err(format!("unexpected token: {token:?}")),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.peek().cloned()?;
        self.pos += 1;
        Some(token)
    }

    fn expect(&mut self, expected: Token, what: &str) -> Result<(), String> {
        match self.advance() {
            Some(token) if token == expected => Ok(()),
            _ => Err(format!("expected {what}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_program(input: &str) -> Result<Program, String> {
        Parser::new(tokenize(input).unwrap()).parse()
    }

    fn parse(input: &str) -> Result<Expr, String> {
        parse_program(input)
            .map(|program| program.expr.expect("program should end with an expression"))
    }

    fn int(n: i64) -> Expr {
        Expr::Int(n)
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string())
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
        assert_eq!(parse("42"), Ok(Expr::Int(42)));
    }

    #[test]
    fn parenthesized() {
        assert_eq!(parse("(42)"), Ok(Expr::Int(42)));
    }

    #[test]
    fn nested_parens() {
        assert_eq!(parse("((42))"), Ok(Expr::Int(42)));
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            parse("1 + 2 * 3"),
            Ok(bin(
                BinaryOp::Add,
                int(1),
                bin(BinaryOp::Mul, int(2), int(3))
            ))
        );
    }

    #[test]
    fn all_operators() {
        assert_eq!(
            parse("1 + 2 - 3 * 4 / 5"),
            Ok(bin(
                BinaryOp::Sub,
                bin(BinaryOp::Add, int(1), int(2)),
                bin(BinaryOp::Div, bin(BinaryOp::Mul, int(3), int(4)), int(5))
            ))
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(
            parse("10 - 3 - 2"),
            Ok(bin(
                BinaryOp::Sub,
                bin(BinaryOp::Sub, int(10), int(3)),
                int(2)
            ))
        );
    }

    #[test]
    fn division_is_left_associative() {
        assert_eq!(
            parse("8 / 4 / 2"),
            Ok(bin(
                BinaryOp::Div,
                bin(BinaryOp::Div, int(8), int(4)),
                int(2)
            ))
        );
    }

    #[test]
    fn parens_override_precedence() {
        assert_eq!(
            parse("(1 + 2) * 3"),
            Ok(bin(
                BinaryOp::Mul,
                bin(BinaryOp::Add, int(1), int(2)),
                int(3)
            ))
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
        assert_eq!(parse("-2 * 3"), Ok(bin(BinaryOp::Mul, neg(int(2)), int(3))));
    }

    #[test]
    fn unary_minus_after_binary_operator() {
        assert_eq!(parse("1 - -2"), Ok(bin(BinaryOp::Sub, int(1), neg(int(2)))));
    }

    #[test]
    fn unary_minus_on_parenthesized_expr() {
        assert_eq!(
            parse("-(1 + 2)"),
            Ok(neg(bin(BinaryOp::Add, int(1), int(2))))
        );
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
            Ok(bin(
                BinaryOp::Mul,
                var("x"),
                bin(BinaryOp::Add, var("y"), int(1))
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
                stmts: vec![Stmt::Let {
                    name: "x".to_string(),
                    value: bin(BinaryOp::Add, int(1), int(2))
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
                stmts: vec![
                    Stmt::Let {
                        name: "x".to_string(),
                        value: int(1)
                    },
                    Stmt::Let {
                        name: "y".to_string(),
                        value: var("x")
                    }
                ],
                expr: Some(var("y"))
            })
        );
    }

    #[test]
    fn unclosed_paren() {
        assert_eq!(parse("(42"), Err("expected ')'".to_string()));
    }

    #[test]
    fn wrong_token_instead_of_closing_paren() {
        assert_eq!(parse("(42 42"), Err("expected ')'".to_string()));
    }

    #[test]
    fn extra_closing_paren() {
        assert_eq!(parse("42)"), Err("unexpected token: RParen".to_string()));
    }

    #[test]
    fn trailing_tokens() {
        assert_eq!(parse("1 2"), Err("unexpected token: Int(2)".to_string()));
    }

    #[test]
    fn empty_parens() {
        assert_eq!(parse("()"), Err("unexpected token: RParen".to_string()));
    }

    #[test]
    fn missing_right_operand() {
        assert_eq!(parse("1 +"), Err("unexpected end of input".to_string()));
    }

    #[test]
    fn missing_left_operand() {
        assert_eq!(parse("* 2"), Err("unexpected token: Star".to_string()));
    }

    #[test]
    fn unary_minus_without_operand() {
        assert_eq!(parse("-"), Err("unexpected end of input".to_string()));
    }

    #[test]
    fn literal_too_large() {
        assert_eq!(
            parse("9223372036854775808"),
            Err("integer literal is too large".to_string())
        );
    }

    #[test]
    fn min_value_literal_in_parens_is_too_large() {
        assert_eq!(
            parse("-(9223372036854775808)"),
            Err("integer literal is too large".to_string())
        );
    }

    #[test]
    fn let_without_name() {
        assert_eq!(
            parse_program("let = 1;"),
            Err("expected variable name after 'let'".to_string())
        );
    }

    #[test]
    fn let_without_eq() {
        assert_eq!(parse_program("let x 1;"), Err("expected '='".to_string()));
    }

    #[test]
    fn let_without_semicolon() {
        assert_eq!(parse_program("let x = 1"), Err("expected ';'".to_string()));
    }

    #[test]
    fn let_after_expression() {
        assert_eq!(
            parse_program("1 let x = 2;"),
            Err("unexpected token: Let".to_string())
        );
    }
}
