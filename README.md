# Rock

A toy programming language written in Rust, built for learning.

## Status

Version 0.8.0: programs are statically typed and type-checked before they run, then executed on a bytecode virtual machine.

- Integer and boolean literals, `+ - * /`, unary minus, parentheses
- Comparisons `== != < <= > >=` and logical `! && ||` with short-circuit
- Operator precedence and left associativity
- Variables with `let`, immutable unless declared with `let mut`, including shadowing, and assignment `x = value;`
- Blocks with lexical scope; `if`/`else` and blocks are expressions
- `while` loops with `break` and `continue`
- Functions with `fn` and `return`, including recursion; parameters and return values have types, e.g. `fn add(a: int, b: int) -> int`; functions are values, can be called before their declaration, and see only their parameters and locals
- The built-in `print`
- Static types `int`, `bool`, `()` and function types such as `fn(int) -> int`
- Errors with line and column: parse errors, type errors (mismatched types, undefined names, wrong argument counts, assignment to immutable variables), and runtime errors (division by zero, integer overflow, stack overflow)
- Interactive REPL, running programs from a file, and checking them without running with `rock check`

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
