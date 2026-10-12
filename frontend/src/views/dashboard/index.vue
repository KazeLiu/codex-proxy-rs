<script setup lang="ts">
import DashboardContent from './components/DashboardContent.vue'
import { useDashboard } from './composables/useDashboard'

const {
  loading,
  refreshing,
  activeTrendKind,
  metrics,
  analytics,
  analyticsError,
  trendPoints,
  trendSummary,
  trendLoading,
  trendError,
  healthTimeline,
  accountUsage,
  wireProfiles,
  usageRecords,
  showFailedRecords,
  usageRecordsLoading,
  usageRecordsError,
  poolSummary,
  capacityInfo,
  rotationStrategy,
  lastRefreshedAt,
  refresh,
  loadTrend,
} = useDashboard()
</script>

<template>
  <DashboardContent
    v-model:trend-kind="activeTrendKind"
    v-model:show-failed-records="showFailedRecords"
    :loading="loading"
    :refreshing="refreshing"
    :last-refreshed-at="lastRefreshedAt"
    :metrics="metrics"
    :analytics="analytics"
    :analytics-error="analyticsError"
    :trend-points="trendPoints"
    :trend-summary="trendSummary"
    :trend-loading="trendLoading"
    :trend-error="trendError"
    :health-timeline="healthTimeline"
    :account-usage="accountUsage"
    :wire-profiles="wireProfiles"
    :usage-records="usageRecords"
    :usage-records-loading="usageRecordsLoading"
    :usage-records-error="usageRecordsError"
    :pool-summary="poolSummary"
    :capacity-info="capacityInfo"
    :rotation-strategy="rotationStrategy"
    @refresh="refresh"
    @trend-change="loadTrend"
  />
</template>
