# Configuration and analysis reference

## Syntax

The parser handles Lua 5.4 and CfxLua extensions, including backtick hash literals, optional
chaining (`value?.field`), compound assignments (`count += 1`), `local a, b in object`, set
constructors (`{ .police, .ambulance }`), `defer` blocks, and `/* ... */` comments.

## Configuration files

`qbx-lint` loads one configuration per invocation. Without `--config`, it searches upward from
the first input file or directory for `qbxlint.toml` or `.qbxlint.toml`. An explicit configuration
path can be relative or absolute. Exclusion and override patterns use the configuration directory
as their base.

All settings are optional. Unknown keys and rule names are errors.

```toml
exclude = ["web/**", "**/vendor/**"]
globals = ["SomeRuntimeGlobal"]
ignore_unused_prefix = "_"

[rules]
"unused-argument" = "off"
"fivem/citizen-prefix" = "warning"
"qbox/prefer-cache" = "info"

[[overrides]]
files = ["tests/**", "**/*.spec.lua"]
globals = ["describe", "it"]
rules = { "undefined-global" = "off" }

[format]
indent_width = 4
use_tabs = false
line_width = 120
quote_style = "preserve"
```

| Setting | Meaning |
| --- | --- |
| `exclude` | Additional glob patterns for paths to skip. |
| `globals` | Names supplied at runtime that the linter cannot discover. |
| `ignore_unused_prefix` | Locals and arguments with this prefix are exempt from unused checks; defaults to `_`. |
| `rules` | Per-rule levels: `off`, `hint`, `info`, `warning` (or `warn`), and `error`. |
| `overrides` | Per-file globals and rule levels, selected by the `files` patterns. Later matching overrides take precedence for rule levels. |
| `format` | Formatting options, shown with their defaults above. |

The default exclusions include `node_modules`, `.git`, and `[builders]` directory contents.
Directory traversal also skips hidden directories and does not follow directory symlinks.

`--rule CODE=LEVEL` changes a rule's base level for the invocation. Matching file overrides are
applied afterward. `--min-severity hint` includes hints, which the default `info` threshold hides.
`--max-warnings 0` makes reported warnings fail a lint run.

## Suppressing findings

Use a rule code to keep the suppression specific:

```lua
-- qbx-lint: disable-next-line unused-local
local keepMe = 1

print(externalValue) -- qbx-lint: disable-line undefined-global

-- qbx-lint: disable fivem/citizen-prefix
Citizen.Wait(0)
-- qbx-lint: enable fivem/citizen-prefix
```

Omitting rule codes suppresses all findings in the selected line or range, except syntax errors.
Comma-separated and space-separated rule codes are accepted. LuaLS-style directives are also
recognized:

```lua
---@diagnostic disable-next-line: undefined-global
print(fromSomewhereElse)
```

`-- luacheck: ignore` is recognized as a broad suppression: on its own line it disables subsequent
findings; after code it suppresses that line. Luacheck's numeric codes and name filters are not
translated. Prefer explicit `qbx-lint` rule codes when narrowing a suppression.

## Globals and client/server context

For a file inside a resource, the nearest `fxmanifest.lua` or `__resource.lua` supplies script
paths and sides. The analysis combines:

1. Lua and CfxLua runtime definitions from the [bundled stubs](../crates/qbx_fivem_data/stubs).
2. Bundled FiveM native signatures and client/server metadata, including `N_0x...` names.
3. Globals defined by scripts available on the file's side.
4. Globals from manifest imports such as `@resource/file.lua`.
5. Configured `globals`.

An import is read from a sibling resource when available. Otherwise, known imports supply their
usual globals, such as `lib` and `cache` for `@ox_lib/init.lua`, `MySQL` for
`@oxmysql/lib/MySQL.lua`, and `qbx` for `@qbx_core/modules/lib.lua`.

Files not listed as manifest scripts, including modules loaded through `require` or `lib.load`,
use globals from both sides. Files outside a resource are checked without a manifest environment.

Recognized runtime guards narrow side checks within a file. Examples include
`IsDuplicityVersion()`, a local flag initialized from it, and `lib.context == 'server'`.
An early return such as `if not IsDuplicityVersion() then return end` narrows the following code
to the server. Event registrations inside these regions use that effective side.

