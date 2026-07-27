import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'
import { chmod, cp, lstat, mkdir, mkdtemp, readFile, rm, stat, symlink, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { fileURLToPath } from 'node:url'

import binding from '../../npm/oxlint-fix-unused-vars/index.js'

const temporaryDirectories = []

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, { recursive: true, force: true })))
})

async function fixture(source, extension = 'ts') {
  const root = await mkdtemp(join(tmpdir(), 'oxlint-fix-unused-vars-'))
  temporaryDirectories.push(root)
  const path = join(root, `source.${extension}`)
  await writeFile(path, source)
  return { root, path }
}

async function dryRun(source, options = {}) {
  const { root, path } = await fixture(source)
  const result = binding.removeUnusedVars({
    root,
    path: [path],
    ...options,
  })
  assert.deepEqual(result.errors, [])
  return result.results[0]
}

test('removes unused declarations and preserves used declarations', async () => {
  const result = await dryRun(
    'const used = 1\nconst unused = 2\nfunction called() { return used }\ncalled()\n',
  )
  assert.equal(result.updated, 'const used = 1\nfunction called() { return used }\ncalled()\n')
})

test('handles multi-declarators, trailing parameters, and catches', async () => {
  const result = await dryRun(
    "const used = 1, unused = 2\nfunction f(value, extra) { return value }\ntry {} catch (error) { console.log(used) }\nf(used)\n",
  )
  assert.equal(
    result.updated,
    "const used = 1\nfunction f(value) { return value }\ntry {} catch { console.log(used) }\nf(used)\n",
  )
})

test('leaves imports alone', async () => {
  const source = "import unused from './dependency.js'\nconsole.log('ready')\n"
  const result = await dryRun(source)
  assert.equal(result, undefined)
})

test('removes unused types, interfaces, enums, and classes', async () => {
  const result = await dryRun(`type UnusedType = string
interface UnusedInterface { unusedField: string }
enum UnusedEnum { UnusedValue = 'unused' }
class UnusedClass {}
console.log('test')
`)
  assert.equal(result.updated, "console.log('test')\n")
})

test('honors ignore patterns and reports an empty expansion', async () => {
  const { root } = await fixture('const unused = 1\n')
  const result = binding.removeUnusedVars({
    root,
    path: ['**/*.{js,ts}'],
    ignorePatterns: ['**/*.ts'],
  })
  assert.equal(result.results.length, 0)
  assert.match(result.errors[0].message, /root.*path.*ignorePatterns/)
})

test('includes removal metadata only when requested', async () => {
  const withoutRemovals = await dryRun('const unused = 1\n')
  assert.equal(withoutRemovals.removals, undefined)

  const withRemovals = await dryRun('const unused = 1\n', { includeRemovals: true })
  assert.equal(withRemovals.removals.length, 1)
  assert.equal(withRemovals.removals[0].name, 'unused')

  const grouped = await dryRun("const first = 1, second = 2\nconsole.log('x')\n", {
    includeRemovals: true,
  })
  assert.equal(grouped.removals.length, 2)
})

test('throws for invalid top-level options', () => {
  assert.throws(
    () => binding.removeUnusedVars({ root: 'relative', path: [], threads: 0 }),
    /root must be absolute|threads must be at least 1/,
  )
})

test('writes changed files atomically', async () => {
  const sourceFixture = fileURLToPath(new URL('./fixtures/write', import.meta.url))
  const root = await mkdtemp(join(tmpdir(), 'oxlint-fix-unused-vars-write-'))
  temporaryDirectories.push(root)
  await cp(sourceFixture, root, { recursive: true })

  const result = binding.removeUnusedVars({
    root,
    path: ['**/*.ts'],
    write: true,
    threads: 1,
  })

  assert.deepEqual(result.errors, [])
  assert.equal(result.results[0].updated, undefined)
  assert.equal(result.results[0].pass, undefined)
  assert.equal(await readFile(join(root, 'source.ts'), 'utf8'), 'const kept = 1\nconsole.log(kept)\n')
})

