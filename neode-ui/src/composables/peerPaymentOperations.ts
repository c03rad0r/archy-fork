import { computed, reactive } from 'vue'

export type PaymentSelection = { peer: string; item: string; generation: number } | null
export type PaymentOperation = { peer: string; item: string; generation: number; kind: string; token: symbol }

/** In-flight UI ownership complements durable node/browser attempt journals.
 * An old callback may persist its own receipt, but cannot own a new modal. */
export function usePeerPaymentOperations(selection: () => PaymentSelection) {
  const slots = reactive(new Map<string, PaymentOperation>())
  const key = (peer: string, item: string) => JSON.stringify([peer, item])
  function begin(kind: string, peer: string, item: string): PaymentOperation | null {
    const index = key(peer, item)
    if (slots.has(index)) return null
    const current = selection()
    if (!current || current.peer !== peer || current.item !== item) return null
    const operation = { peer, item, generation: current.generation, kind, token: Symbol(kind) }
    slots.set(index, operation)
    return operation
  }
  function selected(operation: PaymentOperation): boolean {
    const current = selection()
    return slots.get(key(operation.peer, operation.item))?.token === operation.token
      && current?.peer === operation.peer && current?.item === operation.item && current?.generation === operation.generation
  }
  function finish(operation: PaymentOperation): boolean {
    const index = key(operation.peer, operation.item)
    // Never reset a newer operation's busy flag, even for the same file.
    const owned = slots.get(index)?.token === operation.token
    const wasSelected = owned && selected(operation)
    if (owned) slots.delete(index)
    return wasSelected
  }
  const current = computed(() => {
    const value = selection()
    return value ? slots.get(key(value.peer, value.item)) : undefined
  })
  return { begin, selected, finish, current, busy: computed(() => current.value !== undefined) }
}
