export type SortKey = 'name' | 'account' | 'reset' | 'status'
export interface ListSort { key: SortKey, direction: 'asc' | 'desc' }
export interface ListItem {
  id: string
  name: string
  account: string
  accountSearch: string
  reset: number | null
  status: '正常' | '异常' | '禁用'
}

const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' })
const statusOrder = { 正常: 0, 异常: 1, 禁用: 2 }

export function filterAndSort<T extends ListItem>(items: T[], search: string, sort?: ListSort): T[] {
  const query = search.trim().toLocaleLowerCase()
  const result = items.filter(item => !query || [item.name, item.account, item.accountSearch].some(value => value.toLocaleLowerCase().includes(query)))
  if (!sort)
    return result
  return result.sort((a, b) => {
    let compared: number
    if (sort.key === 'reset') {
      // 未显示的重置时间不参与时间先后比较，升降序均放到末尾。
      if (a.reset == null || b.reset == null)
        return a.reset == null && b.reset == null ? collator.compare(a.id, b.id) : a.reset == null ? 1 : -1
      compared = a.reset - b.reset
    }
    else if (sort.key === 'status') {
      compared = statusOrder[a.status] - statusOrder[b.status]
    }
    else {
      compared = collator.compare(a[sort.key], b[sort.key])
    }
    return compared ? compared * (sort.direction === 'asc' ? 1 : -1) : collator.compare(a.id, b.id)
  })
}

export function paginate<T>(items: T[], requestedPage: number, pageSize: number) {
  const currentPage = Math.max(1, Math.min(requestedPage, Math.max(1, Math.ceil(items.length / pageSize))))
  return { currentPage, rows: items.slice((currentPage - 1) * pageSize, currentPage * pageSize) }
}
