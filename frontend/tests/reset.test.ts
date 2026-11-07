import assert from 'node:assert/strict'
import { it } from 'node:test'
import { expectedKeyReset } from '../src/display.ts'

it('密钥重置时间基于关联基线，不直接使用账号预计时间', () => {
  const now = 1_000_000
  const link = {
    key_id: 'key-a',
    account_id: 'account-a',
    baseline: { observed: 100, reset: 2_000_000, used: null },
    paused: false,
    pending: null,
    events: [],
  }
  assert.equal(expectedKeyReset(link, now), 2_000_000)
  assert.equal(expectedKeyReset({ ...link, paused: true }, now), null)
  assert.equal(expectedKeyReset({ ...link, baseline: { ...link.baseline, reset: now } }, now), null)
})
