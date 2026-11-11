#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Int(i64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
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
            '(' => Token::LParen,
            ')' => Token::RParen,
            '0'..='9' => {
                let mut value = i64::from(c.to_digit(10).expect("c is an ASCII digit"));
                while let Some(&d) = chars.peek() {
                    let Some(digit) = d.to_digit(10) else { break };
                    chars.next();
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(i64::from(digit)))
                        .ok_or("integer literal is too large")?;
                }
                Token::Int(value)
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
    fn max_i64() {
        assert_eq!(
            tokenize("9223372036854775807"),
            Ok(vec![Token::Int(i64::MAX)])
        );
    }

    #[test]
    fn integer_overflow() {
        assert_eq!(
            tokenize("9223372036854775808"),
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
