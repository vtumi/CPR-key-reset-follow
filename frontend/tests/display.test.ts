import assert from 'node:assert/strict'
import process from 'node:process'
import { it } from 'node:test'
import { displayDate, displayResetDate, expectedKeyReset, money, percent, subscriptionLabel } from '../src/display.ts'

it('日期跟随浏览器时区，跨日与夏令时自动适配', () => {
  const value = Date.parse('2026-10-03T02:10:00Z')
  const original = process.env.TZ
  try {
    process.env.TZ = 'Asia/Shanghai'
    assert.equal(displayDate(value), '10/03 10:10')
    assert.equal(displayDate(value, true), '2026/10/03 10:10')
    process.env.TZ = 'America/Los_Angeles'
    assert.equal(displayDate(value), '10/02 19:10')
    assert.equal(displayDate(Date.parse('2026-12-01T00:00:00Z'), true), '2026/11/30 16:00')
    assert.equal(displayDate(null), '暂无时间')
  }
  finally {
    if (original === undefined)
      delete process.env.TZ
    else
      process.env.TZ = original
  }
})

it('重置时间包含本地年月日时分秒', () => {
  const previous = process.env.TZ
  const timestamp = Date.parse('2026-10-18T13:48:21Z')
  try {
    process.env.TZ = 'Asia/Shanghai'
    assert.equal(displayResetDate(timestamp), '2026-10-18 21:48:21')
    process.env.TZ = 'America/Los_Angeles'
    assert.equal(displayResetDate(timestamp), '2026-10-18 06:48:21')
    assert.equal(displayResetDate(null), '-')
  }
  finally {
    if (previous === undefined)
      delete process.env.TZ
    else
      process.env.TZ = previous
  }
})

it('金额和账号用量区分空值与零，并控制展示精度', () => {
  assert.equal(money('66.08130564'), '$66.08')
  assert.equal(money('239.8797087727'), '$239.88')
  assert.equal(money('0'), '$0.00')
  assert.equal(money(undefined), '暂无数据')
  assert.equal(percent(null), '暂无数据')
  assert.equal(percent(0), '0%')
  assert.equal(percent(45.123), '45.1%')
})

it('订阅与平台类型来自真实账号字段，不推测缺失数据', () => {
  assert.equal(subscriptionLabel('plus', 'oauth'), 'Plus')
  assert.equal(subscriptionLabel('pro', 'oauth'), 'Pro')
  assert.equal(subscriptionLabel('custom-tier', 'oauth'), 'custom-tier')
  assert.equal(subscriptionLabel(null, 'oauth'), '—')
  assert.equal(subscriptionLabel(null, 'api_key'), 'API')
})

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
