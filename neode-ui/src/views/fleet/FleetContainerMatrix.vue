<template>
  <div class="glass-card p-5">
    <h3 class="text-sm font-medium text-white/80 mb-4">Container Matrix</h3>

    <div v-if="!nodes.length" class="text-white/40 text-sm py-4 text-center">
      No nodes to display.
    </div>
    <div v-else class="overflow-x-auto">
      <table class="fleet-matrix-table">
        <thead>
          <tr>
            <th class="fleet-matrix-header-cell">App</th>
            <th
              v-for="node in sortedNodes"
              :key="node.node_id"
              class="fleet-matrix-header-cell"
              :title="fleetNodeSubtitle(node)"
            >
              {{ fleetNodeDisplayName(node) }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="app in allAppIds" :key="app">
            <td class="fleet-matrix-cell text-white/70">{{ app }}</td>
            <td
              v-for="node in sortedNodes"
              :key="node.node_id"
              class="fleet-matrix-cell text-center"
            >
              <span v-if="getContainerState(node, app) === 'running'" class="text-green-400">&#10003;</span>
              <span v-else-if="getContainerState(node, app) === 'stopped'" class="text-red-400">&#10007;</span>
              <span v-else class="text-white/20">&mdash;</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { type FleetNode, getContainerState, fleetNodeDisplayName, fleetNodeSubtitle } from './useFleetData'

defineProps<{
  nodes: FleetNode[]
  sortedNodes: FleetNode[]
  allAppIds: string[]
}>()
</script>
