import { t } from '@/i18n'
import { numberLocale } from '@/utils/format'

export function money(value: string | number | null, currency = 'USD') {
  if (value === null)
    return t('usage.metrics.notPriced')
  const amount = Number(value)
  if (!Number.isFinite(amount))
    return '—'
  return new Intl.NumberFormat(numberLocale(), {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 6,
  }).format(amount)
}
