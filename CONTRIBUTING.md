# Contributing

Use a stable Rust toolchain with `rustfmt` and `clippy`. From the repository root:

```sh
cargo build --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Run the CLI against a resource checkout with:

```sh
cargo run --locked -p qbx_lint -- path/to/resource
cargo run --locked -p qbx_lint -- fmt --check path/to/resource
```

## Where changes belong

| Crate | Purpose |
| --- | --- |
| `qbx_lua_syntax` | Lua and CfxLua lexer, parser, syntax tree, and visitors. |
| `qbx_lua_fmt` | Formatter and output verification. |
| `qbx_fivem_data` | Native data, runtime stubs, and known manifest imports. |
| `qbx_lua_analysis` | Scopes, resource models, rules, configuration, and suppressions. |
| `qbx_lint` | Command-line interface and report formats. |
| `xtask` | Native data generation. |

The [language server](https://github.com/Qbox-project/qbx-lua-ls) uses sibling path dependencies
on these crates. When changing a shared API, keep the two repositories side by side and run the
language server's tests too.

## Tests and fixtures

Include a small reproduction when changing parser behavior, a lint rule, or an automatic edit.
For a rule change, cover both code that should be reported and a similar valid case. CLI
regressions belong in `crates/qbx_lint/tests`; analysis tests and fixture resources are under
`crates/qbx_lua_analysis/tests`.

Fixture snapshots can be regenerated after an intentional diagnostic change. In a POSIX shell:

```sh
QBX_BLESS=1 cargo test --locked -p qbx_lua_analysis --test fixtures
```

In PowerShell:

```powershell
$env:QBX_BLESS = '1'
cargo test --locked -p qbx_lua_analysis --test fixtures
Remove-Item Env:QBX_BLESS
```

Review the snapshot diff before accepting it, then rerun tests without `QBX_BLESS`.

## Generated data

Native signatures and documentation are checked in. To refresh them from the configured FiveM
data endpoints:

```sh
cargo xtask natives
```

The generator rejects a download that would reduce the native count by more than 10%. Inspect
the data diff even when generation succeeds. The [natives workflow](.github/workflows/natives.yml)
also proposes updates.

The GLM stub generator requires Node.js with built-in `fetch`. It accepts either a local binding
source directory or a base URL, and uses the configured CfxLua source URL when omitted:

```sh
node scripts/generate-glm-stub.mjs
node scripts/generate-glm-stub.mjs path/to/glm-binding
```

Data generation makes network requests unless local sources are supplied where supported;
ordinary tests use the checked-in data.

## Reporting a bug

Include `qbx-lint --version`, your platform, the command you ran, expected and actual output,
and a minimal Lua example. Include the manifest and relevant `qbxlint.toml` settings when the
problem depends on imports, side selection, or resource layout.

For a pull request, explain the resulting behavior and the checks you ran. Keep generated-data
updates identifiable in the diff. Release notes are generated from commit messages, so write them as [Conventional Commits](https://www.conventionalcommits.org).
