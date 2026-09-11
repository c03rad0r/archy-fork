import { ref } from "vue";

export interface CompanionRelease {
  versionName: string;
  versionCode: number;
}

// One shared value keeps the Discover banner and the automatic/manual
// Companion prompt on exactly the same release metadata.
export const companionRelease = ref<CompanionRelease | null>(null);

let releaseRequest: Promise<CompanionRelease | null> | null = null;

export function loadCompanionRelease(): Promise<CompanionRelease | null> {
  if (companionRelease.value) return Promise.resolve(companionRelease.value);
  if (releaseRequest) return releaseRequest;

  releaseRequest = fetch("/packages/archipelago-companion.json", {
    cache: "no-store",
  })
    .then(async (response) => {
      if (!response.ok) return null;
      const metadata = await response.json();
      if (
        !metadata ||
        typeof metadata.versionName !== "string" ||
        !metadata.versionName
      )
        return null;

      companionRelease.value = {
        versionName: metadata.versionName,
        versionCode: Number(metadata.versionCode) || 0,
      };
      return companionRelease.value;
    })
    .catch(() => null)
    .finally(() => {
      releaseRequest = null;
    });

  return releaseRequest;
}

export function companionReleaseMarker(
  release: CompanionRelease | null,
): string {
  if (release?.versionCode) return `build:${release.versionCode}`;
  if (release?.versionName) return `version:${release.versionName}`;
  return "1";
}
