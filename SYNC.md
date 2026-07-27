# Vendor synchronization

The detector in `src/vendor/no_unused_vars` is based on Oxc commit `14533a3dc118bea73e755426aaf35f71dbe81eb8`, crates release `0.141.0`.

## Files

Sync these files from `crates/oxc_linter/src/rules/eslint/no_unused_vars/`:

- `allowed.rs`
- `binding_pattern.rs`
- `ignored.rs`
- `mod.rs`
- `options.rs`
- `symbol.rs`
- `usage.rs`

## Procedure

1. Check out the target Oxc revision.
2. Copy the files above into `src/vendor/no_unused_vars/`.
3. Preserve the synchronization header in each copied file.
4. Reapply the standalone adaptations:
   - expose `find_unused_bindings`, option types, `UnusedBinding`, and `UnusedKind`;
   - keep `symbol_id: SymbolId` on `UnusedBinding`;
   - replace linter context and diagnostics with the semantic/module-record detector inputs;
   - retain only imports available from the published Oxc crates;
   - preserve the focused detector unit test in `mod.rs`.
5. Review upstream fixer changes under `fixers/`, especially comma and parameter handling, and port applicable behavior to `src/remove.rs` manually.
6. Update the SHA and crate versions in this file, vendor headers, and `Cargo.toml`.

## Verify

```sh
cargo fmt --check
cargo test
pnpm install
pnpm build:debug
node --test napi/test/remove-unused-vars.spec.mjs
```
