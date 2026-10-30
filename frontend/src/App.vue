<script setup lang="ts">
import type { Link, Snapshot } from './api/modules/follow'
import type { DeleteTarget } from './bulk'
import type { ListSort, SortKey } from './list'
import { BaseButton, BaseCheckbox, BaseIconButton, BaseInput, BaseModal, BaseSelect, BaseSwitch, BaseTablePagination, BaseTag } from '@codex-proxy/ui'
import { Plus, Power, PowerOff, Search, Trash2, Triangle } from '@lucide/vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { command, load } from './api/modules/follow'
import { deleteSequentially, reconcileSelection, togglePageSelection } from './bulk'
import AccountPlanBadge from './components/AccountPlanBadge.vue'
import ProviderIconGroup from './components/ProviderIconGroup.vue'
import { displayResetDate, expectedKeyReset, money, subscriptionLabel } from './display'
import { filterAndSort, paginate } from './list'

const data = ref<Snapshot>()
const search = ref('')
const sort = ref<ListSort>()
const page = ref(1)
const pageSize = ref(20)
const busy = ref(false)
const error = ref('')
const modal = ref('')
const selected = ref('')
const selection = ref<DeleteTarget[]>([])
const deleteTargets = ref<DeleteTarget[]>([])
const keyId = ref('')
const accountId = ref('')
const modalOpen = computed({ get: () => !!modal.value, set: (value) => {
  if (!value)
    modal.value = ''
} })
const rows = computed(() => Object.values(data.value?.state.links || {}))
const link = computed(() => data.value?.state.links[selected.value])
const account = (l: Link) => data.value?.accounts.find(a => a.account_id === l.account_id)
// 关联列表只显示邮箱；缺少邮箱时退回账号 ID，不展示“OpenAI 账号”等泛化名称。
const accountDisplay = (l: Link) => account(l)?.email?.trim() || account(l)?.account_id || '账号已删除'
const observation = (l: Link) => data.value?.state.accounts[l.account_id]
const budget = (l: Link) => data.value?.budgets[l.key_id]
const keyName = (l: Link) => data.value?.keys.find(k => k.id === l.key_id)?.name || '密钥已删除'
function status(l: Link): '正常' | '异常' | '禁用' {
  if (l.paused)
    return '禁用'
  if (l.pending?.kind === 'unknown' || l.pending?.kind === 'changed' || l.pending?.kind === 'confirm' || (observation(l)?.failure_since != null && Date.now() - observation(l)!.failure_since! >= 3600000))
    return '异常'
  return '正常'
}
const tone = (l: Link) => ({ 正常: 'success', 异常: 'danger', 禁用: 'neutral' } as const)[status(l)]
const resetAt = (l: Link) => status(l) === '正常' ? expectedKeyReset(l) : null
const keys = computed(() => (data.value?.keys || []).map(k => ({ value: k.id, label: k.name, disabled: !!data.value?.state.links[k.id], description: data.value?.state.links[k.id] ? `已关联 ${account(data.value.state.links[k.id])?.name || '账号'}` : undefined })))
const accounts = computed(() => (data.value?.accounts || []).map(a => ({ value: a.account_id, label: a.name || a.email || a.account_id, description: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') ? '仅支持 Codex 账号周窗口' : a.name !== a.email ? a.email || undefined : undefined, disabled: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') })))
const filteredRows = computed(() => filterAndSort(rows.value.map(l => ({
  id: l.key_id,
  name: keyName(l),
  account: accountDisplay(l),
  accountSearch: [account(l)?.name, account(l)?.email, l.account_id].filter(Boolean).join(' '),
  reset: resetAt(l),
  created: l.created_at ?? null,
  status: status(l),
  link: l,
})), search.value, sort.value))
const paged = computed(() => paginate(filteredRows.value, page.value, pageSize.value))
const pagination = computed(() => ({ currentPage: paged.value.currentPage, pageSize: pageSize.value, total: filteredRows.value.length }))
const sortOptions = [
  { value: '', label: '创建时间倒序' },
  ...([{ key: 'name', label: '名称' }, { key: 'account', label: '账号' }, { key: 'reset', label: '重置时间' }, { key: 'status', label: '状态' }] as const)
    .flatMap(({ key, label }) => [{ value: `${key}:asc`, label: `${label}升序` }, { value: `${key}:desc`, label: `${label}降序` }]),
]
const mobileSort = computed({
  get: () => sort.value ? `${sort.value.key}:${sort.value.direction}` : '',
  set: (value: string) => {
    const [key, direction] = value.split(':')
    sort.value = value ? { key: key as SortKey, direction: direction as 'asc' | 'desc' } : undefined
  },
})
function changeSort(key: SortKey) {
  sort.value = sort.value?.key !== key ? { key, direction: 'asc' } : sort.value.direction === 'asc' ? { key, direction: 'desc' } : undefined
}
const ariaSort = (key: SortKey) => sort.value?.key === key ? sort.value.direction === 'asc' ? 'ascending' : 'descending' : 'none'
const sortDirection = (key: SortKey) => sort.value?.key === key ? sort.value.direction : undefined
const pageLinks = computed(() => paged.value.rows.map(row => row.link))
const isSelected = (l: Link) => selection.value.some(item => item.key_id === l.key_id && item.account_id === l.account_id)
const allSelected = computed(() => pageLinks.value.length > 0 && pageLinks.value.every(isSelected))
const indeterminate = computed(() => pageLinks.value.some(isSelected) && !allSelected.value)
function toggleSelection(l: Link) {
  if (busy.value)
    return
  selection.value = isSelected(l) ? selection.value.filter(item => item.key_id !== l.key_id) : [...selection.value, { key_id: l.key_id, account_id: l.account_id }]
}
function toggleAll() {
  if (!busy.value)
    selection.value = togglePageSelection(selection.value, pageLinks.value)
}
watch(search, () => {
  selection.value = []
})
watch(() => data.value?.state.links, (links) => {
  if (links)
    selection.value = reconcileSelection(selection.value, links)
})
async function deleteBatch() {
  if (busy.value || !deleteTargets.value.length)
    return
  busy.value = true
  error.value = ''
  const result = await deleteSequentially(deleteTargets.value, target => command({ action: 'delete', ...target, acknowledge: true }))
  // 仅移除已确认成功的旧身份；失败及未执行项由权威快照核对，不盲目重发。
  selection.value = selection.value.filter(item => !result.completed.some(done => done.key_id === item.key_id && done.account_id === item.account_id))
  let refreshError = ''
  try {
    data.value = await load()
  }
  catch (e) { refreshError = `页面刷新失败：${(e as Error).message}` }
  modal.value = ''
  error.value = [result.error ? `已确认删除 ${result.completed.length} 条，批量操作已停止：${result.error}` : '', refreshError].filter(Boolean).join('；')
  busy.value = false
}
watch([search, sort, pageSize], () => {
  page.value = 1
}, { flush: 'sync' })
// 删除、刷新或搜索缩小结果后同步有效页，避免停留在空页或随后跳回旧页。
watch(() => paged.value.currentPage, (value) => {
  page.value = value
})
const earlyResetEnabled = (l: Link) => l.early_auto ?? data.value?.state.early_auto ?? true
async function setEarlyAuto(l: Link, enabled: boolean) {
  if (busy.value || !data.value || earlyResetEnabled(l) === enabled)
    return
  busy.value = true
  error.value = ''
  let committed = false
  try {
    await command({ action: 'early_auto', key_id: l.key_id, account_id: l.account_id, enabled })
    committed = true
    // 单个关联设置成功即更新对应开关；读取失败时不误改其他密钥。
    const current = data.value?.state.links[l.key_id]
    if (current?.account_id === l.account_id)
      current.early_auto = enabled
    data.value = await load()
  }
  catch (e) {
    const message = (e as Error).message
    // 版本冲突或请求失败后以服务端状态为准，避免 UI 显示未保存的选项。
    try {
      data.value = await load()
    }
    catch { /* 保留请求错误，用户可通过错误提示重试。 */ }
    error.value = committed ? `开关已保存，但页面刷新失败：${message}` : message
  }
  finally { busy.value = false }
}
async function refresh() {
  try {
    data.value = await load()
    error.value = ''
  }
  catch (e) { error.value = (e as Error).message }
}
function open(kind: string, l?: Link) {
  selected.value = l?.key_id || ''
  modal.value = kind
  error.value = ''
  if (kind === 'bulk-delete')
    deleteTargets.value = selection.value.map(item => ({ ...item }))
  if (kind === 'add') {
    keyId.value = ''
    accountId.value = ''
  }
}
async function act(action: string, extra: object = {}) {
  if (busy.value)
    return
  busy.value = true
  error.value = ''
  try {
    await command({ action, key_id: selected.value, ...(action === 'pause' || action === 'resume' || action === 'delete' ? { account_id: data.value?.state.links[selected.value]?.account_id } : {}), ...extra })
    modal.value = ''
    await refresh()
  }
  catch (e) { error.value = (e as Error).message }
  finally { busy.value = false }
}
// 行内启用/禁用仅影响插件关联监测，不改变宿主密钥的启用状态。
async function toggleLink(l: Link) {
  selected.value = l.key_id
  await act(l.paused ? 'resume' : 'pause')
}
let timer: ReturnType<typeof setInterval>
onMounted(() => {
  refresh()
  timer = setInterval(() => {
    if (!document.hidden && !modal.value && !busy.value)
      refresh()
  }, 30000)
})
onUnmounted(() => clearInterval(timer))
</script>

