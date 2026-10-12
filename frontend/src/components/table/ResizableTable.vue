<script setup lang="ts" generic="Row extends object">
import type { TableColumn } from '@codex-proxy/ui'
import { ZTable } from '@codex-proxy/ui'
import { toRef } from 'vue'
import { useTableColumnWidths } from '@/composables/useTableColumnWidths'

const props = withDefaults(defineProps<{
  tableId: string
  columns: TableColumn<Row>[]
  data: Row[]
  minimumWidths: Readonly<Record<string, number>>
  loading?: boolean
  emptyText?: string
}>(), {
  loading: false,
  emptyText: '暂无记录',
})

const { columns: resizedColumns, minimumWidth, startResize, resizeWithKeyboard, resetColumnWidth, resetColumnWidths }
  = useTableColumnWidths(toRef(props, 'columns'), toRef(props, 'tableId'), props.minimumWidths)

defineExpose({ resetColumnWidths })
</script>

<template>
  <ZTable
    class="resizable-table"
    :columns="resizedColumns"
    :data="data"
    :loading="loading"
    :empty-text="emptyText"
    show-header-when-empty
  >
    <template v-for="column in resizedColumns" :key="column.key" #[`header-${column.key}`]>
      <span class="relative block min-w-0">
        <span class="block truncate">{{ column.label }}</span>
        <button
          type="button"
          :aria-label="`调整${column.label}列宽，当前 ${Math.round(column.width)} 像素，最小 ${minimumWidth(column.key)} 像素`"
          title="拖动或方向键调整列宽，双击恢复默认"
          class="absolute -top-3 -right-4 -bottom-3 flex w-3 touch-none cursor-col-resize items-center justify-center border-0 bg-transparent p-0 text-inherit outline-none hover:text-cp-primary-text focus-visible:text-cp-primary-text"
          @pointerdown.stop="startResize($event, column.key)"
          @keydown.stop="resizeWithKeyboard($event, column.key)"
          @dblclick.stop="resetColumnWidth(column.key)"
        >
          <span class="h-4 w-px bg-current opacity-30" />
        </button>
      </span>
    </template>

    <template v-for="(_, name) in $slots" #[name]="scope">
      <slot :name="name" v-bind="scope" />
    </template>
  </ZTable>
</template>

<style scoped>
.resizable-table :deep(th > div) {
  overflow: visible;
}
</style>
