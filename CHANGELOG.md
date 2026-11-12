# Changelog

## [Unreleased]

## [0.2.0] - 2026-11-12

### Added
- Interactive REPL: run `rock` without arguments
- `rock run <file>` evaluates an expression from a file
- Exit codes: 0 on success, 1 on errors, 2 on invalid usage
- Unary minus: `-5`, `-(1 + 2)`, `2 * -3`
- Integer literal `-9223372036854775808` (`i64::MIN`)

## [0.1.0] - 2026-11-12

### Added
- Lexer for integer literals, `+ - * /` and parentheses
- Recursive descent parser with operator precedence and left associativity
- Tree-walking interpreter with division-by-zero and overflow errors
