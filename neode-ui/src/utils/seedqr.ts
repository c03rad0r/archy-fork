/**
 * SeedQR encoding (SeedSigner standard, supported by Passport/Passport Prime,
 * SeedSigner, Keystone, Nunchuk, Sparrow, …): each BIP39 word becomes its
 * zero-padded 4-digit wordlist index (0000–2047), concatenated into one
 * digit stream and rendered as a numeric-mode QR. A 24-word seed is 96
 * digits. Spec: github.com/SeedSigner/seedsigner/blob/main/docs/seed_qr
 *
 * Only valid for real BIP39 mnemonics — LND's aezeed shares the wordlist but
 * is NOT BIP39, and hardware wallets cannot import it; never SeedQR-encode it.
 */
export async function toSeedQrDigits(words: string[]): Promise<string | null> {
  if (words.length === 0) return null
  const { wordlist } = await import('@scure/bip39/wordlists/english.js')
  const digits: string[] = []
  for (const raw of words) {
    const idx = wordlist.indexOf(raw.trim().toLowerCase())
    if (idx < 0) return null // not a BIP39 word — caller falls back to text
    digits.push(idx.toString().padStart(4, '0'))
  }
  return digits.join('')
}
