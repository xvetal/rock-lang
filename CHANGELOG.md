# Changelog

## [Unreleased]

### Added
- Interactive REPL: run `rock` without arguments
- `rock run <file>` evaluates an expression from a file
- Exit codes: 0 on success, 1 on errors, 2 on invalid usage

## [0.1.0] - 2026-11-12

### Added
- Lexer for integer literals, `+ - * /` and parentheses
- Recursive descent parser with operator precedence and left associativity
- Tree-walking interpreter with division-by-zero and overflow errors
