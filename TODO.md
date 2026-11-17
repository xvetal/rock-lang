# TODO

## Parser
- [ ] Switch to a Pratt parser when more precedence levels appear

## Checker and compiler
- [ ] Avoid stack overflow on very long operator chains: `1 + 1 + … + 1` with a million terms still crashes
