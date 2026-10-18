<script setup lang="ts">
import type { Link, Sample, Snapshot } from './api/modules/follow'
import { BaseButton, BaseCheckbox, BaseIconButton, BaseModal, BaseSelect, BaseTag } from '@codex-proxy/ui'
import { Plus, RefreshCw } from '@lucide/vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { command, load } from './api/modules/follow'
import { displayDate, money, percent, subscriptionLabel } from './display'
import ProviderIconGroup from './components/ProviderIconGroup.vue'
import AccountPlanBadge from './components/AccountPlanBadge.vue'

const data = ref<Snapshot>()
const busy = ref(false)
const error = ref('')
const modal = ref('')
const selected = ref('')
const keyId = ref('')
const accountId = ref('')
const preview = ref<Sample>()
const previewError = ref('')
const previewBusy = ref(false)
const acknowledge = ref(false)
const modalOpen = computed({ get: () => !!modal.value, set: (value) => {
  if (!value)
    modal.value = ''
} })
const rows = computed(() => Object.values(data.value?.state.links || {}))
const link = computed(() => data.value?.state.links[selected.value])
const account = (l: Link) => data.value?.accounts.find(a => a.account_id === l.account_id)
const accountName = (l: Link) => account(l)?.name?.trim() || account(l)?.email || '账号已删除'
const accountDisplay = (l: Link) => account(l)?.email?.trim() || accountName(l)
const accountSecondary = (l: Link) => account(l)?.email?.trim() && account(l)?.name?.trim() && account(l)?.email?.trim() !== account(l)?.name?.trim() ? account(l)?.name?.trim() : ''
const observation = (l: Link) => data.value?.state.accounts[l.account_id]
const sample = (l: Link) => observation(l)?.sample || l.baseline
const budget = (l: Link) => data.value?.budgets[l.key_id]
const keyName = (l: Link) => data.value?.keys.find(k => k.id === l.key_id)?.name || 'Key 已删除'
const date = (value: number | null | undefined, full = false) => displayDate(value, full)
function status(l: Link): '监测中' | '等待确认' | '需处理' | '已暂停' {
  if (l.paused)
    return '已暂停'
  if (l.pending?.kind === 'unknown' || l.pending?.kind === 'ready' || (observation(l)?.failure_since != null && Date.now() - observation(l)!.failure_since! >= 3600000))
    return '需处理'
  return ['confirm', 'changed'].includes(l.pending?.kind || '') ? '等待确认' : '监测中'
}
const tone = (l: Link) => ({ 监测中: 'success', 等待确认: 'warning', 需处理: 'danger', 已暂停: 'neutral' } as const)[status(l)]
const needs = computed(() => rows.value.filter(l => ['等待确认', '需处理'].includes(status(l))))
const keys = computed(() => (data.value?.keys || []).map(k => ({ value: k.id, label: k.name, disabled: !!data.value?.state.links[k.id], description: data.value?.state.links[k.id] ? `已关联 ${account(data.value.state.links[k.id])?.name || '账号'}` : undefined })))
const accounts = computed(() => (data.value?.accounts || []).map(a => ({ value: a.account_id, label: a.name || a.email || a.account_id, description: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') ? '仅支持 Codex 账号周窗口' : a.name !== a.email ? a.email || undefined : undefined, disabled: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') })))
async function setEarlyAuto(enabled: boolean) {
  if (busy.value || !data.value || data.value.state.early_auto === enabled)
    return
  busy.value = true
  error.value = ''
  let committed = false
  try {
    await command({ action: 'early_auto', version: data.value.version, enabled })
    committed = true
    // 写入成功即更新勾选状态；后续 GET 失败时不错误回退为旧值。
    data.value.state.early_auto = enabled
    data.value = await load()
  }
  catch (e) {
    const message = (e as Error).message
    // 版本冲突或请求失败后以服务端状态为准，避免 UI 显示未保存的选项。
    try {
      data.value = await load()
    }
    catch { /* 保留请求错误，用户可手动刷新。 */ }
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
  acknowledge.value = false
  error.value = ''
  if (kind === 'add') {
    keyId.value = ''
    accountId.value = ''
    preview.value = undefined
    previewError.value = ''
  }
}
async function act(action: string, extra: object = {}, close = true) {
  if (busy.value)
    return
  busy.value = true
  error.value = ''
  try {
    await command({ action, key_id: selected.value, version: data.value?.version, acknowledge: acknowledge.value, ...extra })
    if (close)
      modal.value = ''
    await refresh()
  }
  catch (e) { error.value = (e as Error).message }
  finally { busy.value = false }
}
// 行内启用/禁用仅影响插件关联监测，不改变宿主 Key 的启用状态。
async function toggleLink(l: Link) {
  selected.value = l.key_id
  await act(l.paused ? 'resume' : 'pause')
}
let previewGeneration = 0
watch(accountId, async (id) => {
  const generation = ++previewGeneration
  preview.value = undefined
  previewError.value = ''
  if (!id)
    return
  previewBusy.value = true
  try {
    const result = await command({ action: 'preview', account_id: id })
    if (generation === previewGeneration)
      preview.value = result.sample
  }
  catch (e) {
    if (generation === previewGeneration)
      previewError.value = (e as Error).message
  }
  finally {
    if (generation === previewGeneration)
      previewBusy.value = false
  }
})
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
        <div><span class="count">{{ rows.length }} 个关联</span></div>
        <div class="tools toolbar-actions">
          <BaseIconButton label="刷新列表" title="刷新列表" :disabled="busy" @click="refresh">
            <RefreshCw :size="17" />
          </BaseIconButton>
          <div class="inline-early-reset" title="开启后自动处理账号提前重置；关闭后需要手动确认，正常换周仍自动处理">
            <BaseCheckbox :model-value="data?.state.early_auto ?? true" label="提前重置自动清零" :disabled="busy || !data" @update:model-value="setEarlyAuto" />
          </div>
          <BaseButton variant="primary" :disabled="busy || !data" @click="open('add')">
            <Plus :size="16" />添加关联
          </BaseButton>
        </div>
      </div>
      <div v-if="needs.length" class="banner">
        <span>{{ needs.length }} 个关联需要处理</span><BaseButton size="sm" @click="open('process', needs[0])">
          查看并处理
        </BaseButton>
      </div>
      <p v-if="!data && !error" class="empty" role="status">
        正在加载关联
      </p>
      <div v-else-if="data && !rows.length" class="empty">
        <p>尚未建立关联</p><small>选择 Key 和对应账号，开始跟随周额度重置</small><BaseButton variant="primary" @click="open('add')">
          <Plus :size="16" />添加关联
        </BaseButton>
      </div>
      <div v-else-if="data" class="table-scroll">
        <table>
        <thead><tr><th>密钥名称</th><th>账号</th><th>订阅</th><th>平台/类型</th><th>本周用量</th><th>账号使用情况</th><th>账号预计重置</th><th>状态</th><th><span class="sr-only">操作</span></th></tr></thead>
        <tbody>
          <tr v-for="l in rows" :key="l.key_id">
            <td class="identity key-identity" data-label="密钥名称">
              <span class="usage-identity-primary" :title="keyName(l)">{{ keyName(l) }}</span>
            </td>
            <td class="identity account-identity" data-label="账号">
              <span class="usage-identity-primary" :title="accountDisplay(l)">{{ accountDisplay(l) }}</span><span v-if="accountSecondary(l)" class="usage-identity-secondary" :title="accountSecondary(l)">{{ accountSecondary(l) }}</span>
            </td>
            <td data-label="订阅">
              <AccountPlanBadge v-if="account(l)?.plan_type || account(l)?.authentication_kind === 'api_key'" :plan-type="account(l)?.plan_type" :authentication-kind="account(l)?.authentication_kind" :plan-type-display="subscriptionLabel(account(l)?.plan_type, account(l)?.authentication_kind)" size="sm" />
              <span v-else class="muted">—</span>
            </td>
            <td data-label="平台/类型">
              <ProviderIconGroup :provider="account(l)?.provider_id" :authentication-kind="account(l)?.authentication_kind" />
            </td>
            <td data-label="本周用量" class="numeric">
              <span v-if="budget(l)"><span class="amount">{{ money(budget(l)!.weekly_used_usd) }}</span><span class="limit"> / {{ Number(budget(l)!.weekly_limit_usd) === 0 ? '不限额' : money(budget(l)!.weekly_limit_usd) }}</span></span><span v-else class="muted">无法读取预算</span>
            </td>
            <td data-label="账号使用情况" class="numeric">
              {{ percent(sample(l).used) }}
            </td>
            <td data-label="账号预计重置" class="reset-time">
              <span :title="date(sample(l).reset, true)">{{ date(sample(l).reset) }}</span><small v-if="observation(l)?.error">最近检查失败</small><small v-else-if="sample(l).reset <= Date.now()">等待账号更新周期</small>
            </td>
            <td data-label="状态">
              <div class="status">
                <BaseTag :type="tone(l)" size="sm">
                  {{ status(l) }}
                </BaseTag><BaseButton v-if="['等待确认', '需处理'].includes(status(l))" size="sm" :disabled="busy" @click="open('process', l)">
                  处理
                </BaseButton>
              </div>
            </td>
            <td class="row-actions" data-label="操作">
              <div class="tools">
                <BaseIconButton label="立即刷新" title="立即刷新" :disabled="busy || l.paused" @click="selected = l.key_id; act('check')">
                  <RefreshCw :size="16" />
                </BaseIconButton>
                <BaseButton size="sm" :disabled="busy" :title="l.paused ? '启用关联监测并重新建立基线' : '禁用关联监测，不影响宿主 Key'" @click="toggleLink(l)">
                  {{ l.paused ? '启用' : '禁用' }}
                </BaseButton>
                <BaseButton size="sm" variant="destructive" :disabled="busy" title="删除关联" @click="open('delete', l)">
                  删除
                </BaseButton>
              </div>
            </td>
          </tr>
        </tbody>
        </table>
      </div>
  </main>
  <BaseModal v-model="modalOpen" :title="({ add: '添加关联', process: '处理关联', delete: '删除关联' })[modal] || '关联'" :dismissible="!busy" :draggable="false">
    <p v-if="error" role="alert" class="error">
      {{ error }}
    </p>
    <template v-if="modal === 'add'">
      <label class="field">Key<BaseSelect v-model="keyId" :options="keys" filterable placeholder="选择现有 Key" :disabled="busy" /></label>
      <p v-if="keyId && Number(data?.budgets[keyId]?.weekly_limit_usd) === 0" class="muted">
        该 Key 未设周限额，清零无实际作用
      </p>
      <label class="field">额度来源账号<BaseSelect v-model="accountId" :options="accounts" filterable placeholder="选择账号名称或邮箱" :disabled="busy" /></label>
      <p v-if="previewBusy" role="status">
        正在识别周窗口
      </p><p v-else-if="previewError" class="error">
        {{ previewError }}
      </p><p v-else-if="preview">
        已识别 Codex 周窗口，上游预计重置 {{ date(preview.reset) }}
      </p>
    </template>
    <template v-else-if="link && modal === 'process'">
      <h3>{{ keyName(link) }}</h3><p class="muted">
        {{ accountName(link) }}
      </p>
      <template v-if="link.pending?.kind === 'unknown'">
        <p>重置结果未知，已停止自动重试</p><p class="muted">
          保留现有用量并跳过会重新建立基线
        </p><BaseCheckbox v-model="acknowledge" label="我了解可能清掉期间新产生的消费" />
      </template>
      <template v-else-if="link.pending?.kind === 'changed'">
        <p>目标窗口标识或时长发生变化，已停止自动清零</p>
        <p v-for="window in link.pending.windows" :key="window.key">
          {{ window.key }} · {{ window.window_seconds }} 秒 · {{ date(window.reset_at_ms, true) }}
        </p>
        <p class="muted">
          请在宿主检查账号套餐，恢复唯一 Codex 周窗口后再确认或更新基线
        </p>
      </template>
      <template v-else-if="link.pending?.kind === 'confirm'">
        <p>{{ link.pending.reason === 'early' ? '账号提前进入新周期，自动清零开关已关闭' : '周期边界变化无法解释，需要你确认' }}</p><dl><dt>原边界</dt><dd>{{ date(link.baseline.reset, true) }}</dd><dt>新边界</dt><dd>{{ date(link.pending.sample.reset, true) }}</dd><dt>推算的新窗口起点</dt><dd>{{ date(link.pending.sample.reset - 604800000, true) }}</dd></dl><p class="muted">
          确认后会清零当前周用量，此操作无法撤销
        </p>
      </template>
      <p v-else>
        {{ observation(link)?.error || '读取预算失败，尚未调用清零' }}
      </p>
    </template>
    <template v-else-if="modal === 'delete'">
      <p>确定删除“{{ link ? keyName(link) : '此 Key' }}”的关联吗？</p>
      <p class="muted">仅删除本插件的关联记录，不会删除或停用宿主 Key，也不能撤销已执行的重置。</p>
      <BaseCheckbox v-model="acknowledge" label="确认删除此关联" />
    </template>
    <template #footer>
      <span v-if="modal === 'add'" class="muted">仅建立基线，不清零</span><BaseButton :disabled="busy" @click="modal = ''">
        取消
      </BaseButton>
      <BaseButton v-if="modal === 'add'" variant="primary" :loading="busy" :disabled="!keyId || !preview || previewBusy" @click="act('add', { key_id: keyId, account_id: accountId })">
        添加
      </BaseButton>
      <template v-if="modal === 'process' && link?.pending">
        <BaseButton :disabled="busy" @click="act('skip')">
          {{ link.pending.kind === 'unknown' ? '保留现有用量并跳过' : '忽略并更新基线' }}
        </BaseButton>
        <BaseButton v-if="link.pending.kind === 'unknown'" variant="destructive" :disabled="!acknowledge || busy" @click="act('retry')">
          再清零一次
        </BaseButton>
        <BaseButton v-if="['confirm', 'changed'].includes(link.pending.kind)" variant="primary" :loading="busy" @click="act('confirm')">
          确认并清零
        </BaseButton>
      </template>
      <BaseButton v-if="modal === 'process' && !link?.pending" :loading="busy" @click="act('check')">
        立即检查
      </BaseButton>
      <BaseButton v-if="modal === 'delete'" variant="destructive" :disabled="!acknowledge || busy" @click="act('delete')">
        删除关联
      </BaseButton>
    </template>
  </BaseModal>
</template>