test('write object with passes repeats while files change', async () => {
  const { root, path } = await fixture(
    "type A = string\ntype B = A\nconsole.log('ready')\n",
  )

  const result = binding.removeUnusedVars({
    root,
    path: [path],
    write: { enabled: true, passes: 5 },
    includeRemovals: true,
    threads: 1,
  })

  assert.deepEqual(result.errors, [])
  assert.equal(result.results.length, 2)
  assert.equal(result.results[0].pass, 1)
  assert.equal(result.results[0].removals[0].name, 'B')
  assert.equal(result.results[1].pass, 2)
  assert.equal(result.results[1].removals[0].name, 'A')
  assert.equal(await readFile(path, 'utf8'), "console.log('ready')\n")
})

test('write object with passes stops at the pass limit', async () => {
  const { root, path } = await fixture(
    "type A = string\ntype B = A\nconsole.log('ready')\n",
  )

  const result = binding.removeUnusedVars({
    root,
    path: [path],
    write: { enabled: true, passes: 1 },
    includeRemovals: true,
    threads: 1,
  })

  assert.deepEqual(result.errors, [])
  assert.equal(result.results.length, 1)
  assert.equal(result.results[0].pass, 1)
  assert.equal(result.results[0].removals[0].name, 'B')
  assert.equal(await readFile(path, 'utf8'), "type A = string\nconsole.log('ready')\n")
})

test('write object without passes does not tag results', async () => {
  const { root, path } = await fixture('const unused = 1\n')
  const result = binding.removeUnusedVars({
    root,
    path: [path],
    write: { enabled: true },
  })
  assert.deepEqual(result.errors, [])
  assert.equal(result.results.length, 1)
  assert.equal(result.results[0].pass, undefined)
})

test('skips TypeScript definition variants', async () => {
  const { root, path } = await fixture('declare const value: string\n', 'd.mts')
  const result = binding.removeUnusedVars({ root, path: [path] })
  assert.deepEqual(result, { results: [], errors: [] })
})

test('does not panic for paths outside root with ignore patterns', async () => {
  const { path } = await fixture('const unused = 1\n')
  const root = await mkdtemp(join(tmpdir(), 'oxlint-fix-unused-vars-root-'))
  temporaryDirectories.push(root)
  const result = binding.removeUnusedVars({
    root,
    path: [path],
    ignorePatterns: ['*.generated.ts'],
  })
  assert.equal(result.results.length, 0)
  assert.match(result.errors[0].message, /no files found/)
})

test('object config does not implicitly ignore underscore names', async () => {
  const result = await dryRun("const _unused = 1\nconsole.log('x')\n", {
    noUnusedVarsConfig: {},
  })
  assert.equal(result.updated, "console.log('x')\n")
})

test('single-segment globs do not match nested paths', async () => {
  const { root, path } = await fixture("const unused = 1\nconsole.log('root')\n")
  const nestedDirectory = join(root, 'nested')
  await mkdir(nestedDirectory)
  await writeFile(join(nestedDirectory, 'nested.ts'), "const unused = 1\nconsole.log('nested')\n")

  const result = binding.removeUnusedVars({ root, path: ['*.ts'] })
  assert.deepEqual(result.errors, [])
  assert.deepEqual(result.results.map((file) => file.path), [path])
})

test('writes through symlinks and preserves target permissions', async () => {
  const root = await mkdtemp(join(tmpdir(), 'oxlint-fix-unused-vars-symlink-'))
  temporaryDirectories.push(root)
  const target = join(root, 'target.ts')
  const link = join(root, 'source.ts')
  await writeFile(target, "const unused = 1\nconsole.log('x')\n")
  await chmod(target, 0o640)
  await symlink(target, link)

  const result = binding.removeUnusedVars({ root, path: [link], write: true })
  assert.deepEqual(result.errors, [])
  assert.equal((await lstat(link)).isSymbolicLink(), true)
  assert.equal(await readFile(target, 'utf8'), "console.log('x')\n")
  assert.equal((await stat(target)).mode & 0o777, 0o640)
})
