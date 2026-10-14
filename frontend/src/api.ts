import {
  ApiStatusResponse,
  ProviderInput,
  VirtualModelDetail,
  AnalyticsResponse,
  TestConnectionResponse,
  LogsResponse,
  AuthStatusResponse,
  ApiKeyRecord,
  VaultSecretRecord,
  VaultSecretInput,
} from './types';

const API_BASE = '';

export async function fetchStatus(): Promise<ApiStatusResponse> {
  const res = await fetch(`${API_BASE}/api/status`);
  if (!res.ok) {
    throw new Error(`Failed to fetch status: ${res.statusText}`);
  }
  return res.json();
}

export async function createProvider(provider: ProviderInput): Promise<void> {
  const res = await fetch(`${API_BASE}/api/providers`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(provider),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || `Failed to create provider (${res.status})`);
  }
}

export async function updateProvider(name: string, provider: ProviderInput): Promise<void> {
  const res = await fetch(`${API_BASE}/api/providers/${encodeURIComponent(name)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(provider),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || `Failed to update provider (${res.status})`);
  }
}

export async function deleteProvider(name: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/providers/${encodeURIComponent(name)}`, {
    method: 'DELETE',
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || `Failed to delete provider (${res.status})`);
  }
}

export async function testProvider(name: string): Promise<TestConnectionResponse> {
  const res = await fetch(`${API_BASE}/api/providers/${encodeURIComponent(name)}/test`, {
    method: 'POST',
  });
  return res.json();
}

export async function resetProviderCircuit(name: string): Promise<void> {
  const res = await fetch(`${API_BASE}/admin/providers/${encodeURIComponent(name)}/reset`, {
    method: 'POST',
  });
  if (!res.ok) {
    throw new Error(`Failed to reset circuit for ${name}`);
  }
}

export async function fetchVirtualModels(): Promise<VirtualModelDetail[]> {
  const res = await fetch(`${API_BASE}/api/virtual-models`);
  if (!res.ok) {
    throw new Error(`Failed to fetch virtual models: ${res.statusText}`);
  }
  return res.json();
}

export async function upsertVirtualModel(
  name: string,
  model: {
    description?: string;
    weight_smartness?: number;
    weight_speed?: number;
    weight_reliability?: number;
    targets: Array<{ provider: string; model: string }>;
  }
): Promise<void> {
  const res = await fetch(`${API_BASE}/api/virtual-models/${encodeURIComponent(name)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(model),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || `Failed to save virtual model (${res.status})`);
  }
}

export async function deleteVirtualModel(name: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/virtual-models/${encodeURIComponent(name)}`, {
    method: 'DELETE',
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || `Failed to delete virtual model (${res.status})`);
  }
}

export async function fetchAnalytics(days = 30): Promise<AnalyticsResponse> {
  const res = await fetch(`${API_BASE}/api/analytics/daily?days=${days}`);
  if (!res.ok) {
    throw new Error(`Failed to fetch analytics: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchLogs(limit = 100): Promise<LogsResponse> {
  const res = await fetch(`${API_BASE}/logs?limit=${limit}`);
  if (!res.ok) {
    throw new Error(`Failed to fetch logs: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchAuthStatus(): Promise<AuthStatusResponse> {
  const res = await fetch(`${API_BASE}/auth/status`);
  if (!res.ok) {
    throw new Error(`Failed to fetch auth status: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchSetupRecommendations(): Promise<{ markdown: string }> {
  const res = await fetch(`${API_BASE}/api/setup/recommendations`);
  if (!res.ok) {
    throw new Error(`Failed to fetch setup recommendations: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchApiKeys(): Promise<ApiKeyRecord[]> {
  const res = await fetch(`${API_BASE}/api/keys`);
  if (!res.ok) {
    throw new Error(`Failed to fetch API keys: ${res.statusText}`);
  }
  return res.json();
}

export async function createApiKey(name: string, key?: string): Promise<ApiKeyRecord> {
  const res = await fetch(`${API_BASE}/api/keys`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, key: key?.trim() || undefined }),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || `Failed to create API key (${res.status})`);
  }
  return res.json();
}

export async function deleteApiKey(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/keys/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || `Failed to revoke API key (${res.status})`);
  }
}

export async function fetchVaultSecrets(): Promise<VaultSecretRecord[]> {
  const res = await fetch(`${API_BASE}/api/vault/secrets`);
  if (!res.ok) {
    throw new Error(`Failed to fetch vault secrets: ${res.statusText}`);
  }
  return res.json();
}

export async function createVaultSecret(input: VaultSecretInput): Promise<any> {
  const res = await fetch(`${API_BASE}/api/vault/secrets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || data?.error || `Failed to save secret to vault (${res.status})`);
  }
  return res.json();
}

export async function deleteVaultSecret(name: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/vault/secrets/${encodeURIComponent(name)}`, {
    method: 'DELETE',
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || data?.error || `Failed to delete secret (${res.status})`);
  }
}

export async function discoverModels(payload: {
  provider?: string;
  base_url?: string;
  vault_secret?: string;
  api_key?: string;
}): Promise<import('./types').DiscoverModelsResponse> {
  const res = await fetch(`${API_BASE}/api/providers/discover-models`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data?.error?.message || data?.error || `Failed to discover models (${res.status})`);
  }
  return res.json();
}


export async function logout(): Promise<void> {
  await fetch(`${API_BASE}/auth/logout`, { method: 'POST' }).catch(() => {});
  window.location.href = '/ui';
}
