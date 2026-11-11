mod ast;
mod lexer;

fn main() {
    println!("{:?}", lexer::tokenize("12 + (3*4)"));
}
