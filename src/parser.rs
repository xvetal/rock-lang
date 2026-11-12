use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Expr, String> {
        let expr = self.expr()?;
        if let Some(token) = self.peek() {
            return Err(format!("unexpected token: {token:?}"));
        }
        Ok(expr)
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
            Some(Token::Int(n)) => Ok(Expr::Int(n)),
            Some(Token::LParen) => {
                let inner = self.expr()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(inner),
                    _ => Err("expected ')'".to_string()),
                }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse(input: &str) -> Result<Expr, String> {
        Parser::new(tokenize(input).unwrap()).parse()
    }

    fn int(n: i64) -> Expr {
        Expr::Int(n)
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
    fn empty_input() {
        assert_eq!(parse(""), Err("unexpected end of input".to_string()));
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
}
