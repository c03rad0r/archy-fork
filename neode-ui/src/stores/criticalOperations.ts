import { ref } from 'vue'

/// True while a router flash or TollGate provision is running. A detected LoRa
/// radio may not hijack that flow with a global modal or a route change.
export const routerWorkInProgress = ref(false)
