mod ast;
mod checker;
mod chunk;
mod compiler;
mod lexer;
mod parser;
mod runtime;
mod value;
mod vm;

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::panic;
use std::process::ExitCode;
use std::thread;

use ast::Program;
use checker::{Env, Warning};
use compiler::Scope;
use value::Value;
use vm::Vm;

// The parser and the compiler are recursive, so deeply nested input
// needs a bigger stack than the default 8 MB main thread.
const STACK_SIZE: usize = 256 * 1024 * 1024;

fn main() -> ExitCode {
    thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(cli)
        .expect("cli thread should start")
        .join()
        .unwrap_or_else(|payload| panic::resume_unwind(payload))
}

fn cli() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.as_slice() {
        [_] => {
            repl();
            ExitCode::SUCCESS
        }
        [_, cmd, path] if cmd == "run" => report(run_file(path)),
        [_, cmd, path] if cmd == "check" => report(check_file(path).map(|_| Value::Unit)),
        _ => {
            eprintln!("usage: rock [run <file> | check <file>]");
            ExitCode::from(2)
        }
    }
}

fn repl() {
    let stdin = io::stdin();
    let mut line = String::new();
    let mut env = Env::default();
    let mut scope = Scope::default();
    let mut vm = Vm::new(io::stdout());
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

        report(run(&mut vm, &mut env, &mut scope, &line));
    }
}

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))
}

fn run_file(path: &str) -> Result<Value, String> {
    let program = check_file(path)?;
    let bytecode = compiler::compile(&program, &Scope::default());
    Vm::new(io::stdout())
        .run(&bytecode)
        .map_err(|e| e.to_string())
}

// Reports parse and type errors and warnings without compiling or running the program.
fn check_file(path: &str) -> Result<Program, String> {
    let source = read(path)?;
    let (program, env, mut warnings) = check(&source, &Env::default())?;
    warnings.extend(checker::finish(&env));
    warn(warnings);
    Ok(program)
}

fn report(result: Result<Value, String>) -> ExitCode {
    match result {
        Ok(Value::Unit) => ExitCode::SUCCESS,
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

fn warn(mut warnings: Vec<Warning>) {
    warnings.sort_by_key(|w| w.pos);
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

fn run(
    vm: &mut Vm<io::Stdout>,
    env: &mut Env,
    scope: &mut Scope,
    source: &str,
) -> Result<Value, String> {
    let (program, checked, warnings) = check(source, env)?;
    warn(warnings);
    let bytecode = compiler::compile(&program, scope);
    let value = vm.run(&bytecode).map_err(|e| e.to_string())?;
    *env = checked;
    *scope = bytecode.scope;
    Ok(value)
}

fn check(source: &str, env: &Env) -> Result<(Program, Env, Vec<Warning>), String> {
    let tokens = lexer::tokenize(source)?;
    let program = parser::Parser::new(tokens)
        .parse()
        .map_err(|e| e.to_string())?;
    let (checked, warnings) = checker::check(&program, env).map_err(|e| e.to_string())?;
    Ok((program, checked, warnings))
}
