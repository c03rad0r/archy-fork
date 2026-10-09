<template>
  <main class="archy-shell" aria-labelledby="page-title">
    <header class="archy-header">
      <div>
        <p class="archy-eyebrow">ARCHIPELAGO APP</p>
        <h1 id="page-title">Project workspace</h1>
        <p class="archy-muted">A small, useful starting point for a node app.</p>
      </div>
      <button class="archy-button archy-button-secondary" type="button" @click="reload">
        Refresh
      </button>
    </header>

    <section class="archy-card" aria-labelledby="items-title">
      <div class="archy-section-heading">
        <div>
          <p class="archy-eyebrow">YOUR DATA</p>
          <h2 id="items-title">Items</h2>
        </div>
        <button class="archy-button" type="button" @click="addItem">Add item</button>
      </div>

      <div v-if="status === 'loading'" class="archy-state" role="status">Loading items…</div>
      <div v-else-if="status === 'error'" class="archy-state archy-state-error" role="alert">
        <strong>Items are unavailable.</strong>
        <span>Check the app service and try again.</span>
        <button class="archy-button archy-button-secondary" type="button" @click="reload">Try again</button>
      </div>
      <div v-else-if="items.length === 0" class="archy-state">
        <strong>No items yet.</strong>
        <span>Create one to see the app's normal success state.</span>
        <button class="archy-button archy-button-secondary" type="button" @click="addItem">Create first item</button>
      </div>
      <ul v-else class="archy-list" aria-live="polite">
        <li v-for="item in items" :key="item.id" class="archy-list-row">
          <div>
            <strong>{{ item.name }}</strong>
            <span class="archy-muted">{{ item.detail }}</span>
          </div>
          <span class="archy-status">Ready</span>
        </li>
      </ul>
    </section>

    <p class="archy-footnote">This starter is standalone-safe and does not require a host handshake.</p>
  </main>
</template>

<script setup lang="ts">
import { ref } from 'vue'

type Item = { id: number; name: string; detail: string }
const status = ref<'ready' | 'loading' | 'error'>('ready')
const items = ref<Item[]>([])
let nextId = 1

function addItem() {
  status.value = 'ready'
  items.value.push({ id: nextId, name: `Item ${nextId}`, detail: 'Persist this through your app API.' })
  nextId += 1
}

function reload() {
  status.value = 'loading'
  window.setTimeout(() => { status.value = 'ready' }, 250)
}
</script>
