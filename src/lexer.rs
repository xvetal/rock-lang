use std::{fmt, iter::Peekable, str::Chars};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Int(u64),
    Str(String),
    Ident(String),
    Let,
    Mut,
    True,
    False,
    If,
    Else,
    While,
    For,
    In,
    Fn,
    Return,
    Break,
    Continue,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Bang,
    AndAnd,
    OrOr,
    Semicolon,
    Comma,
    Colon,
    Arrow,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    DotDot,
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Int(n) => write!(f, "{n}"),
            Token::Str(s) => write!(f, "{s:?}"),
            Token::Ident(name) => write!(f, "{name}"),
            Token::Let => write!(f, "let"),
            Token::Mut => write!(f, "mut"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::If => write!(f, "if"),
            Token::Else => write!(f, "else"),
            Token::While => write!(f, "while"),
            Token::For => write!(f, "for"),
            Token::In => write!(f, "in"),
            Token::Fn => write!(f, "fn"),
            Token::Return => write!(f, "return"),
            Token::Break => write!(f, "break"),
            Token::Continue => write!(f, "continue"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Percent => write!(f, "%"),
            Token::PlusEq => write!(f, "+="),
            Token::MinusEq => write!(f, "-="),
            Token::StarEq => write!(f, "*="),
            Token::SlashEq => write!(f, "/="),
            Token::PercentEq => write!(f, "%="),
            Token::Eq => write!(f, "="),
            Token::EqEq => write!(f, "=="),
            Token::NotEq => write!(f, "!="),
            Token::Lt => write!(f, "<"),
            Token::LtEq => write!(f, "<="),
            Token::Gt => write!(f, ">"),
            Token::GtEq => write!(f, ">="),
            Token::Bang => write!(f, "!"),
            Token::AndAnd => write!(f, "&&"),
            Token::OrOr => write!(f, "||"),
            Token::Semicolon => write!(f, ";"),
            Token::Comma => write!(f, ","),
            Token::Colon => write!(f, ":"),
            Token::Arrow => write!(f, "->"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::DotDot => write!(f, ".."),
            Token::Eof => write!(f, "end of input"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    fn advance(&mut self, c: char) {
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    pub token: Token,
    pub pos: Pos,
}

pub fn tokenize(input: &str) -> Result<Vec<Located>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut pos = Pos { line: 1, col: 1 };
    let mut end = pos;

    while let Some(c) = chars.next() {
        let start = pos;
        pos.advance(c);
        let token = match c {
            _ if c.is_whitespace() => continue,
            '+' if eat(&mut chars, &mut pos, '=') => Token::PlusEq,
            '+' => Token::Plus,
            '-' if eat(&mut chars, &mut pos, '>') => Token::Arrow,
            '-' if eat(&mut chars, &mut pos, '=') => Token::MinusEq,
            '-' => Token::Minus,
            '*' if eat(&mut chars, &mut pos, '=') => Token::StarEq,
            '*' => Token::Star,
            '/' if eat(&mut chars, &mut pos, '/') => {
                while let Some(c) = chars.next_if(|&c| c != '\n') {
                    pos.advance(c);
                }
                continue;
            }
            '/' if eat(&mut chars, &mut pos, '=') => Token::SlashEq,
            '/' => Token::Slash,
            '%' if eat(&mut chars, &mut pos, '=') => Token::PercentEq,
            '%' => Token::Percent,
            '=' if eat(&mut chars, &mut pos, '=') => Token::EqEq,
            '=' => Token::Eq,
            '!' if eat(&mut chars, &mut pos, '=') => Token::NotEq,
            '!' => Token::Bang,
            '&' if eat(&mut chars, &mut pos, '&') => Token::AndAnd,
            '|' if eat(&mut chars, &mut pos, '|') => Token::OrOr,
            '<' if eat(&mut chars, &mut pos, '=') => Token::LtEq,
            '<' => Token::Lt,
            '>' if eat(&mut chars, &mut pos, '=') => Token::GtEq,
            '>' => Token::Gt,
            ';' => Token::Semicolon,
            ',' => Token::Comma,
            ':' => Token::Colon,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '.' if eat(&mut chars, &mut pos, '.') => Token::DotDot,
            '"' => Token::Str(string(&mut chars, &mut pos, start)?),
            '0'..='9' => {
                let mut value = u64::from(c.to_digit(10).expect("c is an ASCII digit"));
                while let Some(&d) = chars.peek() {
                    let Some(digit) = d.to_digit(10) else { break };
                    chars.next();
                    pos.advance(d);
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(u64::from(digit)))
                        .ok_or_else(|| format!("{start}: integer literal is too large"))?;
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
                    pos.advance(d);
                }
                match name.as_str() {
                    "let" => Token::Let,
                    "mut" => Token::Mut,
                    "true" => Token::True,
                    "false" => Token::False,
                    "if" => Token::If,
                    "else" => Token::Else,
                    "while" => Token::While,
                    "for" => Token::For,
                    "in" => Token::In,
                    "fn" => Token::Fn,
                    "return" => Token::Return,
                    "break" => Token::Break,
                    "continue" => Token::Continue,
                    _ => Token::Ident(name),
                }
            }
            _ => return Err(format!("{start}: unexpected character: '{c}'")),
        };

        tokens.push(Located { token, pos: start });
        end = pos;
    }

    tokens.push(Located {
        token: Token::Eof,
        pos: end,
    });
    Ok(tokens)
}

fn string(chars: &mut Peekable<Chars>, pos: &mut Pos, start: Pos) -> Result<String, String> {
    let mut value = String::new();
    loop {
        let at = *pos;
        let Some(c) = chars.next() else {
            return Err(format!("{start}: unterminated string"));
        };
        pos.advance(c);
        if c == '"' {
            return Ok(value);
        }
        if c != '\\' {
            value.push(c);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err(format!("{start}: unterminated string"));
        };
        pos.advance(escaped);
        value.push(match escaped {
            'n' => '\n',
            't' => '\t',
            '\\' => '\\',
            '"' => '"',
            _ => return Err(format!("{at}: unknown escape: \\{escaped}")),
        });
    }
}

fn eat(chars: &mut Peekable<Chars>, pos: &mut Pos, expected: char) -> bool {
    let matched = chars.next_if_eq(&expected).is_some();
    if matched {
        pos.advance(expected);
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: &str) -> Result<Vec<Token>, String> {
        tokenize(input).map(|tokens| tokens.into_iter().map(|t| t.token).collect())
    }

    fn ident(name: &str) -> Token {
        Token::Ident(name.to_string())
    }

    #[test]
    fn single_char_tokens() {
        assert_eq!(
            tokens("+-*/%(){}"),
            Ok(vec![
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Percent,
                Token::LParen,
                Token::RParen,
                Token::LBrace,
                Token::RBrace,
                Token::Eof
            ])
        );
    }

    #[test]
    fn integers() {
        assert_eq!(
            tokens("0 42 123"),
            Ok(vec![
                Token::Int(0),
                Token::Int(42),
                Token::Int(123),
                Token::Eof
            ])
        );
    }

    #[test]
    fn expression() {
        assert_eq!(
            tokens("12 + (3*4)"),
            Ok(vec![
                Token::Int(12),
                Token::Plus,
                Token::LParen,
                Token::Int(3),
                Token::Star,
                Token::Int(4),
                Token::RParen,
                Token::Eof
            ])
        );
    }

    #[test]
    fn number_followed_by_operator() {
        assert_eq!(
            tokens("123+"),
            Ok(vec![Token::Int(123), Token::Plus, Token::Eof])
        );
    }

    #[test]
    fn skips_whitespace() {
        assert_eq!(tokens(" \t\r\n42 "), Ok(vec![Token::Int(42), Token::Eof]));
    }

    #[test]
    fn skips_comments() {
        assert_eq!(
            tokens("1 // one\n+ 2 /// two"),
            Ok(vec![Token::Int(1), Token::Plus, Token::Int(2), Token::Eof])
        );
        assert_eq!(tokens("// only a comment\n"), Ok(vec![Token::Eof]));
        assert_eq!(
            tokens("1 / 2 // 3"),
            Ok(vec![Token::Int(1), Token::Slash, Token::Int(2), Token::Eof])
        );
    }

    #[test]
    fn comment_may_hold_any_character() {
        assert_eq!(
            tokens("// привет & | . \"\n1"),
            Ok(vec![Token::Int(1), Token::Eof])
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(tokens(""), Ok(vec![Token::Eof]));
    }

    #[test]
    fn max_u64() {
        assert_eq!(
            tokens("18446744073709551615"),
            Ok(vec![Token::Int(u64::MAX), Token::Eof])
        );
    }

    #[test]
    fn identifiers() {
        assert_eq!(
            tokens("x foo_bar _tmp x1"),
            Ok(vec![
                ident("x"),
                ident("foo_bar"),
                ident("_tmp"),
                ident("x1"),
                Token::Eof
            ])
        );
    }

    #[test]
    fn let_keyword() {
        assert_eq!(tokens("let"), Ok(vec![Token::Let, Token::Eof]));
        assert_eq!(tokens("mut"), Ok(vec![Token::Mut, Token::Eof]));
    }

    #[test]
    fn bool_keywords() {
        assert_eq!(
            tokens("true false truth"),
            Ok(vec![Token::True, Token::False, ident("truth"), Token::Eof])
        );
    }

    #[test]
    fn if_else_keywords() {
        assert_eq!(
            tokens("if else iffy"),
            Ok(vec![Token::If, Token::Else, ident("iffy"), Token::Eof])
        );
    }

    #[test]
    fn while_keyword() {
        assert_eq!(tokens("while"), Ok(vec![Token::While, Token::Eof]));
    }

    #[test]
    fn for_keywords() {
        assert_eq!(
            tokens("for in index"),
            Ok(vec![Token::For, Token::In, ident("index"), Token::Eof])
        );
    }

    #[test]
    fn fn_keyword() {
        assert_eq!(
            tokens("fn fun"),
            Ok(vec![Token::Fn, ident("fun"), Token::Eof])
        );
    }

    #[test]
    fn return_keyword() {
        assert_eq!(tokens("return"), Ok(vec![Token::Return, Token::Eof]));
    }

    #[test]
    fn loop_control_keywords() {
        assert_eq!(
            tokens("break continue"),
            Ok(vec![Token::Break, Token::Continue, Token::Eof])
        );
    }

    #[test]
    fn keyword_prefix_is_identifier() {
        assert_eq!(tokens("letter"), Ok(vec![ident("letter"), Token::Eof]));
    }

    #[test]
    fn let_statement() {
        assert_eq!(
            tokens("let x = 1;"),
            Ok(vec![
                Token::Let,
                ident("x"),
                Token::Eq,
                Token::Int(1),
                Token::Semicolon,
                Token::Eof
            ])
        );
    }

    #[test]
    fn digit_then_letters() {
        assert_eq!(
            tokens("1x"),
            Ok(vec![Token::Int(1), ident("x"), Token::Eof])
        );
    }

    #[test]
    fn call_tokens() {
        assert_eq!(
            tokens("f(x, 1)"),
            Ok(vec![
                ident("f"),
                Token::LParen,
                ident("x"),
                Token::Comma,
                Token::Int(1),
                Token::RParen,
                Token::Eof
            ])
        );
    }

    #[test]
    fn comparison_tokens() {
        assert_eq!(
            tokens("== != < <= > >= = ="),
            Ok(vec![
                Token::EqEq,
                Token::NotEq,
                Token::Lt,
                Token::LtEq,
                Token::Gt,
                Token::GtEq,
                Token::Eq,
                Token::Eq,
                Token::Eof
            ])
        );
    }

    #[test]
    fn comparison_without_spaces() {
        assert_eq!(
            tokens("1 <= 2"),
            Ok(vec![Token::Int(1), Token::LtEq, Token::Int(2), Token::Eof])
        );
    }

    #[test]
    fn logical_tokens() {
        assert_eq!(
            tokens("! != && || !x"),
            Ok(vec![
                Token::Bang,
                Token::NotEq,
                Token::AndAnd,
                Token::OrOr,
                Token::Bang,
                ident("x"),
                Token::Eof
            ])
        );
    }

    #[test]
    fn compound_assignment_tokens() {
        assert_eq!(
            tokens("+= -= *= /= %= + = ->= -=>"),
            Ok(vec![
                Token::PlusEq,
                Token::MinusEq,
                Token::StarEq,
                Token::SlashEq,
                Token::PercentEq,
                Token::Plus,
                Token::Eq,
                Token::Arrow,
                Token::Eq,
                Token::MinusEq,
                Token::Gt,
                Token::Eof
            ])
        );
        assert_eq!(tokens("//= x\n%="), Ok(vec![Token::PercentEq, Token::Eof]));
    }

    #[test]
    fn display_round_trip() {
        let source = "let mut x = f ( 1 , y , true , false ) * 2 / 3 % 2 - 4 + 5 \
                      == 6 != 7 < 8 <= 9 > 10 >= 11 && ! true || false ; \
                      x += 1 ; x -= 1 ; x *= 1 ; x /= 1 ; x %= 1 ; \
                      { } if x { } else { } while x { break ; continue } \
                      fn f ( a : int ) -> int { return } [ ] [ .. ] \
                      \"a\\\"b\\\\c\\nd\\te\"";
        let tokens = tokens(source).unwrap();
        let printed: Vec<String> = tokens
            .iter()
            .filter(|t| **t != Token::Eof)
            .map(|t| t.to_string())
            .collect();
        assert_eq!(printed.join(" "), source);
    }

    #[test]
    fn arrow_and_minus() {
        assert_eq!(
            tokens("-> - > --> -1"),
            Ok(vec![
                Token::Arrow,
                Token::Minus,
                Token::Gt,
                Token::Minus,
                Token::Arrow,
                Token::Minus,
                Token::Int(1),
                Token::Eof
            ])
        );
    }

    #[test]
    fn dot_dot() {
        assert_eq!(
            tokens("a[1..n]"),
            Ok(vec![
                ident("a"),
                Token::LBracket,
                Token::Int(1),
                Token::DotDot,
                ident("n"),
                Token::RBracket,
                Token::Eof
            ])
        );
    }

    #[test]
    fn positions_after_two_char_token() {
        let positions: Vec<String> = tokenize("a <= b")
            .unwrap()
            .iter()
            .map(|t| t.pos.to_string())
            .collect();
        assert_eq!(positions, ["1:1", "1:3", "1:6", "1:7"]);
    }

    #[test]
    fn positions() {
        let positions: Vec<String> = tokenize("let x =\n  f(42);\n")
            .unwrap()
            .iter()
            .map(|t| t.pos.to_string())
            .collect();
        assert_eq!(
            positions,
            [
                "1:1", "1:5", "1:7", "2:3", "2:4", "2:5", "2:7", "2:8", "2:9"
            ],
        );
    }

    #[test]
    fn strings() {
        assert_eq!(
            tokens(r#"print("hi", "")"#),
            Ok(vec![
                ident("print"),
                Token::LParen,
                Token::Str("hi".to_string()),
                Token::Comma,
                Token::Str(String::new()),
                Token::RParen,
                Token::Eof
            ])
        );
    }

    #[test]
    fn string_escapes() {
        assert_eq!(
            tokens(r#""a\"b\\c\nd\te""#),
            Ok(vec![Token::Str("a\"b\\c\nd\te".to_string()), Token::Eof])
        );
    }

    #[test]
    fn comment_in_string_is_text() {
        assert_eq!(
            tokens(r#""a // b""#),
            Ok(vec![Token::Str("a // b".to_string()), Token::Eof])
        );
    }

    #[test]
    fn positions_after_comment() {
        let positions: Vec<String> = tokenize("// x\n  1 // y")
            .unwrap()
            .iter()
            .map(|t| t.pos.to_string())
            .collect();
        assert_eq!(positions, ["2:3", "2:4"]);
    }

    #[test]
    fn positions_after_multiline_string() {
        let positions: Vec<String> = tokenize("\"a\nb\" x")
            .unwrap()
            .iter()
            .map(|t| t.pos.to_string())
            .collect();
        assert_eq!(positions, ["1:1", "2:4", "2:5"]);
    }

    #[test]
    fn integer_overflow() {
        assert_eq!(
            tokens("18446744073709551616"),
            Err("1:1: integer literal is too large".to_string())
        );
    }

    #[test]
    fn unexpected_character() {
        assert_eq!(
            tokens("1 $ 2"),
            Err("1:3: unexpected character: '$'".to_string())
        );
    }

    #[test]
    fn single_ampersand() {
        assert_eq!(
            tokens("a & b"),
            Err("1:3: unexpected character: '&'".to_string())
        );
    }

    #[test]
    fn single_dot() {
        assert_eq!(
            tokens("a.len"),
            Err("1:2: unexpected character: '.'".to_string())
        );
    }

    #[test]
    fn single_pipe() {
        assert_eq!(
            tokens("a | b"),
            Err("1:3: unexpected character: '|'".to_string())
        );
    }

    #[test]
    fn unterminated_string() {
        assert_eq!(
            tokens("let s = \"abc;"),
            Err("1:9: unterminated string".to_string())
        );
        assert_eq!(
            tokens("\"abc\\"),
            Err("1:1: unterminated string".to_string())
        );
    }

    #[test]
    fn unknown_escape() {
        assert_eq!(
            tokens(r#""a\qb""#),
            Err(r"1:3: unknown escape: \q".to_string())
        );
    }
}
