use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Int(u64),
    Ident(String),
    Let,
    Plus,
    Minus,
    Star,
    Slash,
    Eq,
    Semicolon,
    Comma,
    LParen,
    RParen,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Int(n) => write!(f, "{n}"),
            Token::Ident(name) => write!(f, "{name}"),
            Token::Let => write!(f, "let"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Eq => write!(f, "="),
            Token::Semicolon => write!(f, ";"),
            Token::Comma => write!(f, ","),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
        }
    }
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        let token = match c {
            _ if c.is_whitespace() => continue,
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '=' => Token::Eq,
            ';' => Token::Semicolon,
            ',' => Token::Comma,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '0'..='9' => {
                let mut value = u64::from(c.to_digit(10).expect("c is an ASCII digit"));
                while let Some(&d) = chars.peek() {
                    let Some(digit) = d.to_digit(10) else { break };
                    chars.next();
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(u64::from(digit)))
                        .ok_or("integer literal is too large")?;
                }
                Token::Int(value)
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut name = String::from(c);
                while let Some(&d) = chars.peek() {
                    if !(d.is_ascii_alphanumeric() || d == '_') {
                        break;
                    }
                    name.push(d);
                    chars.next();
                }
                match name.as_str() {
                    "let" => Token::Let,
                    _ => Token::Ident(name),
                }
            }
            _ => return Err(format!("unexpected character: '{c}'")),
        };

        tokens.push(token);
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(name: &str) -> Token {
        Token::Ident(name.to_string())
    }

    #[test]
    fn single_char_tokens() {
        assert_eq!(
            tokenize("+-*/()"),
            Ok(vec![
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::LParen,
                Token::RParen,
            ])
        );
    }

    #[test]
    fn integers() {
        assert_eq!(
            tokenize("0 42 123"),
            Ok(vec![Token::Int(0), Token::Int(42), Token::Int(123)])
        );
    }

    #[test]
    fn expression() {
        assert_eq!(
            tokenize("12 + (3*4)"),
            Ok(vec![
                Token::Int(12),
                Token::Plus,
                Token::LParen,
                Token::Int(3),
                Token::Star,
                Token::Int(4),
                Token::RParen,
            ])
        );
    }

    #[test]
    fn number_followed_by_operator() {
        assert_eq!(tokenize("123+"), Ok(vec![Token::Int(123), Token::Plus]));
    }

    #[test]
    fn skips_whitespace() {
        assert_eq!(tokenize(" \t\r\n42 "), Ok(vec![Token::Int(42)]));
    }

    #[test]
    fn empty_input() {
        assert_eq!(tokenize(""), Ok(vec![]));
    }

    #[test]
    fn max_u64() {
        assert_eq!(
            tokenize("18446744073709551615"),
            Ok(vec![Token::Int(u64::MAX)])
        );
    }

    #[test]
    fn identifiers() {
        assert_eq!(
            tokenize("x foo_bar _tmp x1"),
            Ok(vec![
                ident("x"),
                ident("foo_bar"),
                ident("_tmp"),
                ident("x1"),
            ])
        );
    }

    #[test]
    fn let_keyword() {
        assert_eq!(tokenize("let"), Ok(vec![Token::Let]));
    }

    #[test]
    fn keyword_prefix_is_identifier() {
        assert_eq!(tokenize("letter"), Ok(vec![ident("letter")]));
    }

    #[test]
    fn let_statement() {
        assert_eq!(
            tokenize("let x = 1;"),
            Ok(vec![
                Token::Let,
                ident("x"),
                Token::Eq,
                Token::Int(1),
                Token::Semicolon,
            ])
        );
    }

    #[test]
    fn digit_then_letters() {
        assert_eq!(tokenize("1x"), Ok(vec![Token::Int(1), ident("x")]));
    }

    #[test]
    fn call_tokens() {
        assert_eq!(
            tokenize("f(x, 1)"),
            Ok(vec![
                ident("f"),
                Token::LParen,
                ident("x"),
                Token::Comma,
                Token::Int(1),
                Token::RParen,
            ])
        );
    }

    #[test]
    fn display_round_trip() {
        let source = "let x = f ( 1 , y ) * 2 / 3 - 4 + 5 ;";
        let tokens = tokenize(source).unwrap();
        let printed: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        assert_eq!(printed.join(" "), source);
    }

    #[test]
    fn integer_overflow() {
        assert_eq!(
            tokenize("18446744073709551616"),
            Err("integer literal is too large".to_string())
        );
    }

    #[test]
    fn unexpected_character() {
        assert_eq!(
            tokenize("1 $ 2"),
            Err("unexpected character: '$'".to_string())
        );
    }
}
