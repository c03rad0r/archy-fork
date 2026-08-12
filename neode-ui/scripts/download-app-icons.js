#!/usr/bin/env node

/**
 * Script to download app icons from GitHub repositories
 * Downloads icons for all dummy apps from Start9Labs/{app-id}-startos repos
 */

import fs from 'fs'
import path from 'path'
import https from 'https'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

const appIds = [
  'bitcoin',
  'btcpay-server',
  'homeassistant',
  'grafana',
  'endurain',
  'fedimint',
  'morphos-server',
  'lightning-stack',
  'mempool',
  'ollama',
  'searxng',
  'penpot'
]

// Map app IDs to their Start9 repo names (some might differ)
const repoMap = {
  'bitcoin': 'bitcoind-startos',
  'btcpay-server': 'btcpayserver-startos',
  'homeassistant': 'home-assistant-startos',
  'grafana': 'grafana-startos',
  'lightning-stack': 'lnd-startos',
  'mempool': 'mempool-startos',
  'searxng': 'searxng-startos',
  'penpot': 'penpot-startos',
}

// Custom icon URLs for apps without Start9 repos
const customIconUrls = {
  'fedimint': [
    'https://raw.githubusercontent.com/fedibtc/fedimint-ui/master/apps/router/public/favicon.svg',
  ],
  // Official bark project avatar (Ark protocol wallet daemon)
  'bark': [
    'https://gitlab.com/uploads/-/system/project/avatar/75519706/bark-smiling-square-white-2.jpg',
  ],
}

const iconDir = path.join(__dirname, '../public/assets/img/app-icons')

// Ensure directory exists
if (!fs.existsSync(iconDir)) {
  fs.mkdirSync(iconDir, { recursive: true })
}

function downloadFile(url, filepath) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(filepath)
    
    https.get(url, (response) => {
      if (response.statusCode === 200) {
        response.pipe(file)
        file.on('finish', () => {
          file.close()
          console.log(`✅ Downloaded: ${path.basename(filepath)}`)
          resolve()
        })
      } else if (response.statusCode === 404) {
        file.close()
        fs.unlinkSync(filepath) // Delete empty file
        console.log(`⚠️  Not found: ${url}`)
        reject(new Error(`404: ${url}`))
      } else {
        file.close()
        fs.unlinkSync(filepath)
        reject(new Error(`HTTP ${response.statusCode}: ${url}`))
      }
    }).on('error', (err) => {
      file.close()
      if (fs.existsSync(filepath)) {
        fs.unlinkSync(filepath)
      }
      reject(err)
    })
  })
}

async function downloadIcon(appId) {
  const targetExt = 'webp' // Prefer webp for consistency with mempool, etc.
  const fallbackExts = ['webp', 'png', 'svg']
  const filepath = path.join(iconDir, `${appId}.webp`)

  // Skip if file already exists
  if (appId === 'fedimint' && fs.existsSync(path.join(iconDir, 'fedimint.png'))) {
    console.log(`⏭️  Skipping ${appId} (fedimint.png exists)`)
    return true
  }
  for (const ext of fallbackExts) {
    const fp = path.join(iconDir, `${appId}.${ext}`)
    if (fs.existsSync(fp)) {
      console.log(`⏭️  Skipping ${appId} (already exists)`)
      return true
    }
  }

  // Try custom URLs first (e.g. fedimint from fedimint-ui)
  if (customIconUrls[appId]) {
    for (const url of customIconUrls[appId]) {
      try {
        const ext = url.endsWith('.svg') ? 'svg' : (url.endsWith('.png') ? 'png' : 'webp')
        const fp = path.join(iconDir, `${appId}.${ext}`)
        await downloadFile(url, fp)
        return true
      } catch (err) {
        continue
      }
    }
  }

  const repoName = repoMap[appId] || `${appId}-startos`
  const iconPaths = ['icon.png', 'icon.svg', 'assets/icon.png', 'assets/icon.svg']

  for (const iconPath of iconPaths) {
    const url = `https://raw.githubusercontent.com/Start9Labs/${repoName}/main/${iconPath}`
    const extension = iconPath.endsWith('.svg') ? 'svg' : 'png'
    const fp = path.join(iconDir, `${appId}.${extension}`)
    try {
      await downloadFile(url, fp)
      return true
    } catch (err) {
      continue
    }
  }

  console.log(`❌ Failed to download icon for ${appId}`)
  return false
}

async function main() {
  console.log('Downloading app icons from GitHub...\n')
  
  const results = {
    success: [],
    failed: []
  }
  
  for (const appId of appIds) {
    try {
      const success = await downloadIcon(appId)
      if (success) {
        results.success.push(appId)
      } else {
        results.failed.push(appId)
      }
      // Small delay to avoid rate limiting
      await new Promise(resolve => setTimeout(resolve, 500))
    } catch (err) {
      console.error(`Error downloading ${appId}:`, err.message)
      results.failed.push(appId)
    }
  }
  
  console.log(`\n✅ Successfully downloaded ${results.success.length} icons`)
  if (results.failed.length > 0) {
    console.log(`❌ Failed to download ${results.failed.length} icons:`, results.failed.join(', '))
  }
}

main().catch(console.error)
