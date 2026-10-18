import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const page = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')

test('management labels and refresh controls stay consistent', () => {
  assert.match(page, /选择密钥和账号，开始跟随重置/)
  assert.match(page, /placeholder="选择密钥"/)
  assert.match(page, /placeholder="选择账号"/)
  assert.match(page, /<th>本周额度<\/th>/)
  assert.match(page, /<th>账号本周用量<\/th>/)
  assert.match(page, /<th>账号重置时间<\/th>/)
  assert.match(page, /<BaseSwitch[^>]*label="提前重置"/)
  assert.doesNotMatch(page, /<BaseSwitch[^>]*show-label/)
  assert.match(page, /label="刷新列表"/)
  assert.match(page, /label="立即刷新"/)
})
