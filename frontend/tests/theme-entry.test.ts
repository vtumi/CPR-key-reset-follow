import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import config from '../vite.config.ts'

function themePrefix(html: string) {
  return html.match(/<meta\s+name="codex-proxy-ui-theme-prefix"\s+content="([^"]+)"/i)?.[1]
}

test('development and packaged entries select the UI theme namespace', () => {
  const development = readFileSync(new URL('../index.html', import.meta.url), 'utf8')
  const emitted: { type: string, fileName: string, source: string }[] = []
  const plugin = config.plugins?.flat().find(plugin => plugin && 'name' in plugin && plugin.name === 'plugin-classic-entry')
  assert.ok(plugin && 'generateBundle' in plugin && typeof plugin.generateBundle === 'function')
  // 验证真实打包钩子的产物，避免只修开发入口而遗漏发行包入口。
  plugin.generateBundle.call({ emitFile: (asset: typeof emitted[number]) => emitted.push(asset) } as never, {} as never, {}, false)
  const packaged = emitted.find(asset => asset.fileName === 'index.html')?.source
  assert.ok(packaged)
  assert.equal(themePrefix(development), 'cp')
  assert.equal(themePrefix(packaged), 'cp')
})
