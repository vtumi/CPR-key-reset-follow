export function displayDate(value: number | null | undefined, full = false): string {
  if (value == null)
    return '暂无时间'
  // 与主项目一致，使用当前浏览器的系统时区，不维护插件私有时区配置。
  const formatted = new Intl.DateTimeFormat('zh-CN', {
    ...(full ? { year: 'numeric' } as const : {}),
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).format(value)
  return full ? `${formatted}（${timezoneOffset(value)}）` : formatted
}

function timezoneOffset(value: number): string {
  return new Intl.DateTimeFormat('en', { timeZoneName: 'shortOffset' })
    .formatToParts(value)
    .find(part => part.type === 'timeZoneName')!
    .value
    .replace('GMT', 'UTC')
}

export function money(value: string | undefined): string {
  if (value == null || !Number.isFinite(Number(value)))
    return '暂无数据'
  return `$${Number(value).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}

export function percent(value: number | null | undefined): string {
  return value == null ? '暂无数据' : `${new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 1 }).format(value)}%`
}
