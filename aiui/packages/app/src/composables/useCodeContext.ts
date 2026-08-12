import { ref, computed, shallowRef, readonly } from 'vue'
import { apiFetch } from '@/utils/api-fetch'

export interface ProjectInfo {
  name: string
  path: string
  isGit: boolean
  language?: string
}

export interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  children?: FileEntry[]
}

// Module-level singleton state
const codeMode = ref(false)
const activeProject = ref<ProjectInfo | null>(null)
const projectList = shallowRef<ProjectInfo[]>([])
const fileTree = shallowRef<FileEntry[]>([])
const activeFile = ref<string | null>(null)
const activeFileContent = ref<string>('')
const activeFileLanguage = ref<string>('plaintext')
const selectedDesignTokens = ref<string[]>([])
const selectedFiles = ref<string[]>([])
const fileLoading = ref(false)
const fileError = ref('')

// Demo projects path
const PROJECTS_ROOT = '/Users/dorian/Projects'

function detectLanguage(filename: string): string {
  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript',
    vue: 'vue', svelte: 'svelte', py: 'python', rs: 'rust', go: 'go',
    java: 'java', kt: 'kotlin', swift: 'swift', rb: 'ruby', php: 'php',
    css: 'css', scss: 'scss', html: 'html', json: 'json', yaml: 'yaml',
    yml: 'yaml', md: 'markdown', toml: 'toml', sh: 'shell', bash: 'shell',
    sql: 'sql', graphql: 'graphql', dockerfile: 'dockerfile',
    c: 'c', cpp: 'cpp', h: 'c', hpp: 'cpp', cs: 'csharp',
  }
  return map[ext] ?? 'plaintext'
}

function detectProjectLanguage(files: string[]): string {
  if (files.includes('package.json')) return 'TypeScript/JavaScript'
  if (files.includes('Cargo.toml')) return 'Rust'
  if (files.includes('go.mod')) return 'Go'
  if (files.includes('requirements.txt') || files.includes('setup.py') || files.includes('pyproject.toml')) return 'Python'
  if (files.includes('pom.xml') || files.includes('build.gradle')) return 'Java'
  if (files.includes('Package.swift')) return 'Swift'
  if (files.includes('Gemfile')) return 'Ruby'
  if (files.includes('composer.json')) return 'PHP'
  if (files.some(f => f.endsWith('.csproj') || f.endsWith('.sln'))) return 'C#'
  return 'Unknown'
}

