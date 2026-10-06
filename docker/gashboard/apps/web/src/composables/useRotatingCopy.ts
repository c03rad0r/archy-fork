import { computed, onMounted, onUnmounted, ref } from "vue";

export function useRotatingCopy(lines: readonly string[], intervalMs = 10_000) {
  const index = ref(0);
  let timer: number | null = null;

  onMounted(() => {
    timer = window.setInterval(() => {
      index.value = (index.value + 1) % lines.length;
    }, intervalMs);
  });

  onUnmounted(() => {
    if (timer !== null) window.clearInterval(timer);
  });

  return computed(() => lines[index.value] ?? lines[0] ?? "");
}
