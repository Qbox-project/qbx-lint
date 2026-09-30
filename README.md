# qbx-lint

A Lua linter and formatter for FiveM resources. It parses CfxLua syntax, reads resource manifests,
and checks code against bundled native definitions, runtime globals, and client/server context.

It reports Lua mistakes, missing manifest imports, event and export mismatches, and common Qbox
patterns. The same workspace contains the parser, formatter, analysis crates and
[qbx-lua-ls](crates/qbx_lua_ls), the server behind the
[Qbox Lua editor integrations](https://github.com/Qbox-project/qbx-editor).

Shared analysis and language-server changes can be developed and tested in one pull request.
See the [language-server guide](crates/qbx_lua_ls/README.md) for features and setup, and the
[migration notes](docs/repository-migration.md) for Git history and release compatibility.

## Install

Build from a checkout with a stable Rust toolchain and Cargo:

```sh
git clone https://github.com/Qbox-project/qbx-lint.git
cd qbx-lint
cargo install --path crates/qbx_lint --locked --profile lint-release
qbx-lint --version
```

Cargo installs the executable into its `bin` directory, which must be on your `PATH`.
For a local build without installation, run `cargo build --profile lint-release --locked -p qbx_lint`.
The executable is then in `target/lint-release` (`qbx-lint.exe` on Windows).

Download the archive for your platform from the
[releases page](https://github.com/Qbox-project/qbx-lint/releases), extract it, and put the
executable on your `PATH`.

## Use

Run these commands from your resource or server checkout:

```sh
qbx-lint                                      # lint the current directory
qbx-lint 'resources/[qbx]'                     # lint a resource category
qbx-lint client/main.lua server/main.lua       # lint selected files
qbx-lint --fix                                # apply available fixes in place
qbx-lint --max-warnings 0                      # fail on warnings as well as errors
qbx-lint --format json --output lint.json      # write a report
qbx-lint --min-severity hint                   # include hint-level findings
qbx-lint --list-rules                          # show rules and their default levels
```

Output formats are `pretty`, `compact`, `json`, `junit`, `github`, and `sarif`. By default, the CLI
shows findings at `info` level and above. Lint exits with `1` for errors or an exceeded warning
limit, and `2` for an invocation or processing failure. `--no-fail` suppresses failures caused
by lint findings.

To format files:

```sh
qbx-lint fmt
qbx-lint fmt --check
qbx-lint fmt --config qbxlint.toml client/main.lua
```

`fmt --check` leaves files untouched and exits with `1` if formatting is needed. Formatting also
fails if a source file cannot be formatted. The formatter verifies tokens and comments before
writing. Both formatting and automatic fixes reject non-UTF-8 source; escrow, binary and
obfuscated files are skipped.

## Configure

The CLI searches upward from the first input path for `qbxlint.toml` or `.qbxlint.toml`.
Use `--config path/to/qbxlint.toml` to select a file explicitly. Paths in configuration patterns
are relative to that file's directory.

```toml
exclude = ["web/**", "**/vendor/**"]
ignore_diagnostics = ['\[standalone\]/']
globals = ["SomeRuntimeGlobal"]

[rules]
"unused-argument" = "off"
"fivem/citizen-prefix" = "warning"

[[overrides]]
files = ["tests/**"]
rules = { "undefined-global" = "off" }

[format]
indent_width = 4
use_tabs = false
line_width = 120
quote_style = "preserve"
```

See the [configuration and analysis reference](docs/reference.md), the
[rule list](docs/rules.md), and the [example configuration](examples/qbxlint.toml).

## GitHub Actions

Add the action to your workflow:

```yaml
- uses: actions/checkout@v4
- uses: Qbox-project/qbx-lint@v1.0.5
  with:
    version: v1.0.5
    paths: .
    args: --max-warnings 0
```

The action emits GitHub annotations and writes a SARIF report. See
[examples/lint.yml](examples/lint.yml) and [action.yml](action.yml) for its inputs and output.

## Limits

Analysis depends on the files available in the run. Dynamic names, runtime loading, encrypted
scripts, and JavaScript or C# resources limit what it can infer. Native definitions are bundled
data, not a live lookup. Security rules flag patterns for review; a clean report does not establish
that a resource is secure. Review automatic edits and test resource behavior in FiveM.

## Contribute

See [CONTRIBUTING.md](CONTRIBUTING.md) for the crate layout, checks, and data generation commands.
Release notes are on the [releases page](https://github.com/Qbox-project/qbx-lint/releases).

## License

[GPL-3.0-or-later](LICENSE).
