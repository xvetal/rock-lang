mod ast;
mod interp;
mod lexer;
mod parser;

use std::io::{self, BufRead, Write};

fn main() {
    repl();
}

fn repl() {
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        print!("> ");
        io::stdout().flush().expect("stdout should be writable");

        line.clear();
        let bytes = stdin
            .lock()
            .read_line(&mut line)
            .expect("stdin should be readable");
        if bytes == 0 {
            println!();
            break;
        }

        if line.trim().is_empty() {
            continue;
        }

        print_result(run(&line));
    }
}

fn print_result(result: Result<i64, String>) {
    match result {
        Ok(value) => println!("{value}"),
        Err(message) => eprintln!("error: {message}"),
    }
}

fn run(source: &str) -> Result<i64, String> {
    let tokens = lexer::tokenize(source)?;
    let expr = parser::Parser::new(tokens).parse()?;
    interp::eval(&expr)
}
