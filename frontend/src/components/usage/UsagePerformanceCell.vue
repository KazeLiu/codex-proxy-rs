<script setup lang="ts">
import type { UsagePerformanceRecord } from './shared/presenter'

import { computed } from 'vue'

import { useI18n } from 'vue-i18n'
import { usagePerformanceDetails } from './shared/presenter'

const props = defineProps<{
  record: UsagePerformanceRecord
}>()

const { t } = useI18n({ useScope: 'global' })

const performance = computed(() => usagePerformanceDetails(props.record))
</script>

<template>
  <div class="grid justify-items-end gap-y-1.5 whitespace-nowrap font-mono text-cp-sm leading-none font-heavy tabular-nums">
    <span class="inline-flex items-baseline gap-1.5 text-cp-text">
      <span v-if="performance.throughputSource" class="text-cp-xs font-normal text-cp-text-secondary">
        {{ performance.throughputSource === 'local' ? t('usage.latency.local') : t('usage.performance.upstream') }}
      </span>
      {{ performance.throughputDisplay }}
    </span>
    <span class="text-cp-xs text-cp-text-secondary">
      <span :title="t('usage.performance.ttftHelp')" class="text-cp-text-quaternary">TTFT</span>
      {{ performance.firstTokenDisplay }}
    </span>
  </div>
</template>
