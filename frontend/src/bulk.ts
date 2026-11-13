export interface DeleteTarget { key_id: string, account_id: string }

export function reconcileSelection(selected: DeleteTarget[], links: Record<string, DeleteTarget>): DeleteTarget[] {
  return selected.filter(target => links[target.key_id]?.account_id === target.account_id)
}

export function togglePageSelection(selected: DeleteTarget[], page: DeleteTarget[]): DeleteTarget[] {
  const all = page.length > 0 && page.every(row => selected.some(item => item.key_id === row.key_id && item.account_id === row.account_id))
  const next = selected.filter(item => !page.some(row => row.key_id === item.key_id))
  return all ? next : [...next, ...page.map(({ key_id, account_id }) => ({ key_id, account_id }))]
}

// 顺序调用现有删除接口；失败后停止，不自动重试可能已经生效的请求。
export async function deleteSequentially(targets: DeleteTarget[], remove: (target: DeleteTarget) => Promise<unknown>) {
  const completed: DeleteTarget[] = []
  for (const target of targets) {
    try {
      await remove(target)
      completed.push(target)
    }
    catch (error) {
      return { completed, error: error instanceof Error ? error.message : '删除失败' }
    }
  }
  return { completed, error: null }
}
