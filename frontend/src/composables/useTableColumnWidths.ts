import type { TableColumn } from '@codex-proxy/ui'
import type { MaybeRefOrGetter } from 'vue'
import { resolveColumns } from '@codex-proxy/ui'
import { useStorage } from '@vueuse/core'
import { computed, onScopeDispose, shallowRef, toValue } from 'vue'

export function useTableColumnWidths<Row extends object>(
  source: MaybeRefOrGetter<TableColumn<Row>[]>,
  tableId: MaybeRefOrGetter<string>,
  minimumWidths: Readonly<Record<string, number>>,
) {
  const widths = useStorage<Record<string, number>>(
    computed(() => `codex-proxy:table-column-widths:${toValue(tableId)}`),
    {},
    undefined,
    {
      shallow: true,
      writeDefaults: false,
      serializer: {
        read: (value) => {
          try {
            const parsed: unknown = JSON.parse(value)
            if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
              return Object.fromEntries(Object.entries(parsed).filter(
                ([, width]) => typeof width === 'number' && Number.isFinite(width) && width > 0,
              ))
            }
          }
          catch {}
          return {}
        },
        write: JSON.stringify,
      },
    },
  )
  const draft = shallowRef<{ key: string, width: number } | null>(null)
  const columns = computed(() => resolveColumns(toValue(source)).map(column => ({
    ...column,
    minWidth: minimumWidth(column.key),
    width: Math.max(
      minimumWidth(column.key),
      draft.value?.key === column.key ? draft.value.width : widths.value[column.key] ?? column.basisWidth,
    ),
  })))
  let stopResize: (() => void) | undefined

  function minimumWidth(key: string) {
    return minimumWidths[key] ?? 64
  }

  function saveWidth(key: string, width: number) {
    if (!Number.isFinite(width))
      return
    widths.value = { ...widths.value, [key]: Math.max(minimumWidth(key), Math.round(width)) }
  }

  function startResize(event: PointerEvent, key: string) {
    if (event.button !== 0)
      return
    const handle = event.currentTarget
    if (!(handle instanceof HTMLElement))
      return
    const header = handle.closest('th')
    if (!header)
      return
    event.preventDefault()
    stopResize?.()
    const startX = event.clientX
    const startWidth = header.getBoundingClientRect().width
    const cursor = document.documentElement.style.cursor
    const userSelect = document.documentElement.style.userSelect
    const controller = new AbortController()
    draft.value = { key, width: startWidth }
    document.documentElement.style.cursor = 'col-resize'
    document.documentElement.style.userSelect = 'none'

    function finish(commit: boolean) {
      if (commit && draft.value)
        saveWidth(key, draft.value.width)
      controller.abort()
      draft.value = null
      document.documentElement.style.cursor = cursor
      document.documentElement.style.userSelect = userSelect
      stopResize = undefined
    }

    stopResize = () => finish(false)
    window.addEventListener('pointermove', (move) => {
      if (move.pointerId === event.pointerId) {
        draft.value = {
          key,
          width: Math.max(minimumWidth(key), startWidth + move.clientX - startX),
        }
      }
    }, { signal: controller.signal })
    window.addEventListener('pointerup', (end) => {
      if (end.pointerId === event.pointerId)
        finish(true)
    }, { signal: controller.signal })
    window.addEventListener('pointercancel', () => finish(false), { signal: controller.signal })
    window.addEventListener('blur', () => finish(false), { signal: controller.signal })
  }

  function resizeWithKeyboard(event: KeyboardEvent, key: string) {
    const column = columns.value.find(column => column.key === key)
    if (!column || !['ArrowLeft', 'ArrowRight', 'Home'].includes(event.key))
      return
    event.preventDefault()
    saveWidth(key, event.key === 'Home'
      ? minimumWidth(key)
      : column.width + (event.key === 'ArrowLeft' ? -8 : 8))
  }

  function resetColumnWidth(key: string) {
    const next = { ...widths.value }
    delete next[key]
    widths.value = next
  }

  function resetColumnWidths() {
    stopResize?.()
    widths.value = {}
  }

  onScopeDispose(() => stopResize?.())

  return { columns, minimumWidth, startResize, resizeWithKeyboard, resetColumnWidth, resetColumnWidths }
}
