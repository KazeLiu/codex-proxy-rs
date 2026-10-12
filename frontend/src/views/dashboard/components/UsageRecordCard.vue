<script setup lang="ts">
import type { dashboardSnapshotView } from '../presenter'

import { useTableColumns, ZCard, ZSwitch, ZTableColumnSettings } from '@codex-proxy/ui'
import { computed, useTemplateRef } from 'vue'
import { usageRecordColumns } from '@/components/usage/shared/columns'
import UsageRecordsTable from '@/components/usage/UsageRecordsTable.vue'

type DashboardSnapshot = ReturnType<typeof dashboardSnapshotView>

withDefaults(defineProps<{
  rows: DashboardSnapshot['usageRecords']
  loading?: boolean
  error?: string
}>(), {
  loading: false,
  error: '',
})

const showFailed = defineModel<boolean>('showFailed', { default: false })
const columns = computed(() => usageRecordColumns
  .filter(column => column.key !== 'actions' && column.key !== 'clientApiKeyName')
  .map(column => column.key === 'logicalOutcome' || column.key === 'message'
    ? { ...column, defaultHidden: !showFailed.value }
    : column),
)
const { visibleColumns, columnOptions, setColumnVisible, setColumnOrder, resetColumns }
  = useTableColumns(columns, 'dashboard-usage-records')
const recordsTable = useTemplateRef('recordsTable')

function resetTableColumns() {
  resetColumns()
  recordsTable.value?.resetColumnWidths()
}
</script>

<template>
  <ZCard
    as="article"
    title="使用记录"
    :description="showFailed ? '最近 10 条请求' : '最近 10 条成功请求'"
    class="h-117 w-full"
  >
    <template #actions>
      <div class="flex items-center gap-3">
        <ZSwitch v-model="showFailed" label="显示失败" aria-label="在使用记录中显示失败请求" />
        <ZTableColumnSettings
          :options="columnOptions"
          @change="setColumnVisible"
          @reorder="setColumnOrder"
          @reset="resetTableColumns"
        />
      </div>
    </template>
    <template #body>
      <div class="flex h-91 w-full overflow-hidden">
        <UsageRecordsTable
          ref="recordsTable"
          table-id="dashboard-usage-records"
          class="min-w-0 flex-1"
          :columns="visibleColumns"
          :rows="rows"
          :loading="loading"
          :empty-text="error ? `加载失败：${error}` : showFailed ? '暂无请求记录' : '暂无成功记录'"
        />
      </div>
    </template>
  </ZCard>
</template>
