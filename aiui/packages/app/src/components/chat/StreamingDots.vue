<template>
  <div class="flex justify-start animate-fade-up-fast">
    <div class="path-glass-card rounded-2xl rounded-bl-md px-4 py-3">
      <div class="flex items-center gap-1.5">
        <span
          v-for="i in 3"
          :key="i"
          class="w-1.5 h-1.5 rounded-full bg-white/40 animate-pulse-glow"
          :style="{ animationDelay: `${i * 200}ms` }"
        />
      </div>
    </div>
  </div>
</template>
