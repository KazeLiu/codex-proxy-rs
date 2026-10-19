<script setup lang="ts">
import type { UpstreamQuotaAccount } from '@/api'
import AccountQuotaEntry from './AccountQuotaEntry.vue'

defineProps<{ accounts: UpstreamQuotaAccount[] }>()
</script>

<template>
  <div class="grid min-w-0 gap-4">
    <section v-for="entry in accounts" :key="entry.id" class="grid min-w-0 gap-2 rounded-cp bg-cp-fill-quaternary p-3">
      <div class="flex min-w-0 flex-wrap items-baseline justify-between gap-x-2 gap-y-1">
        <div class="flex min-w-0 items-baseline gap-2">
          <h4 class="m-0 truncate text-cp-xs font-heavy text-cp-text" :title="entry.label">
            {{ entry.label }}
          </h4>
          <span v-if="entry.status === 'stale'" class="shrink-0 text-cp-xs text-cp-warning-text">数据过期</span>
        </div>
        <span v-if="entry.observedAt" class="shrink-0 text-cp-xs whitespace-nowrap text-cp-text-quaternary">上游额度最后获取时间：{{ entry.observedAtDisplay }}</span>
      </div>
      <AccountQuotaEntry v-if="entry.windows.length" :windows="entry.windows" :label="null" remaining />
      <span v-else class="text-cp-xs text-cp-text-tertiary">额度待观测</span>
    </section>
  </div>
</template>
