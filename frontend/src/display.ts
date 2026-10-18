export function displayDate(value: number | null | undefined, full = false): string {
  if (value == null)
    return '暂无时间'
  // 与主项目一致，直接使用浏览器本地时间，不另外声明时区。
  const formatted = new Intl.DateTimeFormat('zh-CN', {
    ...(full ? { year: 'numeric' } as const : {}),
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).format(value)
  return formatted
}

// 订阅只展示宿主账号目录提供的类型；缺失时不能从使用率推断套餐。
export function subscriptionLabel(planType?: string | null, authenticationKind?: string | null): string {
  if (authenticationKind?.toLowerCase() === 'api_key')
    return 'API'
  const plan = planType?.trim()
  if (!plan)
    return '—'
  switch (plan.toLowerCase()) {
    case 'free': return 'Free'
    case 'plus': return 'Plus'
    case 'pro': return 'Pro'
    case 'prolite': return 'Pro Lite'
    default: return plan
  }
}

export function money(value: string | undefined): string {
  if (value == null || !Number.isFinite(Number(value)))
    return '暂无数据'
  return `$${Number(value).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}

export function percent(value: number | null | undefined): string {
  return value == null ? '暂无数据' : `${new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 1 }).format(value)}%`
}
