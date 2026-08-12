#!/usr/bin/env node
/**
 * Generate "Welcome Noderunner" speech using ElevenLabs AI voice.
 * Slower, softer, sci-fi style with reverb/echo effects.
 *
 * Usage:
 *   ELEVENLABS_API_KEY=your_key node scripts/generate-welcome-speech.js
 *
 * Optional voice ID (browse https://elevenlabs.io/voice-library/sensual):
 *   ELEVENLABS_VOICE_ID=voice_id node scripts/generate-welcome-speech.js
 */

import { writeFileSync, mkdirSync, readFileSync, unlinkSync } from 'fs'
import { dirname, join } from 'path'
import { fileURLToPath } from 'url'
import { execSync } from 'child_process'

const __dirname = dirname(fileURLToPath(import.meta.url))

const API_KEY = process.env.ELEVENLABS_API_KEY
// Sarah - mature, reassuring, confident female (softer than Rachel)
const VOICE_ID = process.env.ELEVENLABS_VOICE_ID || 'EXAVITQu4vr4xnSDxMaL'
const OUTPUT_PATH = join(__dirname, '../public/assets/audio/welcome-noderunner.mp3')
const RAW_PATH = join(__dirname, '../public/assets/audio/welcome-noderunner-raw.mp3')

if (!API_KEY) {
  console.error('Set ELEVENLABS_API_KEY (get a free key at elevenlabs.io)')
  process.exit(1)
}

// Slower (0.78), softer (higher stability 0.65), more expressive (style 0.6)
const res = await fetch(
  `https://api.elevenlabs.io/v1/text-to-speech/${VOICE_ID}?output_format=mp3_44100_128`,
  {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'xi-api-key': API_KEY,
    },
    body: JSON.stringify({
      text: 'Welcome Noderunner',
      model_id: 'eleven_multilingual_v2',
      voice_settings: {
        stability: 0.65,
        similarity_boost: 0.8,
        style: 0.6,
        use_speaker_boost: true,
        speed: 0.7,
      },
    }),
  }
)

if (!res.ok) {
  const err = await res.text()
  console.error('ElevenLabs API error:', res.status, err)
  process.exit(1)
}

const buf = Buffer.from(await res.arrayBuffer())
mkdirSync(dirname(OUTPUT_PATH), { recursive: true })
writeFileSync(RAW_PATH, buf)

// Add sci-fi reverb: dense short delays that blend (no distinct echo)
try {
  execSync(
    `ffmpeg -y -i "${RAW_PATH}" -af "aecho=0.6:0.15:25|45|70:0.55|0.45|0.35,highpass=f=80,equalizer=f=4000:t=q:w=1:g=-1" -q:a 2 "${OUTPUT_PATH}" 2>/dev/null`,
    { stdio: 'pipe' }
  )
  unlinkSync(RAW_PATH)
} catch {
  writeFileSync(OUTPUT_PATH, buf)
  try { unlinkSync(RAW_PATH) } catch {}
}

console.log('Generated:', OUTPUT_PATH)
console.log('Add this file to git and deploy.')
