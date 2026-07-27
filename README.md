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

### Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `root` | `string` | (required) | **Must be an absolute path.** Project root that relative `path` entries resolve against. |
| `path` | `string[]` | (required) | Absolute paths, or files, directories, and globs relative to `root`. |
| `ignorePatterns` | `string[]` | `[]` | Oxlint-style gitignore syntax. `.gitignore` is not loaded. |
| `noUnusedVarsConfig` | `'all' \| 'local' \| NoUnusedVarsOptions` | Oxlint defaults | Rule options: `vars`, `args`, `caughtErrors`, ignore patterns, etc. |
| `write` | `boolean \| { enabled: true, passes?: number }` | `false` | `false` = dry run; results carry `updated` only for changed files. |
| `includeRemovals` | `boolean` | `false` | Include a `removals` array per file result. |
| `threads` | `number` | available parallelism | Worker count. Minimum `1`; clamped to 4× available parallelism, max 256. |

- **`passes`**: when set, the tool re-runs while any files are written, up to that many times. `results` stays a flat array with a 1-based `pass` on each entry.
- **Errors**: parse and file errors are returned in `errors`; invalid top-level options throw.

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
