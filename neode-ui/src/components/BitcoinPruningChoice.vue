<template>
    <div class="mt-5 space-y-2">
      <label class="flex items-center gap-2 text-sm text-white/80">
        <input v-model="model" type="checkbox" class="accent-orange-400" />
        Prune Bitcoin to save disk space
      </label>
      <p class="text-xs text-white/50">
        Keeps about 50 GB of recent blocks, using the same settings as automatic
        pruning on smaller disks. All blocks are still downloaded and verified.
        Mempool and other apps that need the full blockchain won’t be available.
        Turning pruning off later requires downloading the blockchain again.
      </p>
      <p v-if="!model" class="text-xs text-white/50">
        Automatic pruning still applies on disks smaller than 1 TB.
      </p>
    </div>

</template>
<script setup lang="ts">
const model = defineModel<boolean>({ default: false })
</script>
