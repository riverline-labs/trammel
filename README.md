# trammel

Config-driven architectural conformance for Rust workspaces.

## What it is

`trammel` parses your Rust source with `syn`, classifies each file into a layer
(per `trammel.toml`), and runs a small set of rule kinds — forbidden imports,
forbidden inline paths, forbidden macros, forbidden methods, required struct
attributes, n+1 query detection, filesystem layout assertions, and content
scans — to keep architectural boundaries from drifting at review time.

It's a single binary with a config file. No editor integration, no rule DSL,
no severity levels, no in-source suppression comments (other than the
`#[allow_n_plus_one]` marker from the companion `trammel-attrs` crate).

## Install

```sh
cargo install --locked trammel
```

For the n+1 opt-out attribute, add to your project:

```toml
[dependencies]
trammel-attrs = "0.1"
```

## Configure

Create `trammel.toml` at your project root. The full schema lives in this
README under [Configuration](#configuration) (filled in as features land).

## Run

```sh
trammel check
```

Exit 0 = clean, 1 = violations, 2 = config or IO error.

## License

Apache 2.0. See `LICENSE` and `NOTICE`.
