use crate::ast::Expr;
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
}
