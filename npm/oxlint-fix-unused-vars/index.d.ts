export type VarsOption = 'all' | 'local'
export type ArgsOption = 'after-used' | 'all' | 'none'

export interface NoUnusedVarsOptions {
  vars?: VarsOption
  varsIgnorePattern?: string
  args?: ArgsOption
  argsIgnorePattern?: string
  ignoreRestSiblings?: boolean
  caughtErrors?: 'all' | 'none'
  caughtErrorsIgnorePattern?: string
  destructuredArrayIgnorePattern?: string
  ignoreClassWithStaticInitBlock?: boolean
  ignoreUsingDeclarations?: boolean
  reportVarsOnlyUsedAsTypes?: boolean
  /** Accepted for Oxlint compatibility and ignored. */
  fix?: unknown
}

export interface WriteOptions {
  enabled: true
  /** When set, re-run while files are written, up to this many passes. */
  passes?: number
}

export interface RemoveUnusedVarsOptions {
  /** Absolute project root. */
  root: string
  /** Absolute paths or paths and globs relative to `root`. */
  path: string[]
  ignorePatterns?: string[]
  noUnusedVarsConfig?: VarsOption | NoUnusedVarsOptions
  write?: boolean | WriteOptions
  includeRemovals?: boolean
  /** Worker count. Must be at least one; excessive values are clamped. */
  threads?: number
}

export interface Removal {
  name: string
  start: number
  end: number
  kind: string
}

export interface FileResult {
  /** Absolute file path. */
  path: string
  /** Full updated source when `write` is false and the file changed. */
  updated?: string
  /** Included only when `includeRemovals` is true. */
  removals?: Removal[]
  /** 1-based pass index when `write: { enabled: true, passes }` is used. */
  pass?: number
}

export interface RunError {
  /** Absolute file path when the error belongs to a file. */
  path?: string
  message: string
}

export interface RemoveUnusedVarsResult {
  results: FileResult[]
  errors: RunError[]
}

export function removeUnusedVars(options: RemoveUnusedVarsOptions): RemoveUnusedVarsResult
