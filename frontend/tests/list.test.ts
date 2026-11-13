import type { ListItem, SortKey } from '../src/list.ts'
import assert from 'node:assert/strict'
import { it } from 'node:test'
import { filterAndSort, paginate } from '../src/list.ts'

const items: ListItem[] = [
  { id: '3', name: 'Key 10', account: 'z@example.test', accountSearch: 'Team Gamma z@example.test acct-3', reset: null, status: '禁用' },
  { id: '1', name: 'Key 2', account: 'b@example.test', accountSearch: 'Team Beta b@example.test acct-1', reset: 300, status: '正常' },
  { id: '2', name: 'Key 1', account: 'a@example.test', accountSearch: 'Team Alpha a@example.test acct-2', reset: 100, status: '异常' },
]
const ids = (rows: ListItem[]) => rows.map(row => row.id)

it('搜索名称及账号名称、邮箱和 ID，忽略大小写及首尾空白', () => {
  for (const query of [' key 10 ', 'GAMMA', 'Z@EXAMPLE', 'acct-3'])
    assert.deepEqual(ids(filterAndSort(items, query)), ['3'])
  assert.deepEqual(filterAndSort(items, 'not-found'), [])
  assert.equal(filterAndSort([{ ...items[0]!, account: '账号已删除', accountSearch: 'acct-3' }], '账号已删除').length, 1)
  assert.deepEqual(ids(filterAndSort(items, '  ')), ['1', '2', '3'])
})

it('四种字段均支持双向排序，不改变原快照顺序', () => {
  const expectations: Record<SortKey, [string[], string[]]> = {
    name: [['2', '1', '3'], ['3', '1', '2']],
    account: [['2', '1', '3'], ['3', '1', '2']],
    reset: [['2', '1', '3'], ['1', '2', '3']],
    status: [['1', '2', '3'], ['3', '2', '1']],
  }
  for (const key of Object.keys(expectations) as SortKey[]) {
    assert.deepEqual(ids(filterAndSort(items, '', { key, direction: 'asc' })), expectations[key][0])
    assert.deepEqual(ids(filterAndSort(items, '', { key, direction: 'desc' })), expectations[key][1])
  }
  assert.deepEqual(ids(items), ['3', '1', '2'])
})

it('相同值按稳定身份排序，缺失重置时间在两个方向均置后', () => {
  const same = items.map(item => ({ ...item, name: 'same', reset: null }))
  for (const direction of ['asc', 'desc'] as const) {
    assert.deepEqual(ids(filterAndSort(same, '', { key: 'name', direction })), ['1', '2', '3'])
    assert.deepEqual(ids(filterAndSort(same, '', { key: 'reset', direction })), ['1', '2', '3'])
  }
})

it('先筛选排序再分页，空结果及删除末页后回到有效页', () => {
  const sorted = filterAndSort(items, 'key', { key: 'name', direction: 'asc' })
  assert.deepEqual(ids(paginate(sorted, 1, 2).rows), ['2', '1'])
  assert.deepEqual(ids(paginate(sorted, 2, 2).rows), ['3'])
  assert.deepEqual(paginate(sorted.slice(0, 2), 2, 2), { currentPage: 1, rows: sorted.slice(0, 2) })
  assert.deepEqual(paginate([], 8, 20), { currentPage: 1, rows: [] })
  assert.equal(paginate(sorted, 0, 20).currentPage, 1)
})

it('默认按关联创建时间倒序，旧记录未知时间稳定置后', () => {
  const rows = [{ ...items[0]!, created: 100 }, { ...items[1]!, created: null }, { ...items[2]!, created: 300 }]
  assert.deepEqual(ids(filterAndSort(rows, '')), ['2', '3', '1'])
  assert.deepEqual(ids(filterAndSort(rows, '', { key: 'name', direction: 'asc' })), ['2', '1', '3'])
})