## Events, exports, and locales

The CLI first collects event registrations and exports from the resources being analyzed, then
checks their callers. When an input belongs to a resource, the other Lua files in that resource
contribute to analysis even if only one file was requested. Including related resources in the
same run provides more context for calls between resources.

Static event names can be checked for the target side and missing or excess arguments. Export
checks report excess arguments and unknown names. Unavailable resources and computed names limit
these checks. A diagnostic about missing event arguments may describe an intentional optional
parameter.

Locale checks compare static `locale('key')` calls with `locales/en.json`, falling back to the first
JSON file in `locales/` when that file is absent. Unused keys are reported on the JSON file when
the run includes the resource manifest. Dynamic locale usage limits which keys can be considered
unused.

## Escrowed, obfuscated and mixed-language resources

Encrypted FiveM files beginning with `FXAP`, Lua bytecode, and detected binary blobs are skipped.
Obfuscated or minified files are skipped too: a file counts as obfuscated when one of its lines is
at least 4096 bytes long and contains the `function` keyword. Long data lines, such as tables or
encoded strings, do not count. To skip other generated files, use `exclude`.

A `.fxap` marker or a skipped or unreadable script makes the resource opaque to checks that need
complete knowledge of its globals or locale usage. Readable scripts are still analyzed.

Unknown-export checks are suppressed for opaque resources and resources with non-Lua scripts.
An opaque resource may also handle events named with its `resource:` prefix, so a wrong-side
diagnostic is suppressed when the missing handler could be in that resource. Computed export
registrations similarly prevent a complete list of exports.

Read-only linting uses replacement characters for invalid UTF-8 bytes. `--fix`, `fmt`, and
`fmt --check` report an encoding error for such source, and do not rewrite it. Convert its encoding
explicitly before using editing commands.

## Start order and optional integrations

For a resource below the `resources` directory next to a `server.cfg`, the linter reads `ensure`
and `start` commands, included `exec` files, and `ensure [category]` groups. A resource that starts
earlier satisfies a dependency check. Resources started by the same category command have no
relative order in this model.

The resources directory also supplies installed resource names, including manifest `provide`
aliases. `fivem/resource-not-found` reports an export call to a missing resource only when that
call is outside an `if`. Its default level is `info`, because the enclosing function may never run.

Dependency checks make allowances for integrations selected at runtime:

```lua
if Config.Inventory == 'ox' then
    exports.ox_inventory:AddItem(source, 'water', 1)
elseif Config.Inventory == 'qs' then
    exports['qs-inventory']:AddItem(source, 'water', 1)
end
```

Recognized string comparisons and early-return guards suppress missing-dependency and
resource-not-found findings for the selected code. The same allowances apply inside `pcall` or
`xpcall`, in files below `bridge`, `bridges`, `framework`, `frameworks`, `compat`, or `integrations`,
and in resource files not loaded as manifest scripts. Dependency checks also account for
`GetResourceState` usage. These are heuristics, not proof that a dependency is available at runtime.

## Formatting and fixes

```sh
qbx-lint fmt resources
qbx-lint fmt --check resources
qbx-lint fmt --config config/qbxlint.toml resources
qbx-lint --fix resources
```

The formatter adjusts spacing, indentation, and line wrapping while preserving constructs such
as one-line guards, expanded tables, trailing commas, inline casts, and blank-line grouping.
`quote_style` accepts `preserve`, `single`, and `double`; quote changes are limited to strings that
do not need new escapes.

Before writing, the formatter compares the input and output tokens and comments. String contents
are compared after decoding escapes. A verification failure leaves that file unchanged. Statements
with comments the printer cannot place are retained verbatim. Syntax errors also prevent formatting.

`--fix` applies available, non-overlapping fixes and then lints again. The candidate output must
parse before it is written. A source file that changed after analysis is not overwritten. Hash
literal replacements are offered in value positions; a standalone `GetHashKey('adder')` call
cannot become a bare hash statement.

Editing commands can process several files before encountering an error; they are not a
transaction over the entire input. Review the resulting diff.
