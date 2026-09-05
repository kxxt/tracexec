# Guidelines for Agents

## Requirement

- If the user asks you to create a pull request, please include a file named `.agent-created-pr` in it.
- Make sure `cargo clippy --workspace` and `cargo +nightly fmt --all --check` check pass.
- `README.md` should be updated through `README.template.md` and `just update-readme`. 

## Development Guideline

- Use rust idioms.
- Minimize `unwrap` and `expect` usage.
- Avoid `unsafe` code unless necessary.
- Do ablation study on your diffs. Drop unnecessary changes but never remove security related checks to make the diff look smaller.
- Add tests that really matters.
- Document invariants that are **not obvious** in comments.
- Do not use the docs as your daybook.
 