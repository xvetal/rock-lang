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
use checker::{CheckError, Checked, Env, Warning};
use compiler::Scope;
use value::Value;
use vm::Vm;

// Nesting is limited by the parser, but a long chain like `1 + 1 + ...` still
// builds a deep tree, and the checker and the compiler walk it recursively.
// That needs a bigger stack than the default 8 MB main thread.
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

fn run_file(path: &str) -> Result<Value, Errors> {
    let program = check_file(path)?;
    let bytecode = compiler::compile(&program, &Scope::default());
    let value = Vm::new(io::stdout())
        .run(&bytecode)
        .map_err(|e| e.to_string())?;
    Ok(value)
}

// The whole file is known, so top-level variables get their warnings too.
fn check_file(path: &str) -> Result<Program, Errors> {
    let source = read(path)?;
    let (program, checked) = check(&source, &Env::default())?;
    let mut warnings = checked.warnings;
    warnings.extend(checker::finish(&checked.env));
    diagnose(checked.errors, warnings)?;
    Ok(program)
}

struct Errors(Vec<String>);

impl From<String> for Errors {
    fn from(message: String) -> Self {
        Errors(vec![message])
    }
}

impl<E: ToString> From<Vec<E>> for Errors {
    fn from(errors: Vec<E>) -> Self {
        Errors(errors.iter().map(ToString::to_string).collect())
    }
}

fn report(result: Result<Value, Errors>) -> ExitCode {
    match result {
        Ok(Value::Unit) => ExitCode::SUCCESS,
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(Errors(messages)) => {
            for message in messages {
                eprintln!("error: {message}");
            }
            ExitCode::FAILURE
        }
    }
}

fn diagnose(errors: Vec<CheckError>, warnings: Vec<Warning>) -> Result<(), Errors> {
    warn(warnings);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.into())
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
) -> Result<Value, Errors> {
    let (program, checked) = check(source, env)?;
    diagnose(checked.errors, checked.warnings)?;
    let bytecode = compiler::compile(&program, scope);
    let value = vm.run(&bytecode).map_err(|e| e.to_string())?;
    *env = checked.env;
    *scope = bytecode.scope;
    Ok(value)
}

fn check(source: &str, env: &Env) -> Result<(Program, Checked), Errors> {
    let tokens = lexer::tokenize(source)?;
    let program = parser::Parser::new(tokens).parse()?;
    let checked = checker::check(&program, env);
    Ok((program, checked))
}
