# qbx-lint

A fast linter for FiveM Lua. It parses CfxLua 5.4 natively, reads `fxmanifest.lua` to learn which
globals exist on which side, and knows every FiveM native — so it needs no hand-maintained list of
"extra libraries" and can catch mistakes a generic Lua linter cannot see.

> Status: proof of concept. The rule set and config format may still change.

## Why not luacheck?

The current Qbox workflow runs luacheck through `iLLeniumStudios/fivem-lua-lint-action`. That setup

- needs an `extra_libs` list (`ox_lib+mysql+qbox+...`) that has to be kept in sync by hand,
- ships a natives snapshot that is years old,
- cannot parse CfxLua syntax such as `` `hash` ``, `a?.b`, `x += 1` or `local a, b in t` without patches,
- has no idea whether a file runs on the client or the server.

`qbx-lint` replaces all of that with one static binary:

| | luacheck action | qbx-lint |
| --- | --- | --- |
| CfxLua syntax (hashes, `?.`, `+=`, `in`, set constructors, `defer`, `/* */`) | partial | full |
| Globals from `fxmanifest.lua` (scripts, `@resource/file.lua` imports) | no | yes |
| Natives | stale snapshot | 9,900+ generated from the live docs, with client/server side |
| Client native used in a server script | no | `fivem/native-wrong-side` |
| `while true do` without `Wait` | no | `fivem/loop-never-yields` |
| `source` read after `Wait` | no | `fivem/source-after-yield` |
| Manifest validation | no | `manifest/*` |
| Automatic fixes | no | `--fix` |
| Event/export call checks, security and locale rules | no | yes, across files and resources |
| Formatter | no | `qbx-lint fmt`, verified output |
| PR annotations | via JUnit converter | native (`--format github`), plus SARIF/JUnit/JSON |
| Runtime | Docker image + Lua interpreter | single static binary; qbx_core (55 files) lints in ~30 ms |

## Usage

```bash
qbx-lint                      # lint the current directory
qbx-lint resources/[qbx]      # lint many resources at once
qbx-lint --fix                # apply safe automatic fixes
qbx-lint --format github      # GitHub Actions annotations
qbx-lint --list-rules         # every rule, its default level and category
qbx-lint --rule unused-argument=warning --max-warnings 0
```

Exit code `1` means at least one error (or more warnings than `--max-warnings`); `2` means the
tool itself could not run.

### GitHub Actions

```yaml
- uses: actions/checkout@v4
- uses: Qbox-project/qbx-lint@v1
```

See [examples/lint.yml](examples/lint.yml) for a drop-in replacement of the existing Qbox workflow.

## How globals are resolved

For every file the linter looks for the nearest `fxmanifest.lua` and builds the environment the
file really runs in:

1. The Lua 5.4 standard library and the CfxLua runtime (`Citizen`, `exports`, `json`, `vector3`,
   state bags, ...), defined by the annotated stubs in `crates/qbx_fivem_data/stubs`.
2. Every FiveM native, including `N_0x...` hash names. Each native knows whether it is client,
   server or shared.
3. Globals defined by any script the manifest loads **on the same side** as the file.
4. `@resource/file.lua` imports. If the other resource is on disk (a full server checkout) the
   file is read; otherwise a built-in table of well-known imports is used (`@ox_lib/init.lua` →
   `lib`, `cache`; `@oxmysql/lib/MySQL.lua` → `MySQL`; `@qbx_core/modules/lib.lua` → `qbx`; ...).
5. `globals` from `qbxlint.toml`.

Files that the manifest does not list as scripts (modules loaded with `require` / `lib.load`)
are checked against the union of both sides.

### Escrowed resources

Files protected by the FiveM asset escrow keep the `.lua` extension but contain ciphertext (they
start with `FXAP`). They are never parsed, formatted or indexed. A resource that ships a `.fxap`
marker, or contains such a file, is treated as *opaque*: the readable part (usually `config.lua`)
is still linted, but `undefined-global` and `qbox/unused-locale-key` stay silent, because the
encrypted scripts may define any global and use any locale key.

