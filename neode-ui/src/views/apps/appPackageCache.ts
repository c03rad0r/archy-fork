import { computed, ref, watch, type Ref } from 'vue'
import type { PackageDataEntry } from '@/types/api'

export type PackageMap = Record<string, PackageDataEntry>

export function useLastKnownPackages(
  livePackages: Ref<PackageMap>,
  containersScanned: Ref<boolean>,
) {
  const lastKnownPackages = ref<PackageMap>({})

  watch(
    livePackages,
    (packages) => {
      const hasPackages = Object.keys(packages).length > 0
      if (hasPackages || containersScanned.value) {
        lastKnownPackages.value = { ...packages }
      }
    },
    { immediate: true, deep: true },
  )

  const isUsingLastKnownPackages = computed(() => (
    !containersScanned.value &&
    Object.keys(livePackages.value).length === 0 &&
    Object.keys(lastKnownPackages.value).length > 0
  ))

  const packages = computed<PackageMap>(() => (
    isUsingLastKnownPackages.value ? lastKnownPackages.value : livePackages.value
  ))

  return {
    packages,
    isUsingLastKnownPackages,
    lastKnownPackages,
  }
}
