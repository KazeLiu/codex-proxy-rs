<script setup lang="ts">
import { ZButton, ZDialog, ZIconButton } from '@codex-proxy/ui'

import { Copy, Upload } from '@lucide/vue'
import { useI18n } from 'vue-i18n'

defineProps<{ createdKey: string }>()

const emit = defineEmits<{
  copy: [text: string]
  importCcs: []
  afterLeave: []
}>()

const { t } = useI18n({ useScope: 'global' })

const open = defineModel<boolean>({ default: false })
</script>

<template>
  <ZDialog
    v-model="open"
    :title="t('keys.created.apiKeyCreated')"
    :description="t('keys.created.description')"
    type="success"
    width="36rem"
    @closed="emit('afterLeave')"
  >
    <div class="flex flex-col gap-4">
      <div class="rounded-cp border border-cp-warning-border bg-cp-warning-container px-4 py-3">
        <p class="m-0 text-cp font-semibold text-cp-warning-on-container">
          {{ t('keys.created.accessNotice') }}
        </p>
      </div>
      <div>
        <p class="mb-2 text-cp font-medium text-cp-text-secondary">
          {{ t('common.fields.apiKey') }}
        </p>
        <div class="flex items-center gap-2">
          <code class="flex-1 rounded-cp bg-cp-fill-quaternary px-3 py-2.5 font-mono text-cp break-all text-cp-text">
            {{ createdKey }}
          </code>
          <ZIconButton size="default" :aria-label="t('common.actions.copy')" @click="emit('copy', createdKey)">
            <Copy class="size-4" />
          </ZIconButton>
        </div>
      </div>
    </div>

    <template #footer>
      <ZButton @click="emit('copy', createdKey)">
        <template #icon>
          <Copy class="size-4" />
        </template>
        {{ t('keys.created.copyKey') }}
      </ZButton>
      <ZButton @click="emit('importCcs')">
        <template #icon>
          <Upload class="size-4" />
        </template>
        {{ t('keys.configuration.importCcSwitch') }}
      </ZButton>
      <ZButton type="primary" @click="open = false">
        {{ t('keys.created.iHaveSaved') }}
      </ZButton>
    </template>
  </ZDialog>
</template>
