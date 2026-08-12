// Utility to fetch app information from GitHub repositories
// Used to get icons, descriptions, and other metadata for dummy apps

export interface GitHubAppInfo {
  icon?: string
  description?: string
  readme?: string
  homepage?: string
}

/**
 * Fetch app information from GitHub repository
 * @param repoUrl GitHub repository URL (e.g., https://github.com/start9labs/bitcoin)
 * @param appId App ID to help find the correct repository
 */
export async function fetchGitHubAppInfo(repoUrl: string, appId: string): Promise<GitHubAppInfo> {
  try {
    // Extract owner and repo from URL
    const match = repoUrl.match(/github\.com\/([^\/]+)\/([^\/]+)/)
    if (!match) {
      if (import.meta.env.DEV) console.warn(`[GitHub] Invalid repo URL: ${repoUrl}`)
      return {}
    }
    
    const [, owner, repo] = match
    
    // Try to find Start9 wrapper repo first (e.g., bitcoin-startos)
    const start9RepoName = `${appId}-startos`
    let targetOwner = owner
    let targetRepo = repo
    
    // If the repo URL doesn't match the expected pattern, try Start9Labs
    if (repo && !repo.includes('startos') && !repo.includes('start9')) {
      // Try Start9Labs wrapper repo
      try {
        const start9RepoUrl = `https://api.github.com/repos/Start9Labs/${start9RepoName}`
        const start9Response = await fetch(start9RepoUrl)
        if (start9Response.ok) {
          targetOwner = 'Start9Labs'
          targetRepo = start9RepoName
        }
      } catch (e) {
        if (import.meta.env.DEV) console.warn('Start9 repo lookup failed, falling back to original repo', e)
      }
    }
    
    // Fetch repository info
    const repoApiUrl = `https://api.github.com/repos/${targetOwner}/${targetRepo}`
    const repoResponse = await fetch(repoApiUrl)
    
    if (!repoResponse.ok) {
      if (import.meta.env.DEV) console.warn(`[GitHub] Failed to fetch repo ${targetOwner}/${targetRepo}: ${repoResponse.status}`)
      return {}
    }
    
    const repoData = await repoResponse.json()
    
    // Fetch README
    let readme = ''
    try {
      const readmeResponse = await fetch(`https://api.github.com/repos/${targetOwner}/${targetRepo}/readme`)
      if (readmeResponse.ok) {
        const readmeData = await readmeResponse.json()
        readme = atob(readmeData.content) // Base64 decode
      }
    } catch (e) {
      if (import.meta.env.DEV) console.warn(`[GitHub] Failed to fetch README for ${targetOwner}/${targetRepo}`)
    }
    
    // Try to find icon in repository
    // Common locations: icon.png, icon.svg, assets/icon.png, etc.
    let icon: string | undefined
    const iconPaths = [
      'icon.png',
      'icon.svg',
      'assets/icon.png',
      'assets/icon.svg',
      'icon/icon.png',
      'icon/icon.svg'
    ]
    
    for (const iconPath of iconPaths) {
      try {
        const iconResponse = await fetch(`https://api.github.com/repos/${targetOwner}/${targetRepo}/contents/${iconPath}`)
        if (iconResponse.ok) {
          const iconData = await iconResponse.json()
          if (iconData.download_url) {
            icon = iconData.download_url
            break
          }
        }
      } catch (e) {
        if (import.meta.env.DEV) console.warn('Icon path lookup failed, trying next path', e)
      }
    }
    
    // If no icon found, try to get from releases/assets
    if (!icon) {
      try {
        const releasesResponse = await fetch(`https://api.github.com/repos/${targetOwner}/${targetRepo}/releases/latest`)
        if (releasesResponse.ok) {
          const releasesData = await releasesResponse.json()
          const asset = releasesData.assets?.find((a: { name: string; browser_download_url: string }) => 
            a.name.includes('icon') || a.name.includes('logo')
          )
          if (asset) {
            icon = asset.browser_download_url
          }
        }
      } catch (e) {
        if (import.meta.env.DEV) console.warn('No icon from releases', e)
      }
    }
    
    // If still no icon, try raw GitHub content URLs for common icon names
    if (!icon) {
      const rawIconPaths = [
        `https://raw.githubusercontent.com/${targetOwner}/${targetRepo}/main/icon.png`,
        `https://raw.githubusercontent.com/${targetOwner}/${targetRepo}/main/icon.svg`,
        `https://raw.githubusercontent.com/${targetOwner}/${targetRepo}/master/icon.png`,
        `https://raw.githubusercontent.com/${targetOwner}/${targetRepo}/master/icon.svg`,
        `https://raw.githubusercontent.com/${targetOwner}/${targetRepo}/main/assets/icon.png`,
        `https://raw.githubusercontent.com/${targetOwner}/${targetRepo}/main/assets/icon.svg`,
      ]
      
      // Test each URL
      for (const iconUrl of rawIconPaths) {
        try {
          const testResponse = await fetch(iconUrl, { method: 'HEAD' })
          if (testResponse.ok) {
            icon = iconUrl
            break
          }
        } catch (e) {
          if (import.meta.env.DEV) console.warn('Raw icon URL failed, trying next URL', e)
        }
      }
    }
    
    return {
      icon,
      description: repoData.description || '',
      readme,
      homepage: repoData.homepage || repoData.html_url
    }
  } catch (error) {
    if (import.meta.env.DEV) console.error(`[GitHub] Error fetching app info for ${repoUrl}:`, error)
    return {}
  }
}

/**
 * Batch fetch app info for multiple apps
 */
export async function fetchMultipleAppInfo(
  apps: Array<{ id: string; 'wrapper-repo': string }>
): Promise<Record<string, GitHubAppInfo>> {
  const results: Record<string, GitHubAppInfo> = {}
  
  // Fetch in parallel with rate limiting (max 5 concurrent)
  const batchSize = 5
  for (let i = 0; i < apps.length; i += batchSize) {
    const batch = apps.slice(i, i + batchSize)
    const batchPromises = batch.map(async (app) => {
      const info = await fetchGitHubAppInfo(app['wrapper-repo'], app.id)
      return { id: app.id, info }
    })
    
    const batchResults = await Promise.all(batchPromises)
    batchResults.forEach(({ id, info }) => {
      results[id] = info
    })
    
    // Rate limit: wait 1 second between batches
    if (i + batchSize < apps.length) {
      await new Promise(resolve => setTimeout(resolve, 1000))
    }
  }
  
  return results
}

