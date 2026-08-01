# oxlint-fix-unused-vars

[![NPM Version](https://img.shields.io/npm/v/oxlint-fix-unused-vars)](https://www.npmjs.com/package/oxlint-fix-unused-vars)

> Automatically remove unused variables reported by `no-unused-vars` — the fixer oxlint doesn't ship.

Fixes unused TypeScript and JavaScript variables flagged by oxlint's `no-unused-vars` analysis, using the same rule options as ESLint's `no-unused-vars`. Conservative by design: imports are reported by the detector but intentionally never removed, and partial destructuring edits are skipped so getters, defaults, rest elements, and iterators are not disturbed.

## Features

- Oxlint-compatible `no-unused-vars` detection
- Same options as ESLint's `no-unused-vars` (via oxlint's implementation): `vars`, `args`, ignore patterns, etc.
- Conservative edits — see [What it changes](#what-it-changes) below
- High performance, written in Rust using Oxc

## What it changes

```ts
// before
const used = 1, unused = 2
function f(value, extra) { return value }
try {} catch (error) { console.log(used) }

// after
const used = 1
function f(value) { return value }
try {} catch { console.log(used) }
```

**Removed:** variables (`var`/`let`/`const`, including a single unused declarator out of several), trailing function parameters, catch bindings (dropped to an optional catch binding), functions, classes, TS types, interfaces, and enums.

**Never removed** (reported, left alone):

- Imports. The detector flags them but never edits them, oxlint rule already handles that.
- Bindings that are assigned to somewhere, even if never read
- Anything whose initializer, decorator, default, or computed key could run code
- Partial destructuring edits, which would disturb getters, defaults, rest elements, or iterators
- Declarations carrying comments in the span that would be removed
- Loop headers (`for (const x of xs)`)

## Install

```sh
npm install oxlint-fix-unused-vars
```

```sh
pnpm add oxlint-fix-unused-vars
```

```sh
yarn add oxlint-fix-unused-vars
```

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

`write: false` (default) changes nothing on disk; each changed file's full rewritten source comes back in `updated`. `write: { enabled: true, passes: n }` writes files in place, re-running while files keep changing since removing one variable can make another unused.

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
