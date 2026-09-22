# Changelog

## 1.0.0

### Added

- Lua and CfxLua parsing, scope analysis, and resource-aware linting.
- Bundled native definitions, runtime stubs, and known manifest imports.
- Client/server checks, event and export analysis, locale checks, and security-pattern rules.
- TOML configuration, per-file overrides, inline suppressions, and selected automatic fixes.
- Formatting with token and comment verification, plus a check mode for CI.
- Text, JSON, JUnit, GitHub annotation, and SARIF reports; a composite GitHub Action.

### Fixed

- Formatting and automatic fixes reject non-UTF-8 source without replacing its bytes.
- Hash fixes avoid standalone call statements and other positions that require a prefix
  expression. Automatic fixes are parsed before writing.
- Excessive expression-chain depth produces a diagnostic instead of overflowing the stack.
- Relative configuration paths apply the same exclusions and overrides as absolute paths.
- The Intel macOS release build uses a supported runner label.
