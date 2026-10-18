<script setup lang="ts">
import type { Link, Sample, Snapshot } from './api/modules/follow'
import { BaseButton, BaseIconButton, BaseModal, BaseSelect, BaseSwitch, BaseTag } from '@codex-proxy/ui'
import { Plus, Power, PowerOff, RefreshCw, Trash2 } from '@lucide/vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { command, load } from './api/modules/follow'
import AccountPlanBadge from './components/AccountPlanBadge.vue'
import ProviderIconGroup from './components/ProviderIconGroup.vue'
import { displayDate, money, percent, subscriptionLabel } from './display'

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
const sample = (l: Link) => observation(l)?.sample || l.baseline
const budget = (l: Link) => data.value?.budgets[l.key_id]
const keyName = (l: Link) => data.value?.keys.find(k => k.id === l.key_id)?.name || '密钥已删除'
const date = (value: number | null | undefined, full = false) => displayDate(value, full)
function status(l: Link): '正常' | '异常' | '禁用' {
  if (l.paused)
    return '禁用'
  if (l.pending?.kind === 'unknown' || l.pending?.kind === 'changed' || l.pending?.kind === 'confirm' || (observation(l)?.failure_since != null && Date.now() - observation(l)!.failure_since! >= 3600000))
    return '异常'
  return '正常'
}
const tone = (l: Link) => ({ 正常: 'success', 异常: 'danger', 禁用: 'neutral' } as const)[status(l)]
const keys = computed(() => (data.value?.keys || []).map(k => ({ value: k.id, label: k.name, disabled: !!data.value?.state.links[k.id], description: data.value?.state.links[k.id] ? `已关联 ${account(data.value.state.links[k.id])?.name || '账号'}` : undefined })))
const accounts = computed(() => (data.value?.accounts || []).map(a => ({ value: a.account_id, label: a.name || a.email || a.account_id, description: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') ? '仅支持 Codex 账号周窗口' : a.name !== a.email ? a.email || undefined : undefined, disabled: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') })))
async function setEarlyAuto(enabled: boolean) {
  if (busy.value || !data.value || data.value.state.early_auto === enabled)
    return
  busy.value = true
  error.value = ''
  let committed = false
  try {
    await command({ action: 'early_auto', enabled })
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
    await command({ action, key_id: selected.value, ...(action === 'check' || action === 'pause' || action === 'resume' || action === 'delete' ? { account_id: data.value?.state.links[selected.value]?.account_id } : {}), ...extra })
    if (close)
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
        <div class="inline-early-reset" title="提前重置：开启时自动清零，关闭时仅更新周期基线">
          <BaseSwitch :model-value="data?.state.early_auto ?? true" label="提前重置" :disabled="busy || !data" @update:model-value="setEarlyAuto" />
        </div>
        <BaseButton variant="primary" :disabled="busy || !data" @click="open('add')">
          <Plus :size="16" />添加关联
        </BaseButton>
      </div>
    </div>
    <p v-if="!data && !error" class="empty" role="status">
      正在加载关联
    </p>
    <div v-else-if="data && !rows.length" class="empty">
      <p>尚未建立关联</p><small>选择密钥和对应账号，开始跟随周额度重置</small><BaseButton variant="primary" @click="open('add')">
        <Plus :size="16" />添加关联
      </BaseButton>
    </div>
    <div v-else-if="data" class="table-scroll">
      <table>
        <thead><tr><th>密钥名称</th><th>账号</th><th>订阅</th><th>平台/类型</th><th>本周额度</th><th>账号本周用量</th><th>账号重置时间</th><th>状态</th><th><span class="sr-only">操作</span></th></tr></thead>
        <tbody>
          <tr v-for="l in rows" :key="l.key_id">
            <td class="identity key-identity" data-label="密钥名称">
              <span class="usage-identity-primary" :title="keyName(l)">{{ keyName(l) }}</span>
            </td>
            <td class="identity account-identity" data-label="账号">
              <span class="usage-identity-primary" :title="accountDisplay(l)">{{ accountDisplay(l) }}</span>
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
            <td data-label="账号本周用量" class="numeric">
              {{ percent(sample(l).used) }}
            </td>
            <td data-label="账号重置时间" class="reset-time">
              <span :title="date(sample(l).reset, true)">{{ date(sample(l).reset) }}</span><small v-if="observation(l)?.error">最近检查失败</small><small v-else-if="sample(l).reset <= Date.now()">等待账号更新周期</small>
            </td>
            <td data-label="状态">
              <div class="status">
                <BaseTag :type="tone(l)" size="sm">
                  {{ status(l) }}
                </BaseTag>
              </div>
            </td>
            <td class="row-actions" data-label="操作">
              <div class="tools">
                <BaseIconButton label="立即刷新" title="立即刷新" :disabled="busy || l.paused" @click="selected = l.key_id; act('check')">
                  <RefreshCw :size="16" />
                </BaseIconButton>
                <BaseIconButton :label="l.paused ? '启用关联' : '禁用关联'" :title="l.paused ? '启用关联' : '禁用关联'" :disabled="busy" @click="toggleLink(l)">
                  <Power v-if="l.paused" :size="16" />
                  <PowerOff v-else :size="16" />
                </BaseIconButton>
                <BaseIconButton label="删除关联" title="删除关联" :disabled="busy" @click="open('delete', l)">
                  <Trash2 :size="16" />
                </BaseIconButton>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </main>
  <BaseModal v-model="modalOpen" :title="({ add: '添加关联', delete: '删除关联' })[modal] || '关联'" :dismissible="!busy" :draggable="false">
    <p v-if="error" role="alert" class="error">
      {{ error }}
    </p>
    <template v-if="modal === 'add'">
      <label class="field">密钥<BaseSelect v-model="keyId" :options="keys" filterable placeholder="选择密钥" :disabled="busy" /></label>
      <p v-if="keyId && Number(data?.budgets[keyId]?.weekly_limit_usd) === 0" class="muted">
        该密钥未设周限额，清零无实际作用
      </p>
      <label class="field">账号<BaseSelect v-model="accountId" :options="accounts" filterable placeholder="选择账号" :disabled="busy" /></label>
      <p v-if="previewBusy" role="status">
        正在识别周窗口
      </p><p v-else-if="previewError" class="error">
        {{ previewError }}
      </p><p v-else-if="preview">
        已识别 Codex 周窗口，上游预计重置 {{ date(preview.reset) }}
      </p>
    </template>
    <template v-else-if="modal === 'delete'">
      <p>
        确定删除“{{ link ? keyName(link) : '此密钥' }}”的关联吗？
      </p>
    </template>
    <template #footer>
      <BaseButton :disabled="busy" @click="modal = ''">
        取消
      </BaseButton>
      <BaseButton v-if="modal === 'add'" variant="primary" :loading="busy" :disabled="!keyId || !preview || previewBusy" @click="act('add', { key_id: keyId, account_id: accountId })">
        添加
      </BaseButton>
      <BaseButton v-if="modal === 'delete'" variant="destructive" :disabled="busy" @click="act('delete', { acknowledge: true })">
        删除关联
      </BaseButton>
    </template>
  </BaseModal>
</template>
