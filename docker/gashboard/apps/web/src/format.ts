export function compactNumber(n: number, digits = 1): string {
  if (!Number.isFinite(n)) return "-";
  if (Math.abs(n) >= 1e12) return `${(n / 1e12).toFixed(digits)}T`;
  if (Math.abs(n) >= 1e9) return `${(n / 1e9).toFixed(digits)}B`;
  if (Math.abs(n) >= 1e6) return `${(n / 1e6).toFixed(digits)}M`;
  if (Math.abs(n) >= 1e3) return `${(n / 1e3).toFixed(digits)}K`;
  return n.toFixed(digits);
}

export function oneIn(probability: number): string {
  if (!Number.isFinite(probability) || probability <= 0) return "basically never";
  return `1 in ${compactNumber(1 / probability, 1)}`;
}

export function tinyPercentAsOdds(percent: number): string {
  if (!Number.isFinite(percent) || percent <= 0) return "0%";
  const probability = percent / 100;
  if (percent < 0.001) return oneIn(probability);
  if (percent < 1) return `${percent.toFixed(4)}%`;
  return `${percent.toFixed(2)}%`;
}

export function multiple(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "-";
  return `${compactNumber(n, 1)}x`;
}
