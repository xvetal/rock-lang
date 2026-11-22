# TODO

## Lexer
- [ ] Block comments `/* … */`

## Parser
- [ ] Switch to a Pratt parser when more precedence levels appear

## Ranges
- [ ] Ranges as values, such as `let r = 0..n;`, with their own type, printing and slicing with `s[r]`
- [ ] Inclusive ranges `a..=b`

## Strings
- [ ] Iterating over the characters of a string, once there is a `char` type

## Arrays
- [ ] The index together with the element, as in `for (i, x) in enumerate(a)`, once there are tuples
- [ ] `pop`, once there is `Option` to return from an empty array

## Methods
- [ ] Method syntax such as `a.push(1)` and `a.len()`, together with structs