### Start order and installed resources

When the resource lives in the `resources` folder next to a `server.cfg`, that cfg's `ensure` /
`start` lines (including `exec`'d cfg files and `ensure [category]`) are read. A resource that the
cfg starts earlier counts as a satisfied dependency, so `manifest/missing-dependency` is only
reported when neither the manifest nor the cfg settles the order. Resources started by the same
`ensure [category]` line have no defined order relative to each other.

The same folder tells the linter what is installed (names a manifest `provide`s included). An
export of a resource that does not exist is reported as `fivem/resource-not-found`, but only
when the call is unconditional.

### Bridge code

Bridges pick one integration out of many at runtime:

```lua
if Config.Inventory == 'ox' then
    exports.ox_inventory:AddItem(...)
elseif Config.Inventory == 'qs' then
    exports['qs-inventory']:AddItem(...)
end
```

None of these is a requirement and most are not installed, so exports used inside a branch whose
condition compares against a string, or after a guard such as
`if Config.Framework ~= 'esx' then return end`, produce neither `manifest/missing-dependency` nor
`fivem/resource-not-found`.

## Rules

Run `qbx-lint --list-rules` for the authoritative list. Highlights:

| Rule | Default | What it catches |
| --- | --- | --- |
| `undefined-global` | warning | reading a global nothing defines |
| `fivem/import-not-declared` | warning | using `lib`, `MySQL`, `qbx`, ... without the manifest import for that side |
| `fivem/native-wrong-side` | error | `PlayerPedId()` in a server script, `TriggerClientEvent` on the client |
| `fivem/loop-never-yields` | error | infinite loops with no `Wait`, which freeze the game/server |
| `fivem/source-after-yield` | warning | reading global `source` after `Wait`/`await` or inside `SetTimeout`/`CreateThread` callbacks |
| `fivem/citizen-prefix` | info, fixable | `Citizen.Wait` → `Wait` |
| `fivem/hash-literal` | info, fixable | ``GetHashKey('adder')`` → `` `adder` `` |
| `fivem/legacy-native-pattern` | info | `GetPlayerPed(-1)`, `GetDistanceBetweenCoords`, `Vdist` |
| `qbox/prefer-cache` | hint | `PlayerPedId()` when ox_lib's `cache.ped` is available |
| `qbox/legacy-core-object` | hint | `exports['qb-core']:GetCoreObject()` |
| `fivem/event-argument-count` | warning | `TriggerServerEvent('x', a, b, c)` when the handler of `x` only takes two parameters |
| `fivem/event-missing-arguments` | info | fewer arguments than the handler declares (fine for optional parameters, a bug otherwise) |
| `fivem/event-wrong-side` | warning | `TriggerServerEvent` for an event that is only handled on the client, and the reverse |
| `fivem/export-argument-count`, `fivem/unknown-export` | warning / info | calls to `exports.resource:Fn()` checked against the resource's `exports('Fn', ...)` |
| `security/client-supplied-source` | warning | server net event that takes the player id as an argument instead of using `source` |
| `security/unvalidated-event-argument` | warning | a client-sent value reaches `AddMoney`, `AddItem`, `SetJob`, `ExecuteCommand`, `load`, ... without appearing in any check |
| `security/sql-concatenation` | warning | queries built with `..` or `:format()` and no parameter table |
| `qbox/unknown-locale-key`, `qbox/unused-locale-key` | warning / info | `locale('key')` validated against `locales/en.json`, unused keys reported on the JSON file |
| `manifest/missing-dependency` | info | `exports.foo` used without `dependency 'foo'` (skipped for bridge code, when guarded by `GetResourceState`, or when `server.cfg` starts `foo` earlier) |
| `fivem/resource-not-found` | warning | an unconditional `exports.foo:...` call while no resource `foo` exists in the server's resources folder |
| `manifest/lua54`, `manifest/missing-file`, `manifest/unknown-directive` | warning | broken or misspelled manifest entries |
| `unused-local`, `unused-function`, `redefined-local`, `unreachable-code`, `duplicate-index`, `const-reassign`, `unbalanced-assignments`, `self-assignment`, `lowercase-global`, `implicit-global`, `builtin-overwrite`, `undefined-field`, `deprecated`, ... | varies | the usual Lua mistakes |

### Suppressing findings

```lua
-- qbx-lint: disable-next-line unused-local
local keepMe = 1

local x = compute() -- qbx-lint: disable-line

---@diagnostic disable-next-line: undefined-global   (LuaLS style works too)
print(fromSomewhereElse)

-- qbx-lint: disable fivem/citizen-prefix
...
-- qbx-lint: enable fivem/citizen-prefix
```

Existing `-- luacheck: ignore` comments are honoured, so migrating does not require touching code.

The cross-file rules work in two passes: every handler and export under the linted paths is
collected first, then each trigger and export call is checked against them. Events and resources
that are not part of the run are simply not checked, so linting a single resource never produces
guesses about code it cannot see.

## Formatting

```bash
qbx-lint fmt                  # format every Lua file under the current directory
qbx-lint fmt --check          # CI mode: exit 1 if anything would change
```

The formatter is deliberately conservative about taste: it fixes spacing, indentation and line
length, but keeps one-line guards (`if not player then return end`), tables the author expanded,
trailing commas as written, inline casts (`value --[[@as number]]`) and blank-line grouping.

It is also paranoid about safety. After printing, the result is lexed again and must contain
exactly the same tokens (string contents compared after unescaping) and the same comments as the
input; otherwise the file is left untouched and reported. Statements with a comment in a position
the printer has no slot for are copied verbatim instead of guessing. All 339 files of the
validation corpus format this way, and formatting them twice changes nothing.

```toml
[format]
indent_width = 4
use_tabs = false
line_width = 120
quote_style = "preserve"   # or "single" / "double"; only applied where no escaping is needed
```

## Configuration

Optional `qbxlint.toml` (or `.qbxlint.toml`) anywhere above the linted files:

```toml
exclude = ["web/**"]
globals = ["SomeRuntimeGlobal"]

[rules]
"unused-argument" = "off"
"fivem/citizen-prefix" = "warning"

[[overrides]]
files = ["**/tests/**"]
rules = { "undefined-global" = "off" }
```

See [examples/qbxlint.toml](examples/qbxlint.toml).

## Workspace layout

| Crate | Purpose |
| --- | --- |
| `qbx_lua_syntax` | Error-tolerant lexer/parser for Lua 5.4 + CfxLua. No dependencies besides `smol_str`. |
| `qbx_lua_fmt` | The formatter and its output verifier. |
| `qbx_fivem_data` | Embedded natives (sorted TSV, binary-searched in place), runtime stubs, known imports. |
| `qbx_lua_analysis` | Scope resolution, manifest/project model, rules, config, suppression directives. |
| `qbx_lint` | The `qbx-lint` CLI and output formats. |
| `xtask` | `cargo xtask natives` regenerates the natives data from `runtime.fivem.net`. |

`qbx_lua_syntax`, `qbx_fivem_data` and `qbx_lua_analysis` are also the foundation of
[`qbx-lua-ls`](../qbx-lua-ls), so the editor and CI always agree.

## Development

```bash
cargo test --workspace
cargo xtask natives                     # refresh natives.tsv / natives_docs.tsv
QBX_BLESS=1 cargo test -p qbx_lua_analysis --test fixtures   # accept new fixture output
```

The linter is validated against real code: qbx_core, qbx_police, qbx_vehicleshop, ox_lib and
ox_inventory (229 files) parse with zero syntax errors and lint without false positives.

## License

GPL-3.0-or-later
