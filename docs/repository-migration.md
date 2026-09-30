# Combined Lua tooling workspace

`Qbox-project/qbx-lint` contains the linter, language server, and their shared Rust crates.
The editor adapters remain in `Qbox-project/qbx-editor`. A change to shared analysis and LSP
behavior now belongs in one pull request, checked by the workspace CI on Windows and Linux.

## Preserved history

The language server was imported into `crates/qbx_lua_ls` with a full-history subtree merge,
without squashing or rewriting its commits. The import joins the two existing commit graphs:

- Linter source tip: `6cdc955`.
- Language-server source tip: `3957972`.

All commits reachable from those tips retain their original IDs, authors, committers,
timestamps and messages. Workspace integration is recorded in subsequent commits. Historical
server commits still describe paths at their original repository root; use `git log --follow -m`
for the history of an individual moved file.

The migration pull request must be merged with **Create a merge commit**. Squashing or rebasing
the migration would not retain the imported commit graph on the default branch.

The original repositories both used `v1.0.x` tags. Keep the linter tags unchanged and preserve
the server's historical tag objects under `lua-ls/v1.0.x`, avoiding collisions:

```sh
git fetch --no-tags https://github.com/Qbox-project/qbx-lua-ls.git \
  'refs/tags/*:refs/tags/lua-ls/*'
git push origin 'refs/tags/lua-ls/*:refs/tags/lua-ls/*'
```

These names do not trigger the release workflow's `v*` tag filter. Old GitHub release pages
and any closed issues or pull requests remain available in the original server repository.

## Build and release compatibility

One root `Cargo.toml` controls versions and dependencies, with one `Cargo.lock` and `target/`
directory. The executables remain `qbx-lint` and `qbx-lua-ls`.

The release workflow builds each tool separately for the existing five platforms, using the
same archive names as before. CLI builds use `lint-release` with abort-on-panic and without
the server's documentation features. Server builds use `release` with panic unwinding,
preserving LSP request recovery. A new `vX.Y.Z` release contains both sets of archives.

The root `action.yml` and `Qbox-project/qbx-lint@<tag>` action address remain available.
Existing linter tags and their release assets are unchanged. Existing server releases remain
downloadable from `Qbox-project/qbx-lua-ls`; future releases are published here.

## Cutover order

1. Merge the workspace migration with a merge commit and publish the namespaced historical
   server tags. Do not recreate or move existing `v1.0.x` tags.
2. Bump the workspace to the next unused version and publish a new shared release, such as
   `v1.0.5`. Check that it contains both tools for all five platforms and their checksums.
3. Merge the companion `qbx-editor` migration. Its CI and development scripts use this
   workspace, its release builds use the matching workspace tag, and Zed downloads server
   archives from this repository. Publish the editor using the new shared server version.
4. Add a migration notice to `Qbox-project/qbx-lua-ls` and archive it after the editor cutover.
   Keep its historical releases available for existing editor versions and download links.

Do not publish an editor release using an old linter tag: those tags predate the server import.
