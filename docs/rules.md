# Rules

Use `qbx-lint --list-rules` to inspect the rules in your installed version. Levels below are the
defaults; `hint` findings require `--min-severity hint` to appear in CLI output. Configuration can
set any rule to `off`, `hint`, `info`, `warning`, or `error`.

## Lua

| Rule | Default | Check |
| --- | --- | --- |
| `syntax-error` | error | Source rejected by the parser, including its nesting limit. |
| `undefined-global` | warning | A read of a global absent from the known environment. |
| `undefined-field` | warning | An unknown field on a standard library table. |
| `unused-local` | warning | A local variable is never read. |
| `unused-function` | warning | A local function is never used. |
| `unused-argument` | hint | A function parameter is never read. |
| `unused-loop-variable` | hint | A loop variable is never read. |
| `unused-label` | warning | A label is never targeted by a `goto`. |
| `undefined-label` | error | A `goto` targets a label outside its visible scope. |
| `redefined-local` | warning | A local is declared twice in one scope. |
| `shadowed-local` | off | A local hides one in an enclosing scope. |
| `unreachable-code` | warning | Code follows a control-flow statement that exits the path. |
| `empty-block` | info | A block has no statements. |
| `unbalanced-assignments` | warning | An assignment has mismatched target and value counts. |
| `duplicate-index` | warning | A table constructor assigns the same key twice. |
| `duplicate-argument` | error | Function parameters share a name. |
| `const-reassign` | error | An assignment to a `<const>` or `<close>` local. |
| `self-assignment` | warning | A variable is assigned to itself. |
| `self-comparison` | warning | A comparison has the same expression on both sides. |
| `lowercase-global` | warning | A global definition starts with a lowercase letter. |
| `implicit-global` | warning | A function creates a global without a file-scope declaration. |
| `builtin-overwrite` | warning | Code overwrites a known runtime global or native. |
| `deprecated` | warning | A call uses a deprecated runtime function. |

## FiveM and Qbox

| Rule | Default | Check |
| --- | --- | --- |
| `fivem/native-wrong-side` | error | A native is used on the wrong client/server side. |
| `fivem/import-not-declared` | warning | A library global is used without its manifest import on that side. |
| `fivem/loop-never-yields` | error | An apparently infinite loop has no recognized yield or exit. |
| `fivem/source-after-yield` | warning | Global `source` is read after a yield or in a deferred callback. |
| `fivem/citizen-prefix` | info | A `Citizen` call has a shorter global alias. Fix available. |
| `fivem/hash-literal` | info | A literal `GetHashKey` argument can become a compile-time hash. Fix available in valid value positions. |
| `fivem/legacy-native-pattern` | info | Use of `GetPlayerPed(-1)`, `GetDistanceBetweenCoords`, `Vdist`, or `Vdist2`. A fix is available for `GetPlayerPed(-1)`. |
| `qbox/prefer-cache` | hint | A native lookup has a corresponding ox_lib cache value. |
| `qbox/legacy-core-object` | hint | Use of the QBCore compatibility core object. |
| `fivem/event-argument-count` | warning | More event arguments than known handlers accept. |
| `fivem/event-missing-arguments` | info | Fewer arguments than known handlers declare. |
| `fivem/event-wrong-side` | warning | Known handlers exist only on the other side. |
| `fivem/export-argument-count` | warning | More arguments than the known exported function accepts. |
| `fivem/unknown-export` | info | An analyzed resource does not register the named export. |
| `fivem/resource-not-found` | info | A call outside an `if` targets an export of a resource not found in the server. |
| `qbox/unknown-locale-key` | warning | A static key is absent from the resource's selected locale file. |
| `qbox/unused-locale-key` | info | A locale key has no known use in the resource. |

## Manifests

| Rule | Default | Check |
| --- | --- | --- |
| `manifest/missing-field` | warning | Missing `fx_version` or `game`. |
| `manifest/missing-file` | warning | A referenced file or glob has no match. |
| `manifest/unknown-directive` | warning | A directive resembles a misspelled known name. |
| `manifest/missing-dependency` | info | An export dependency lacks a declared or recognized start order. |
| `manifest/unlisted-script` | off | A Lua file is not referenced by the manifest. |
| `manifest/lua54` | off | Missing `lua54 'yes'`; an opt-in check for older server artifacts. Fix available. |

## Security patterns

| Rule | Default | Check |
| --- | --- | --- |
| `security/client-supplied-source` | warning | A server net event accepts a player identifier from its caller. |
| `security/unvalidated-event-argument` | warning | A client argument reaches a sensitive call without a recognized check. |
| `security/sql-concatenation` | warning | A query is assembled with concatenation or formatting without a recognized parameter argument. |

These checks do not prove validation or authorization. For example, recognizing a parameter in
a guard does not establish that the guard always runs before the sensitive operation. Review the
handler's control flow and runtime behavior separately.

See the [analysis reference](reference.md) for exclusions, suppressions, and limits on checks
across files and resources.
