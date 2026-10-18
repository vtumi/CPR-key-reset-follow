import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const page = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')

test('management labels and refresh controls stay consistent', () => {
  assert.match(page, /选择密钥和账号，开始跟随重置/)
  assert.match(page, /placeholder="选择密钥"/)
  assert.match(page, /placeholder="选择账号"/)
  assert.match(page, /<th>本周额度<\/th>/)
  assert.match(page, /<th>重置时间<\/th>/)
  assert.doesNotMatch(page, /账号本周用量|账号重置时间|TimerReset/)
  assert.match(page, /<th>提前重置<\/th>/)
  assert.match(page, /<th>操作<\/th>/)
  assert.match(page, /<BaseSwitch[\s\S]*:model-value="earlyResetEnabled\(l\)"/)
  assert.match(page, /@update:model-value="enabled => setEarlyAuto\(l, enabled\)"/)
  assert.doesNotMatch(page, /aria-pressed="data\?\.state\.early_auto/)
  assert.match(page, /label="刷新列表"/)
  assert.doesNotMatch(page, /label="立即刷新"/)
  assert.match(page, /displayResetDate\(resetAt\(l\)\)/)
  assert.match(page, /<span v-if="resetAt\(l\) != null">/)
})
