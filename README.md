# oxlint-fix-unused-vars

> Fills the gap of Oxlint's `no-unused-vars` fix to remove all reported unused variables.

Remove unused TypeScript and JavaScript bindings with Oxlint's `no-unused-vars` analysis. Imports are reported by the detector but intentionally never removed. Partial destructuring edits are skipped so getters, defaults, rest elements, and iterators are not disturbed.

## Features

- Oxlint-compatible `no-unused-vars` detection
- Honors `NoUnusedVarsConfig` options (`vars`, `args`, ignore patterns, etc.)
- Conservative edits: skips imports, partial destructuring, and removals that could cause side effects
- High performance, written in Rust using Oxc

## API

```js
import { removeUnusedVars } from 'oxlint-fix-unused-vars'

const result = removeUnusedVars({
  root: '/absolute/path/to/project',
  path: ['src/**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}'],
  ignorePatterns: ['**/*.generated.ts'],
  noUnusedVarsConfig: {
    vars: 'all',
    args: 'after-used',
    caughtErrors: 'all',
  },
  write: false,
  // or: write: { enabled: true, passes: 3 },
  includeRemovals: true,
  threads: 4,
})
```

`root` must be absolute. `path` accepts absolute paths or root-relative files, directories, and globs. `ignorePatterns` uses Oxlint-style gitignore syntax without loading `.gitignore`. `noUnusedVarsConfig` accepts `"all"`, `"local"`, or an options object. `write` defaults to `false` and accepts `true`/`false` or `{ enabled: true, passes?: number }`; dry runs include `updated` only for changed files. When `passes` is set, the tool re-runs while any files are written, up to that many times, and returns a flat `results` array with a 1-based `pass` on each entry. `includeRemovals` defaults to `false`. `threads` defaults to the available parallelism, must be at least one, and is clamped to four times the available parallelism (up to 256).

Parse and file errors are returned in `errors`; invalid top-level options throw.

## Local development

Build and test in this repo, then link the package into another project.

```sh
pnpm install
pnpm build:debug
pnpm test
cd npm/oxlint-fix-unused-vars && pnpm link --global
# in the consumer project:
pnpm link --global oxlint-fix-unused-vars
```

Rebuild after Rust changes before retesting the linked consumer.
