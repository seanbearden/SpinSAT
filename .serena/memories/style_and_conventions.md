# Style and Conventions

## Rust Style
- Follow standard Rust conventions (rustfmt defaults)
- Use snake_case for functions and variables
- Use CamelCase for types and structs
- Prefer iterators over index loops where idiomatic
- Use `f64` for all numerical computation (voltages, memories, derivatives)

## Code Organization
- One module per concern (parser, formula, dmm, integrator, solver)
- Keep the inner integration loop as tight as possible — this is the hot path
- Avoid heap allocations in the integration loop
- Pre-allocate all arrays before the solve loop

## Naming Conventions (match paper notation)
- `v` or `voltages` — voltage array
- `x_s` or `x_short` — short-term memory
- `x_l` or `x_long` — long-term memory
- `c_m` — clause constraint values
- `g_nm` — gradient-like function
- `r_nm` — rigidity function
- `q` — polarity matrix

## Task Completion Checklist
1. cargo fmt
2. cargo clippy (no warnings)
3. cargo test (all pass)
4. Test on at least one .cnf instance
5. Verify output matches competition format