export function useCodeContext() {
  const isCodeMode = computed(() => codeMode.value)
  const hasActiveProject = computed(() => activeProject.value !== null)

  async function loadProjects(): Promise<void> {
    // In dev/demo mode, scan the Projects folder
    // This would be replaced by Archy integration later
    try {
      const response = await apiFetch(`/api/fs/list?path=${encodeURIComponent(PROJECTS_ROOT)}`)
      if (response.ok) {
        const data = await response.json()
        projectList.value = data.projects ?? []
      }
    } catch {
      // Fallback: use hardcoded list from build time
      // In real app, this would come from local filesystem or Archy nodes
      projectList.value = getDemoProjects()
    }
  }

  function getDemoProjects(): ProjectInfo[] {
    // Generic demo projects for prod/Archy deployment
    return [
      { name: 'my-lightning-app', path: '/projects/my-lightning-app', isGit: true, language: 'TypeScript/JavaScript' },
      { name: 'node-dashboard', path: '/projects/node-dashboard', isGit: true, language: 'TypeScript/JavaScript' },
      { name: 'btc-price-tracker', path: '/projects/btc-price-tracker', isGit: true, language: 'Python' },
      { name: 'nostr-relay-config', path: '/projects/nostr-relay-config', isGit: true, language: 'Rust' },
      { name: 'channel-monitor', path: '/projects/channel-monitor', isGit: true, language: 'Go' },
      { name: 'backup-scripts', path: '/projects/backup-scripts', isGit: false, language: 'Shell' },
    ]
  }

  function enterCodeMode(): void {
    codeMode.value = true
    loadProjects()
  }

  function exitCodeMode(): void {
    codeMode.value = false
    activeProject.value = null
    activeFile.value = null
    activeFileContent.value = ''
    fileTree.value = []
    selectedDesignTokens.value = []
    selectedFiles.value = []
  }

  function toggleDesignToken(id: string): void {
    const idx = selectedDesignTokens.value.indexOf(id)
    if (idx >= 0) selectedDesignTokens.value.splice(idx, 1)
    else selectedDesignTokens.value.push(id)
  }

  function isDesignTokenSelected(id: string): boolean {
    return selectedDesignTokens.value.includes(id)
  }

  function clearDesignTokens(): void {
    selectedDesignTokens.value = []
  }

  function toggleFileSelection(path: string): void {
    const idx = selectedFiles.value.indexOf(path)
    if (idx >= 0) selectedFiles.value.splice(idx, 1)
    else selectedFiles.value.push(path)
  }

  function isFileSelected(path: string): boolean {
    return selectedFiles.value.includes(path)
  }

  function clearFileSelection(): void {
    selectedFiles.value = []
  }

  function selectProject(project: ProjectInfo): void {
    activeProject.value = project
    loadFileTree(project.path)
  }

  async function loadFileTree(projectPath: string): Promise<void> {
    try {
      const response = await apiFetch(`/api/fs/tree?path=${encodeURIComponent(projectPath)}`)
      if (response.ok) {
        const data = await response.json()
        fileTree.value = data.files ?? []
      }
    } catch {
      // Demo fallback: generate a simple tree
      fileTree.value = getDemoFileTree()
    }
  }

  function getDemoFileTree(): FileEntry[] {
    // Generic project structure for demo
    return [
      { name: 'src', path: 'src', isDirectory: true, children: [
        { name: 'index.ts', path: 'src/index.ts', isDirectory: false },
        { name: 'app.ts', path: 'src/app.ts', isDirectory: false },
        { name: 'utils.ts', path: 'src/utils.ts', isDirectory: false },
      ]},
      { name: 'package.json', path: 'package.json', isDirectory: false },
      { name: 'tsconfig.json', path: 'tsconfig.json', isDirectory: false },
      { name: 'README.md', path: 'README.md', isDirectory: false },
    ]
  }

  async function openFile(filePath: string): Promise<void> {
    activeFile.value = filePath
    activeFileLanguage.value = detectLanguage(filePath)
    fileLoading.value = true
    fileError.value = ''

    try {
      const fullPath = activeProject.value
        ? `${activeProject.value.path}/${filePath}`
        : filePath
      const response = await apiFetch(`/api/fs/read?path=${encodeURIComponent(fullPath)}`)
      if (response.status === 413) {
        fileError.value = 'File too large to preview (max 1MB)'
        activeFileContent.value = ''
        return
      }
      if (response.ok) {
        const data = await response.json()
        activeFileContent.value = data.content ?? ''
      }
    } catch {
      // Demo fallback
      activeFileContent.value = getDemoFileContent(filePath)
    } finally {
      fileLoading.value = false
    }
  }

  function getDemoFileContent(filePath: string): string {
    const name = filePath.split('/').pop() ?? filePath
    if (name === 'package.json') {
      return JSON.stringify({
        name: activeProject.value?.name?.toLowerCase() ?? 'project',
        version: '1.0.0',
        type: 'module',
        scripts: { dev: 'vite', build: 'vite build', test: 'vitest' },
        dependencies: {},
      }, null, 2)
    }
    if (name === 'README.md') {
      return `# ${activeProject.value?.name ?? 'Project'}\n\nA project in the AIUI ecosystem.\n`
    }
    if (name.endsWith('.ts') || name.endsWith('.js')) {
      return `// ${name}\n// ${activeProject.value?.name ?? 'Project'}\n\nexport function main() {\n  console.log('Hello from ${name}')\n}\n`
    }
    return `// ${name}\n`
  }

  async function createProject(name: string): Promise<void> {
    const safeName = name.trim().replace(/[^a-zA-Z0-9_\-. ]/g, '')
    if (!safeName) return

    const projectPath = `${PROJECTS_ROOT}/${safeName}`

    try {
      const res = await apiFetch('/api/fs/mkdir', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: projectPath }),
      })
      if (!res.ok) {
        const data = await res.json().catch(() => ({}))
        console.warn('[AIUI code] Failed to create directory:', data.error ?? res.status)
      }
    } catch (err) {
      console.warn('[AIUI code] Could not create directory:', err)
    }

    const newProject: ProjectInfo = {
      name: safeName,
      path: projectPath,
      isGit: false,
      language: 'Unknown',
    }

    projectList.value = [newProject, ...projectList.value]
    selectProject(newProject)
  }

  function clearActiveFile(): void {
    activeFile.value = null
    activeFileContent.value = ''
    activeFileLanguage.value = 'plaintext'
  }

  return {
    // State
    codeMode,
    isCodeMode,
    activeProject,
    hasActiveProject,
    projectList,
    fileTree,
    activeFile,
    activeFileContent,
    activeFileLanguage,
    selectedDesignTokens,
    selectedFiles,
    fileLoading: readonly(fileLoading),
    fileError: readonly(fileError),

    // Actions
    enterCodeMode,
    exitCodeMode,
    selectProject,
    openFile,
    loadProjects,
    detectLanguage,
    createProject,
    clearActiveFile,
    toggleDesignToken,
    isDesignTokenSelected,
    clearDesignTokens,
    toggleFileSelection,
    isFileSelected,
    clearFileSelection,
  }
}
