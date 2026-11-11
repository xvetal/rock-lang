mod lexer;

fn main() {
    println!("{:?}", lexer::tokenize("12 + (3*4)"));
    println!("{:?}", lexer::tokenize("1 $ 2"));
    println!("{:?}", lexer::tokenize(""));
    println!("{:?}", lexer::tokenize("123+"));
    println!("{:?}", lexer::tokenize("  42  "));
    println!("{:?}", lexer::tokenize("9223372036854775807"));
    println!("{:?}", lexer::tokenize("9999999999999999999"));
}
