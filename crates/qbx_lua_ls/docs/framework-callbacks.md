# Framework callback conventions

These adapters maintain four call shapes. Callback names, payload parameter names
and types come from the user's indexed Lua handlers, not copied framework API
documentation. No additional dependency, network request or catalog is required.

| Framework | Server registration | Client trigger | Server handler parameters |
|---|---|---|---|
| QB-Core | `core.Functions.CreateCallback(name, handler)` | `core.Functions.TriggerCallback(name, cb, ...payload)` | `source, cb, ...payload` |
| ESX | `esx.RegisterServerCallback(name, handler)` | `esx.TriggerServerCallback(name, cb, ...payload)` | `source, cb, ...payload` |

Conventions reviewed on 2026-09-26 against primary sources:

- QB-Core [client function reference](https://qbcore.org/docs/qb-core/client-function-reference#qbcorefunctionstriggercallback)
  and [server function reference](https://qbcore.org/docs/qb-core/server-function-reference#qbcorefunctionscreatecallback).
  Source revision: `9b3cddcce93e5e12cbcf6b47b866b687d32ac7bf`,
  [client functions](https://github.com/qbcore-fivem/qb-core/blob/9b3cddcce93e5e12cbcf6b47b866b687d32ac7bf/client/functions.lua).
- ESX source revision: `fe59ca0bd6da59e2ec6eb4a8d06ece312e96ae7a`,
  [server callbacks](https://github.com/esx-framework/esx_core/blob/fe59ca0bd6da59e2ec6eb4a8d06ece312e96ae7a/%5Bcore%5D/es_extended/server/modules/callback.lua)
  and [client callbacks](https://github.com/esx-framework/esx_core/blob/fe59ca0bd6da59e2ec6eb4a8d06ece312e96ae7a/%5Bcore%5D/es_extended/client/modules/callback.lua).

## Recognition and limits

`src/framework_callbacks.rs` recognizes literal standard export initializations
(`exports['qb-core']:GetCoreObject()` and
`exports['es_extended']:getSharedObject()`), local aliases, the manifest import
`@es_extended/imports.lua`, and canonical globals in indexed provider resources.
Reassigned receivers, shadowed `exports`/`_ENV` and detected callback member
overwrites disable the adapter. This is conservative static recognition, not a
complete analysis of arbitrary runtime table mutations or wrapper functions.

Manifest side must identify server registrations and client triggers. Shared
files need a recognized `IsDuplicityVersion()` guard. Names must be nonempty
literal strings. Handler parameter inference supports inline functions and
unreassigned local function references, with LuaCATS annotations where present.

Each framework has its own event-index family. The first two handler parameters
are removed from client payload hints; handler return values never describe the
asynchronous response. Conflicting payload definitions disable derived signature
and inlay hints and are identified in completion details. Navigation and hovers
can list multiple registrations.

Client-callback/Await variants, filtered `GetCoreObject` calls, computed callback
names and response-type inference for these frameworks require a separate scope
decision. Existing native events and ox_lib callbacks keep their own conventions.
A resource's own wrappers are declared with `---@callback` instead; see below.

## Resource-defined wrappers

A function tagged `---@callback register|await|trigger [family]` joins a callback
system of its own. Nothing about it is hardcoded: the parts come from the
wrapper's annotations.

| Part | Taken from |
|---|---|
| Name | The first parameter typed `string`, or else the first that takes no function. |
| Handler (`register`) or response function (`trigger`) | The first parameter typed `fun(...)` or `function`. |
| Payload (`await`, `trigger`) | The `...` parameter. |

The tag travels with the function's type, so local aliases, table members and
`:` methods of a wrapper are recognized too. Registrations are indexed like
`lib.callback.register`: a doc comment directly above the registering statement
types an inline handler and its parameters, and handlers registered outside the
client drop their first parameter, `source`, from the payload. A call made on one
side reaches handlers registered on the other side, as `lib.callback` does.

`await` calls return the handler's `@return` types. `trigger` calls pass them to
the response function's parameters. Wrappers without a family name share one set
of names; a family name keeps separate systems apart. A registration is
recognized once its wrapper has been indexed; the startup scan's second pass
settles this for files on disk.

## Maintaining the adapters

When upstream changes one of these four conventions, review the primary source,
update this revision note, then adjust the classifier/indexer and event-call
mapping. Add an isolated fixture under `tests/fixtures/framework_callbacks` and an
LSP regression in `tests/lsp.rs`; preserve family isolation and shadowing checks.
Run the repository's Rust checks and the editor integration suite. Changes to a
framework's unrelated API documentation require no adapter update.
