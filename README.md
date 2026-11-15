# Rock

A toy programming language written in Rust, built for learning.

## Status

Version 0.7.0: programs run on a bytecode virtual machine, and names are checked before the program runs.

- Integer and boolean literals, `+ - * /`, unary minus, parentheses
- Comparisons `== != < <= > >=` and logical `! && ||` with short-circuit
- Operator precedence and left associativity
- Variables with `let`, including shadowing, and assignment `x = value;`
- Blocks with lexical scope; `if`/`else` and blocks are expressions
- `while` loops with `break` and `continue`
- Functions with `fn` and `return`, including recursion; functions are values, can be called before their declaration, and see only their parameters and locals
- The built-in `print`
- Errors with line and column: parse errors, compile errors (undefined names, duplicate functions, wrong `print` arguments), and runtime errors (division by zero, integer overflow, type errors, stack overflow)
- Interactive REPL and running programs from a file

## Goals

- Learn how lexers, parsers, and interpreters work
- Build a bytecode virtual machine
- Eventually generate native code

## Example

```rock
let mut n = 5;
let mut acc = 1;
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

Check a program for errors without running it:

```sh
rock check examples/print.rk
```
