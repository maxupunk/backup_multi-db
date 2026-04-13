import type {
  ApiResponse,
  DockerActionResult,
  DockerContainerDetail,
  DockerContainerGroup,
  DockerImageDetail,
  DockerImageSummary,
  DockerLogEntry,
  DockerLogsParams,
  DockerNetworkDetail,
  DockerNetworkSummary,
  DockerPruneResult,
  DockerVolumeDetail,
  DockerVolumeSummary,
} from '@/types/api'

const BASE = '/api/docker'

async function apiFetch<T>(url: string, options: RequestInit = {}): Promise<T> {
  const token = localStorage.getItem('token')
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    Accept: 'application/json',
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  }

  const res = await fetch(url, { ...options, headers: { ...headers, ...(options.headers as Record<string, string> | undefined) } })
  const data = (await res.json()) as ApiResponse<T> & { available?: boolean }

  if (!res.ok) {
    const msg =
      (data as unknown as { message?: string }).message ?? 'Erro na requisição Docker'
    throw new Error(msg)
  }

  if (data.available === false) {
    throw new Error('DOCKER_UNAVAILABLE')
  }

  return data.data as T
}

/** Inicia download do browser sem precisar parsear JSON */
function downloadViaAnchor(url: string, filename: string) {
  const token = localStorage.getItem('token')
  // Usa fetch para incluir o token de autenticação, depois cria object URL
  void fetch(url, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  }).then(async (res) => {
    if (!res.ok) throw new Error('Falha no download')
    const blob = await res.blob()
    const href = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = href
    a.download = filename
    document.body.appendChild(a)
    a.click()
    a.remove()
    URL.revokeObjectURL(href)
  })
}

// ============================================================
// Containers
// ============================================================

export const dockerContainersApi = {
  getGroups(): Promise<DockerContainerGroup[]> {
    return apiFetch<DockerContainerGroup[]>(`${BASE}/containers`)
  },

  getDetail(id: string): Promise<DockerContainerDetail> {
    return apiFetch<DockerContainerDetail>(`${BASE}/containers/${encodeURIComponent(id)}`)
  },

  start(id: string): Promise<DockerActionResult> {
    return apiFetch<DockerActionResult>(`${BASE}/containers/${encodeURIComponent(id)}/start`, {
      method: 'POST',
    })
  },

  stop(id: string): Promise<DockerActionResult> {
    return apiFetch<DockerActionResult>(`${BASE}/containers/${encodeURIComponent(id)}/stop`, {
      method: 'POST',
    })
  },

  restart(id: string): Promise<DockerActionResult> {
    return apiFetch<DockerActionResult>(`${BASE}/containers/${encodeURIComponent(id)}/restart`, {
      method: 'POST',
    })
  },

  remove(id: string, force = false): Promise<DockerActionResult> {
    return apiFetch<DockerActionResult>(
      `${BASE}/containers/${encodeURIComponent(id)}?force=${force}`,
      { method: 'DELETE' }
    )
  },

  getLogs(id: string, params: DockerLogsParams = {}): Promise<DockerLogEntry[]> {
    const qs = new URLSearchParams()
    if (params.tail !== undefined) qs.set('tail', String(params.tail))
    if (params.since !== undefined) qs.set('since', String(params.since))
    if (params.timestamps) qs.set('timestamps', 'true')
    const query = qs.toString() ? `?${qs.toString()}` : ''
    return apiFetch<DockerLogEntry[]>(
      `${BASE}/containers/${encodeURIComponent(id)}/logs${query}`
    )
  },
}

// ============================================================
// Volumes
// ============================================================

export const dockerVolumesApi = {
  list(): Promise<DockerVolumeSummary[]> {
    return apiFetch<DockerVolumeSummary[]>(`${BASE}/volumes`)
  },

  getDetail(name: string): Promise<DockerVolumeDetail> {
    return apiFetch<DockerVolumeDetail>(`${BASE}/volumes/${encodeURIComponent(name)}`)
  },

  remove(name: string, force = false): Promise<DockerActionResult> {
    const query = force ? '?force=true' : ''
    return apiFetch<DockerActionResult>(
      `${BASE}/volumes/${encodeURIComponent(name)}${query}`,
      { method: 'DELETE' }
    )
  },

  exportVolume(name: string): void {
    const safeName = name.replace(/[^a-zA-Z0-9_-]/g, '_')
    const date = new Date().toISOString().slice(0, 10)
    const filename = `volume-${safeName}-${date}.tar.gz`
    downloadViaAnchor(`${BASE}/volumes/${encodeURIComponent(name)}/export`, filename)
  },

  exportToStorage(name: string, storageId: number): Promise<{ fileName: string; relativePath: string }> {
    return apiFetch<{ fileName: string; relativePath: string }>(
      `${BASE}/volumes/${encodeURIComponent(name)}/backup`,
      { method: 'POST', body: JSON.stringify({ storageId }) }
    )
  },
}

// ============================================================
// Networks
// ============================================================

export const dockerNetworksApi = {
  list(): Promise<DockerNetworkSummary[]> {
    return apiFetch<DockerNetworkSummary[]>(`${BASE}/networks`)
  },

  getDetail(id: string): Promise<DockerNetworkDetail> {
    return apiFetch<DockerNetworkDetail>(`${BASE}/networks/${encodeURIComponent(id)}`)
  },

  create(name: string, driver = 'bridge'): Promise<DockerActionResult> {
    return apiFetch<DockerActionResult>(`${BASE}/networks`, {
      method: 'POST',
      body: JSON.stringify({ name, driver }),
    })
  },

  connect(networkId: string, containerId: string): Promise<DockerActionResult> {
    return apiFetch<DockerActionResult>(
      `${BASE}/networks/${encodeURIComponent(networkId)}/connect`,
      { method: 'POST', body: JSON.stringify({ containerId }) }
    )
  },

  disconnect(networkId: string, containerId: string, force = false): Promise<DockerActionResult> {
    return apiFetch<DockerActionResult>(
      `${BASE}/networks/${encodeURIComponent(networkId)}/disconnect`,
      { method: 'POST', body: JSON.stringify({ containerId, force }) }
    )
  },
}

// ============================================================
// Images
// ============================================================

export const dockerImagesApi = {
  list(): Promise<DockerImageSummary[]> {
    return apiFetch<DockerImageSummary[]>(`${BASE}/images`)
  },

  getDetail(id: string): Promise<DockerImageDetail> {
    return apiFetch<DockerImageDetail>(`${BASE}/images/${encodeURIComponent(id)}`)
  },

  remove(id: string, force = false): Promise<DockerActionResult> {
    const query = force ? '?force=true' : ''
    return apiFetch<DockerActionResult>(
      `${BASE}/images/${encodeURIComponent(id)}${query}`,
      { method: 'DELETE' }
    )
  },

  prune(): Promise<DockerPruneResult> {
    return apiFetch<DockerPruneResult>(`${BASE}/images/prune`, { method: 'POST' })
  },
}
