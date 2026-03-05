# Contributing

Thanks for contributing to `prompt-compress`.

## Local Development

Build and test before opening a PR:

```bash
cargo check --all
cargo clippy --all
cargo test --all
cargo build --release
```

## Wiki Publishing

Wiki pages are maintained in-repo under:

- `wiki/`

Publish to GitHub Wiki:

```bash
./scripts/publish-wiki.sh
```

If the wiki remote is not initialized yet, create the first page once in the browser:

- https://github.com/DevvGwardo/prompt-compress/wiki

Then rerun the publish script.

If push authentication fails, run:

```bash
gh auth login
gh auth setup-git
```
