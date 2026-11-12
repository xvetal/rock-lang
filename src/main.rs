mod ast;
mod interp;
mod lexer;
mod parser;

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.as_slice() {
        [_] => {
            repl();
            ExitCode::SUCCESS
        }
        [_, cmd, path] if cmd == "run" => report(run_file(path)),
        _ => {
            eprintln!("usage: rock [run <file>]");
            ExitCode::from(2)
        }
    }
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

        report(run(&line));
    }
}

fn run_file(path: &str) -> Result<i64, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    run(&source)
}

fn report(result: Result<i64, String>) -> ExitCode {
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(source: &str) -> Result<i64, String> {
    let tokens = lexer::tokenize(source)?;
    let expr = parser::Parser::new(tokens).parse()?;
    interp::eval(&expr)
}
