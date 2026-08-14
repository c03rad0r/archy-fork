<script setup lang="ts">
import InterfaceModeSection from '@/views/settings/InterfaceModeSection.vue'
import KioskDisplaySection from '@/views/settings/KioskDisplaySection.vue'
import ClaudeAuthSection from '@/views/settings/ClaudeAuthSection.vue'
import AIDataAccessSection from '@/views/settings/AIDataAccessSection.vue'
import RoutstrBudgetSection from '@/views/settings/RoutstrBudgetSection.vue'
import WebhookSection from '@/views/settings/WebhookSection.vue'
import TelemetrySection from '@/views/settings/TelemetrySection.vue'
import NodeCertificateSection from '@/views/settings/NodeCertificateSection.vue'
import LightningCredentialsSection from '@/views/settings/LightningCredentialsSection.vue'
import BackupSection from '@/views/settings/BackupSection.vue'
import SystemDangerZone from '@/views/settings/SystemDangerZone.vue'
</script>

<template>
  <InterfaceModeSection />
  <KioskDisplaySection />
  <ClaudeAuthSection />
  <RoutstrBudgetSection />
  <AIDataAccessSection />
  <WebhookSection />
  <TelemetrySection />
  <NodeCertificateSection />
  <LightningCredentialsSection />
  <BackupSection />
  <SystemDangerZone />
</template>
