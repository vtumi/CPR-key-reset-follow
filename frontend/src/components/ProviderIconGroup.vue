<script setup lang="ts">
import { Key, LinkAlt, Openai, Xai } from '@boxicons/vue'
import { Fingerprint, Plug } from '@lucide/vue'
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  provider?: string | null
  authenticationKind?: string | null
  size?: 'xs' | 'sm' | 'md'
}>(), {
  size: 'md',
})

const providerId = computed(() => props.provider?.trim().toLowerCase())
const providerIcon = computed(() => {
  switch (providerId.value) {
    case 'openai': return Openai
    case 'xai': return Xai
    default: return providerId.value ? Plug : undefined
  }
})
const providerLabel = computed(() => {
  switch (providerId.value) {
    case 'openai': return 'OpenAI'
    case 'xai': return 'xAI'
    default: return props.provider?.trim() || '未知平台'
  }
})
const authenticationIcon = computed(() => {
  switch (props.authenticationKind?.trim().toLowerCase()) {
    case 'oauth': return LinkAlt
    case 'api_key': return Key
    default: return props.authenticationKind?.trim() ? Fingerprint : undefined
  }
})
const authenticationLabel = computed(() => {
  const kind = props.authenticationKind?.trim()
  switch (kind?.toLowerCase()) {
    case 'oauth': return 'OAuth'
    case 'api_key': return 'API Key'
    default: return kind ? `自定义认证 · ${kind}` : '未知认证类型'
  }
})
const showAuthenticationKind = computed(() => props.authenticationKind !== undefined)
const iconContainerClass = computed(() => props.size === 'xs' ? 'size-4.5 rounded-md' : props.size === 'sm' ? 'size-5 rounded-md' : 'size-7 rounded-lg')
const iconClass = computed(() => props.size === 'xs' ? 'size-2.5' : props.size === 'sm' ? 'size-3' : 'size-4')
const gapClass = computed(() => props.size === 'xs' ? 'gap-0.75' : props.size === 'sm' ? 'gap-1' : 'gap-2')
</script>

<template>
  <span class="inline-flex items-center whitespace-nowrap" :class="gapClass">
    <span
      class="inline-flex shrink-0 items-center justify-center bg-cp-fill-secondary/80 text-cp-text"
      :class="iconContainerClass"
      :title="providerLabel"
    >
      <component :is="providerIcon" v-if="providerIcon" :class="iconClass" />
      <span v-else class="text-[10px] font-heavy text-cp-text-quaternary">?</span>
    </span>
    <span
      v-if="showAuthenticationKind"
      class="inline-flex shrink-0 items-center justify-center bg-cp-fill-secondary/80 text-cp-text"
      :class="iconContainerClass"
      :title="authenticationLabel"
    >
      <component :is="authenticationIcon" v-if="authenticationIcon" :class="iconClass" />
      <span v-else class="text-[10px] font-heavy text-cp-text-quaternary">?</span>
    </span>
    <span class="sr-only">
      {{ providerLabel }}<template v-if="showAuthenticationKind">，{{ authenticationLabel }}</template>
    </span>
  </span>
</template>
