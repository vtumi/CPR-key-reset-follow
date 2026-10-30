import assert from 'node:assert/strict'
import { it } from 'node:test'
import { deleteSequentially, reconcileSelection, togglePageSelection } from '../src/bulk.ts'

const a = { key_id: 'key-a', account_id: 'account-a' }
const b = { key_id: 'key-b', account_id: 'account-b' }
const c = { key_id: 'key-c', account_id: 'account-c' }

it('全选仅切换当前页，保留其他页选择并避免重复', () => {
  assert.deepEqual(togglePageSelection([c, a], [a, b]), [c, a, b])
  assert.deepEqual(togglePageSelection([c, a, b], [a, b]), [c])
  assert.deepEqual(togglePageSelection([a], []), [a])
})

it('刷新后清除已删除和身份被替换的选择', () => {
  assert.deepEqual(reconcileSelection([a, b, c], { 'key-a': a, 'key-b': { ...b, account_id: 'replacement' } }), [a])
})

it('批量删除顺序执行，失败后停止且不重试未知结果', async () => {
  const calls: string[] = []
  const result = await deleteSequentially([a, b, c], async (target) => {
    calls.push(target.key_id)
    if (target.key_id === b.key_id)
      throw new Error('synthetic response lost')
  })
  assert.deepEqual(calls, ['key-a', 'key-b'])
  assert.deepEqual(result, { completed: [a], error: 'synthetic response lost' })
})

it('空批次不调用接口，成功批次逐项报告完成', async () => {
  let calls = 0
  const remove = async () => {
    calls += 1
  }
  assert.deepEqual(await deleteSequentially([], remove), { completed: [], error: null })
  assert.equal(calls, 0)
  assert.deepEqual(await deleteSequentially([a, b], remove), { completed: [a, b], error: null })
  assert.equal(calls, 2)
})
