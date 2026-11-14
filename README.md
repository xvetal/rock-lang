# Rock

A toy programming language written in Rust, built for learning.

## Status

Version 0.4.0: booleans, blocks, `if`/`else`, assignment, and `while` loops.

- Integer and boolean literals, `+ - * /`, unary minus, parentheses
- Comparisons `== != < <= > >=` and logical `! && ||` with short-circuit
- Operator precedence and left associativity
- Variables with `let`, including shadowing, and assignment `x = value;`
- Blocks with lexical scope; `if`/`else` and blocks are expressions
- `while` loops
- Function calls and the built-in `print`
- Errors with line and column for parse errors; division by zero, integer overflow, undefined names, and type errors at runtime
- Interactive REPL and running programs from a file

## Goals

- Learn how lexers, parsers, and interpreters work
- Build a bytecode virtual machine
- Eventually generate native code

## Example

```rock
let n = 5;
let acc = 1;
while n > 1 {
    acc = acc * n;
    n = n - 1;
}
print(acc);
acc > 100
```

Running it prints `120` from `print`, then `true`, the value of the final expression.

## Usage

Install the `rock` binary from the repository root:

```sh
cargo install --path .
```

Start the interactive REPL (exit with Ctrl+D):

```sh
rock
> let x = 1 + 2 * 3;
> x * 2
14
```

Run a program from a file:

```sh
rock run examples/print.rk
```