<template>
  <main class="panel">
    <p v-if="error && !modal" role="alert" class="error">
      {{ error }} <BaseButton size="sm" @click="refresh">
        重试
      </BaseButton>
    </p>
    <div class="toolbar">
      <BaseInput v-model="search" class="list-search" :disabled="busy" placeholder="搜索名称、账号" aria-label="搜索名称、账号" type="search">
        <template #prefix>
          <Search :size="18" />
        </template>
      </BaseInput>
      <BaseSelect v-model="mobileSort" class="mobile-sort" :options="sortOptions" aria-label="列表排序" />
      <div class="tools toolbar-actions">
        <BaseButton v-if="selection.length" variant="destructive" :disabled="busy" @click="open('bulk-delete')">
          <Trash2 :size="16" />删除所选（{{ selection.length }}）
        </BaseButton>
        <BaseButton variant="primary" :disabled="busy || !data" @click="open('add')">
          <Plus :size="16" />添加关联
        </BaseButton>
      </div>
    </div>
    <div v-if="data && filteredRows.length" class="mobile-selection">
      <BaseCheckbox label="选择当前页" show-label :model-value="allSelected" :indeterminate="indeterminate" :disabled="busy" @update:model-value="toggleAll" />
    </div>
    <p v-if="!data && !error" class="empty" role="status">
      正在加载关联
    </p>
    <div v-else-if="data && !filteredRows.length" class="empty" role="status">
      暂无关联
    </div>
    <div v-else-if="data" class="table-scroll">
      <table>
        <thead>
          <tr>
            <th class="selection-cell">
              <BaseCheckbox label="选择当前页" :model-value="allSelected" :indeterminate="indeterminate" :disabled="busy" @update:model-value="toggleAll" />
            </th>
            <th v-for="column in [{ key: 'name' as const, label: '密钥名称' }, { key: 'account' as const, label: '账号' }, { key: 'status' as const, label: '状态' }]" :key="column.key" :aria-sort="ariaSort(column.key)">
              <button type="button" class="sort-button" @click="changeSort(column.key)">
                {{ column.label }}
                <span class="sort-indicators" aria-hidden="true">
                  <Triangle :stroke-width="0" :class="{ active: sortDirection(column.key) === 'asc', faded: sortDirection(column.key) === 'desc' }" />
                  <Triangle :stroke-width="0" class="descending" :class="{ active: sortDirection(column.key) === 'desc', faded: sortDirection(column.key) === 'asc' }" />
                </span>
              </button>
            </th>
            <th>订阅</th><th>平台/类型</th><th>本周额度</th>
            <th :aria-sort="ariaSort('reset')">
              <button type="button" class="sort-button" @click="changeSort('reset')">
                重置时间
                <span class="sort-indicators" aria-hidden="true">
                  <Triangle :stroke-width="0" :class="{ active: sortDirection('reset') === 'asc', faded: sortDirection('reset') === 'desc' }" />
                  <Triangle :stroke-width="0" class="descending" :class="{ active: sortDirection('reset') === 'desc', faded: sortDirection('reset') === 'asc' }" />
                </span>
              </button>
            </th>
            <th>提前重置</th><th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="{ link: l } in paged.rows" :key="l.key_id" :class="{ 'selected-row': isSelected(l) }">
            <td class="selection-cell" data-label="选择">
              <BaseCheckbox :label="`选择 ${keyName(l)}`" :model-value="isSelected(l)" :disabled="busy" @update:model-value="toggleSelection(l)" />
            </td>
            <td class="identity key-identity" data-label="密钥名称">
              <span class="usage-identity-primary" :title="keyName(l)">{{ keyName(l) }}</span>
            </td>
            <td class="identity account-identity" data-label="账号">
              <span class="usage-identity-primary" :title="accountDisplay(l)">{{ accountDisplay(l) }}</span>
            </td>
            <td data-label="状态">
              <div class="status">
                <BaseTag :type="tone(l)" size="sm">
                  {{ status(l) }}
                </BaseTag>
              </div>
            </td>
            <td data-label="订阅">
              <AccountPlanBadge v-if="account(l)?.plan_type || account(l)?.authentication_kind === 'api_key'" :plan-type="account(l)?.plan_type" :authentication-kind="account(l)?.authentication_kind" :plan-type-display="subscriptionLabel(account(l)?.plan_type, account(l)?.authentication_kind)" size="sm" />
              <span v-else class="muted">—</span>
            </td>
            <td data-label="平台/类型">
              <ProviderIconGroup :provider="account(l)?.provider_id" :authentication-kind="account(l)?.authentication_kind" />
            </td>
            <td data-label="本周额度" class="numeric">
              <span v-if="budget(l)"><span class="amount">{{ money(budget(l)!.weekly_used_usd) }}</span><span class="limit"> / {{ Number(budget(l)!.weekly_limit_usd) === 0 ? '不限额' : money(budget(l)!.weekly_limit_usd) }}</span></span><span v-else class="muted">无法读取预算</span>
            </td>
            <td data-label="重置时间" class="reset-time">
              <span v-if="resetAt(l) != null">{{ displayResetDate(resetAt(l)) }}</span>
              <span v-else class="muted">-</span>
            </td>
            <td data-label="提前重置" class="early-reset-cell">
              <BaseSwitch
                :model-value="earlyResetEnabled(l)"
                :label="`${keyName(l)}提前重置`"
                :disabled="busy || !data"
                @update:model-value="enabled => setEarlyAuto(l, enabled)"
              />
            </td>
            <td class="row-actions" data-label="操作">
              <div class="tools">
                <BaseIconButton :label="l.paused ? '启用' : '禁用'" :title="l.paused ? '启用' : '禁用'" :disabled="busy" @click="toggleLink(l)">
                  <Power v-if="l.paused" :size="16" />
                  <PowerOff v-else :size="16" />
                </BaseIconButton>
                <BaseIconButton label="删除" title="删除" :disabled="busy" @click="open('delete', l)">
                  <Trash2 :size="16" />
                </BaseIconButton>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="data" class="list-footer">
      <BaseTablePagination
        :pagination="pagination"
        :loading="busy"
        @page-change="page = $event"
        @page-size-change="pageSize = $event"
      />
    </div>
  </main>
  <BaseModal v-model="modalOpen" :title="({ 'add': '添加关联', 'delete': '删除关联', 'bulk-delete': '批量删除' })[modal] || '关联'" :dismissible="!busy" :draggable="false">
    <p v-if="error" role="alert" class="error">
      {{ error }}
    </p>
    <template v-if="modal === 'add'">
      <label class="field">密钥<BaseSelect v-model="keyId" :options="keys" filterable placeholder="选择密钥" :disabled="busy" /></label>
      <p v-if="keyId && Number(data?.budgets[keyId]?.weekly_limit_usd) === 0" class="muted">
        该密钥未设周限额，清零无实际作用
      </p>
      <label class="field">账号<BaseSelect v-model="accountId" :options="accounts" filterable placeholder="选择账号" :disabled="busy" /></label>
    </template>
    <template v-else-if="modal === 'delete'">
      <p>
        确定删除 {{ link ? keyName(link) : '此密钥' }} 的关联吗？
      </p>
    </template>
    <template v-else-if="modal === 'bulk-delete'">
      <p>确定删除选中的 {{ deleteTargets.length }} 条关联吗？</p>
    </template>
    <template #footer>
      <BaseButton :disabled="busy" @click="modal = ''">
        取消
      </BaseButton>
      <BaseButton v-if="modal === 'add'" variant="primary" :loading="busy" :disabled="!keyId || !accountId" @click="act('add', { key_id: keyId, account_id: accountId })">
        添加
      </BaseButton>
      <BaseButton v-if="modal === 'bulk-delete'" variant="destructive" :loading="busy" :disabled="busy" @click="deleteBatch">
        删除
      </BaseButton>
      <BaseButton v-if="modal === 'delete'" variant="destructive" :disabled="busy" @click="act('delete', { acknowledge: true })">
        删除关联
      </BaseButton>
    </template>
  </BaseModal>
</template>
