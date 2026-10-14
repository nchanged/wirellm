import React, { useState, useEffect, useMemo } from 'react';
import {
  Lock,
  Plus,
  Trash2,
  Copy,
  Check,
  Search,
  RefreshCw,
  Cpu,
  Zap,
  Eye,
  EyeOff,
  ShieldCheck,
  ExternalLink,
} from 'lucide-react';
import { ApiStatusResponse, VaultSecretRecord, DiscoverModelsResponse } from '../types';
import {
  fetchVaultSecrets,
  createVaultSecret,
  deleteVaultSecret,
  discoverModels,
} from '../api';
import { Button, Badge, Input, Modal, cn } from '../ui';

interface VaultStudioProps {
  status: ApiStatusResponse | null;
  onRefresh: () => void;
  onNavigateToProviders?: () => void;
}

interface VaultPreset {
  id: string;
  name: string;
  secretName: string;
  baseUrl: string;
  description: string;
  badge: string;
}

const VAULT_PRESETS: VaultPreset[] = [
  {
    id: 'nvidia',
    name: 'NVIDIA',
    secretName: 'NVIDIA_API_KEY',
    baseUrl: 'https://integrate.api.nvidia.com/v1',
    description: 'NVIDIA NIM Nemotron & Llama 3.3',
    badge: '1k Credits',
  },
  {
    id: 'groq',
    name: 'Groq',
    secretName: 'GROQ_API_KEY',
    baseUrl: 'https://api.groq.com/openai/v1',
    description: 'GroqCloud LPUs (GPT-OSS, Qwen)',
    badge: '30 RPM Free',
  },
  {
    id: 'gemini',
    name: 'Gemini',
    secretName: 'GEMINI_API_KEY',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai/',
    description: 'Google Gemini 2.5 Flash 1M context',
    badge: '15 RPM Free',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    secretName: 'OPENROUTER_API_KEY',
    baseUrl: 'https://openrouter.ai/api/v1',
    description: 'OpenRouter 200+ models router',
    badge: 'Multi-Model',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    secretName: 'DEEPSEEK_API_KEY',
    baseUrl: 'https://api.deepseek.com/v1',
    description: 'DeepSeek-V3 & DeepSeek-R1 (Reasoner)',
    badge: 'SOTA R1',
  },
  {
    id: 'opencode',
    name: 'OpenCode',
    secretName: 'OPENCODE_GO_KEY',
    baseUrl: 'https://opencode.ai/zen/go/v1',
    description: 'OpenCode Zen / Go Models',
    badge: 'Zen API',
  },
  {
    id: 'cerebras',
    name: 'Cerebras',
    secretName: 'CEREBRAS_API_KEY',
    baseUrl: 'https://api.cerebras.ai/v1',
    description: 'Cerebras 1800+ tokens/sec',
    badge: '1800+ tps',
  },
];

export const VaultStudio: React.FC<VaultStudioProps> = ({
  status,
  onRefresh,
}) => {
  const [secrets, setSecrets] = useState<VaultSecretRecord[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [search, setSearch] = useState<string>('');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Add Secret Modal State
  const [isAddModalOpen, setIsAddModalOpen] = useState<boolean>(false);
  const [secretName, setSecretName] = useState<string>('');
  const [secretValue, setSecretValue] = useState<string>('');
  const [secretDescription, setSecretDescription] = useState<string>('');
  const [showSecretValue, setShowSecretValue] = useState<boolean>(false);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // Model Discovery Modal State
  const [isDiscoveryOpen, setIsDiscoveryOpen] = useState<boolean>(false);
  const [discoverySecret, setDiscoverySecret] = useState<VaultSecretRecord | null>(null);
  const [discoveryBaseUrl, setDiscoveryBaseUrl] = useState<string>('');
  const [discoveryLoading, setDiscoveryLoading] = useState<boolean>(false);
  const [discoveryResult, setDiscoveryResult] = useState<DiscoverModelsResponse | null>(null);
  const [discoveryError, setDiscoveryError] = useState<string | null>(null);

  const loadSecrets = async () => {
    try {
      setLoading(true);
      const data = await fetchVaultSecrets();
      setSecrets(data);
    } catch (err: any) {
      console.error('Failed to load vault secrets', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSecrets();
  }, []);

  const handleCopy = (text: string, identifier: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(identifier);
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const handleOpenAddModal = (preset?: VaultPreset) => {
    if (preset) {
      setSecretName(preset.secretName);
      setSecretDescription(preset.description);
    } else {
      setSecretName('');
      setSecretDescription('');
    }
    setSecretValue('');
    setShowSecretValue(false);
    setSaveError(null);
    setIsAddModalOpen(true);
  };

  const handleSaveSecret = async () => {
    if (!secretName.trim()) {
      setSaveError('Please enter a secret identifier (e.g. NVIDIA_API_KEY)');
      return;
    }
    if (!secretValue.trim()) {
      setSaveError('Please enter the secret value');
      return;
    }

    try {
      setIsSaving(true);
      setSaveError(null);
      await createVaultSecret({
        name: secretName.trim().toUpperCase().replace(/-/g, '_'),
        value: secretValue.trim(),
        description: secretDescription.trim(),
      });
      setIsAddModalOpen(false);
      await loadSecrets();
      onRefresh();
    } catch (err: any) {
      setSaveError(err.message || 'Failed to save secret to vault');
    } finally {
      setIsSaving(false);
    }
  };

  const handleDeleteSecret = async (name: string) => {
    if (!confirm(`Remove '${name}' from Central Vault?`)) return;
    try {
      await deleteVaultSecret(name);
      await loadSecrets();
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to delete vault secret');
    }
  };

  const handleOpenDiscovery = (secret: VaultSecretRecord) => {
    setDiscoverySecret(secret);
    setDiscoveryResult(null);
    setDiscoveryError(null);

    const cleanName = secret.name.toUpperCase();
    const matchedPreset = VAULT_PRESETS.find((p) => cleanName.includes(p.id.toUpperCase()));
    const matchedProv = status?.providers?.find((p) => cleanName.includes(p.name.toUpperCase()));

    const initialUrl = matchedPreset?.baseUrl || matchedProv?.base_url || 'https://integrate.api.nvidia.com/v1';
    setDiscoveryBaseUrl(initialUrl);
    setIsDiscoveryOpen(true);
  };

  const handleExecuteDiscovery = async () => {
    if (!discoverySecret || !discoveryBaseUrl.trim()) return;
    try {
      setDiscoveryLoading(true);
      setDiscoveryError(null);
      const res = await discoverModels({
        vault_secret: discoverySecret.name,
        base_url: discoveryBaseUrl.trim(),
      });
      setDiscoveryResult(res);
    } catch (err: any) {
      setDiscoveryError(err.message || 'Failed to query upstream models');
    } finally {
      setDiscoveryLoading(false);
    }
  };

  // Map providers to secrets
  const providerUsageMap = useMemo(() => {
    const map: Record<string, string[]> = {};
    if (!status?.providers) return map;

    for (const p of status.providers) {
      const tmpl = p.api_key_template || '';
      const varName = tmpl
        .replace(/^\$\{?/, '')
        .replace(/\}?$/, '')
        .toUpperCase()
        .replace(/-/g, '_');

      if (!map[varName]) map[varName] = [];
      map[varName].push(p.name);

      if (varName.endsWith('_API_KEY')) {
        const base = varName.replace(/_API_KEY$/, '');
        if (!map[base]) map[base] = [];
        map[base].push(p.name);
      }
    }
    return map;
  }, [status?.providers]);

  const filteredSecrets = useMemo(() => {
    if (!search.trim()) return secrets;
    const q = search.toLowerCase();
    return secrets.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        (s.description && s.description.toLowerCase().includes(q))
    );
  }, [secrets, search]);

  return (
    <div className="space-y-4">
      {/* Clean Header Bar */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-white/[0.08]">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-base font-bold font-mono tracking-tight text-white flex items-center gap-2">
              <Lock className="w-4 h-4 text-[var(--primary)]" />
              Central Vault Secrets
            </h1>
            <Badge variant="solid-primary" size="xs">
              AES-256-GCM
            </Badge>
            <span className="text-xs font-mono text-slate-500">
              ({secrets.length} {secrets.length === 1 ? 'key' : 'keys'})
            </span>
          </div>
          <p className="text-xs text-slate-400 mt-0.5">
            Encrypted keys available across all providers via <code className="text-[var(--primary)] font-mono">${`{NAME}`}</code>
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={loadSecrets}
            disabled={loading}
            className="h-8 text-xs font-mono"
          >
            <RefreshCw className={cn('w-3 h-3 mr-1.5', loading && 'animate-spin text-[var(--primary)]')} />
            Refresh
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={() => handleOpenAddModal()}
            className="h-8 text-xs font-mono font-bold shadow-[0_0_15px_var(--primary-glow)]"
          >
            <Plus className="w-3.5 h-3.5 mr-1" />
            Add Secret
          </Button>
        </div>
      </div>

      {/* Quick Preset Chips Row */}
      <div className="flex items-center gap-1.5 overflow-x-auto py-1 text-xs font-mono">
        <span className="text-slate-500 text-[11px] shrink-0 mr-1">Quick Add:</span>
        {VAULT_PRESETS.map((p) => {
          const isConfigured = secrets.some(
            (s) => s.name === p.secretName || s.name === p.id.toUpperCase()
          );
          return (
            <button
              key={p.id}
              onClick={() => handleOpenAddModal(p)}
              className={cn(
                'h-6 px-2 rounded text-[11px] font-mono flex items-center gap-1 transition-all shrink-0 border select-none',
                isConfigured
                  ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20 hover:border-emerald-500/40'
                  : 'bg-white/[0.03] text-slate-400 border-white/[0.08] hover:text-white hover:border-white/20'
              )}
            >
              <span>+ {p.name}</span>
              {isConfigured && <Check className="w-2.5 h-2.5 text-emerald-400" />}
            </button>
          );
        })}
      </div>

      {/* Search Filter Bar */}
      <div className="relative">
        <Search className="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" />
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Filter secrets by name..."
          className="pl-9 h-8 text-xs font-mono bg-[#030612] border-white/10"
        />
      </div>

      {/* Simple, Highly Readable List View */}
      {loading ? (
        <div className="p-8 text-center text-xs font-mono text-slate-500 space-y-2">
          <RefreshCw className="w-4 h-4 animate-spin mx-auto text-[var(--primary)]" />
          <span>Loading Vault secrets...</span>
        </div>
      ) : filteredSecrets.length === 0 ? (
        <div className="p-8 rounded-xl border border-dashed border-white/10 text-center space-y-2 bg-white/[0.01]">
          <p className="text-xs text-slate-400">No secrets found in the Vault.</p>
          <Button
            variant="primary"
            size="sm"
            onClick={() => handleOpenAddModal()}
            className="h-7 text-xs font-mono"
          >
            <Plus className="w-3 h-3 mr-1" />
            Add Secret
          </Button>
        </div>
      ) : (
        <div className="rounded-xl border border-white/[0.08] bg-[#02050f]/80 overflow-hidden">
          <div className="divide-y divide-white/[0.06]">
            {filteredSecrets.map((secret) => {
              const templatePill = `\${${secret.name}}`;
              const linkedProviders = providerUsageMap[secret.name] || [];
              const isCopied = copiedKey === secret.name;

              return (
                <div
                  key={secret.name}
                  className="px-4 py-3 flex flex-col sm:flex-row sm:items-center justify-between gap-3 hover:bg-white/[0.02] transition-colors"
                >
                  {/* Left: Name, Description, and Linked Provider */}
                  <div className="space-y-1 min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-mono font-bold text-xs text-white">
                        {secret.name}
                      </span>

                      {/* Template Pill with 1-click Copy */}
                      <button
                        onClick={() => handleCopy(templatePill, secret.name)}
                        title="Click to copy template variable"
                        className={cn(
                          'h-5 px-1.5 rounded text-[10px] font-mono flex items-center gap-1 transition-all border',
                          isCopied
                            ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/30'
                            : 'bg-white/[0.04] text-[var(--primary)] border-white/10 hover:border-[var(--primary-border)] hover:bg-[var(--primary-bg-badge)]'
                        )}
                      >
                        {isCopied ? (
                          <>
                            <Check className="w-2.5 h-2.5 text-emerald-400" />
                            <span>Copied</span>
                          </>
                        ) : (
                          <>
                            <Copy className="w-2.5 h-2.5 opacity-70" />
                            <span>{templatePill}</span>
                          </>
                        )}
                      </button>

                      {/* Linked Provider Badge */}
                      {linkedProviders.length > 0 ? (
                        <div className="flex items-center gap-1">
                          <span className="text-[10px] text-slate-500 font-mono">provider:</span>
                          {linkedProviders.map((p) => (
                            <span
                              key={p}
                              className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-semibold"
                            >
                              {p}
                            </span>
                          ))}
                        </div>
                      ) : (
                        <span className="text-[10px] font-mono text-slate-500 italic">
                          (standby)
                        </span>
                      )}
                    </div>

                    {secret.description && (
                      <p className="text-[11px] text-slate-400 truncate">
                        {secret.description}
                      </p>
                    )}
                  </div>

                  {/* Right: Key Preview & Actions */}
                  <div className="flex items-center gap-3 shrink-0 self-end sm:self-center">
                    <span className="text-slate-500 text-[11px] font-mono tracking-widest hidden md:inline">
                      {secret.key_preview || '••••••••••••'}
                    </span>

                    <div className="flex items-center gap-1">
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleOpenDiscovery(secret)}
                        title="Query live models upstream using this secret"
                        className="h-7 px-2 text-[11px] font-mono text-slate-300 hover:text-[var(--primary)]"
                      >
                        <Cpu className="w-3 h-3 mr-1 text-[var(--primary)]" />
                        Explore Models
                      </Button>

                      <button
                        onClick={() => handleDeleteSecret(secret.name)}
                        title={`Delete ${secret.name}`}
                        className="h-7 w-7 rounded flex items-center justify-center text-slate-500 hover:text-rose-400 hover:bg-rose-500/10 border border-transparent hover:border-rose-500/30 transition-all"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Add Secret Modal */}
      <Modal
        isOpen={isAddModalOpen}
        onClose={() => setIsAddModalOpen(false)}
        title="Add Secret to Central Vault"
      >
        <div className="space-y-4 font-mono text-xs">
          <div className="p-2.5 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-slate-300 text-[11px] font-sans">
            Encrypted with AES-256-GCM. Plaintext is never returned in API or UI responses.
          </div>

          <div className="space-y-1">
            <label className="text-slate-300 font-bold block text-xs">
              Secret Name <span className="text-rose-400">*</span>
            </label>
            <Input
              value={secretName}
              onChange={(e) => setSecretName(e.target.value.toUpperCase().replace(/\s+/g, '_'))}
              placeholder="e.g. NVIDIA_API_KEY"
              className="font-mono text-xs h-8"
            />
            <span className="text-[10px] text-slate-500">
              Template: <code className="text-[var(--primary)]">${`{${secretName || 'NAME'}}`}</code>
            </span>
          </div>

          <div className="space-y-1">
            <label className="text-slate-300 font-bold block text-xs">
              API Key / Token <span className="text-rose-400">*</span>
            </label>
            <div className="relative">
              <Input
                type={showSecretValue ? 'text' : 'password'}
                value={secretValue}
                onChange={(e) => setSecretValue(e.target.value)}
                placeholder="nvapi-... or gsk_... or sk-..."
                className="font-mono text-xs h-8 pr-9"
              />
              <button
                type="button"
                onClick={() => setShowSecretValue(!showSecretValue)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-200"
              >
                {showSecretValue ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              </button>
            </div>
          </div>

          <div className="space-y-1">
            <label className="text-slate-400 block text-xs">
              Description (Optional)
            </label>
            <Input
              value={secretDescription}
              onChange={(e) => setSecretDescription(e.target.value)}
              placeholder="e.g. NVIDIA NIM production token"
              className="text-xs h-8"
            />
          </div>

          {saveError && (
            <div className="p-2.5 rounded bg-rose-500/10 border border-rose-500/20 text-rose-300 text-xs">
              {saveError}
            </div>
          )}

          <div className="pt-2 border-t border-white/10 flex items-center justify-end gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setIsAddModalOpen(false)}
              disabled={isSaving}
              className="h-8 text-xs font-mono"
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={handleSaveSecret}
              disabled={isSaving}
              className="h-8 text-xs font-mono font-bold"
            >
              {isSaving ? 'Saving...' : 'Save Secret'}
            </Button>
          </div>
        </div>
      </Modal>

      {/* Model Discovery Modal */}
      <Modal
        isOpen={isDiscoveryOpen}
        onClose={() => setIsDiscoveryOpen(false)}
        title={`Explore Upstream Models for '${discoverySecret?.name}'`}
      >
        <div className="space-y-3 font-mono text-xs">
          <p className="text-[11px] text-slate-400 font-sans">
            Queries the provider's upstream <code className="text-slate-300">/v1/models</code> catalog using the encrypted Vault key:
          </p>

          <div className="space-y-1">
            <label className="text-slate-400 text-xs block">Base URL</label>
            <div className="flex items-center gap-2">
              <Input
                value={discoveryBaseUrl}
                onChange={(e) => setDiscoveryBaseUrl(e.target.value)}
                placeholder="https://integrate.api.nvidia.com/v1"
                className="font-mono text-xs h-8 flex-1"
              />
              <Button
                variant="primary"
                size="sm"
                onClick={handleExecuteDiscovery}
                disabled={discoveryLoading}
                className="h-8 text-xs font-mono font-bold shrink-0"
              >
                {discoveryLoading ? (
                  <>
                    <RefreshCw className="w-3 h-3 animate-spin mr-1" />
                    Querying...
                  </>
                ) : (
                  <>
                    <Zap className="w-3 h-3 mr-1 text-amber-300" />
                    Fetch Models
                  </>
                )}
              </Button>
            </div>
          </div>

          {discoveryError && (
            <div className="p-2.5 rounded bg-rose-500/10 border border-rose-500/20 text-rose-300 text-xs">
              {discoveryError}
            </div>
          )}

          {discoveryResult && (
            <div className="space-y-2.5 pt-2 border-t border-white/10">
              <div className="flex items-center justify-between text-xs font-mono">
                <span className="text-emerald-400 font-bold">
                  ✓ Found {discoveryResult.total_models} Upstream Models
                </span>
                <span className="text-slate-500 text-[11px]">{discoveryResult.base_url}</span>
              </div>

              {discoveryResult.recommended_coding.length > 0 && (
                <div className="space-y-1">
                  <span className="text-slate-400 text-[11px] block">Coding Recommendations:</span>
                  <div className="flex flex-wrap gap-1">
                    {discoveryResult.recommended_coding.map((m) => (
                      <span
                        key={m}
                        className="px-1.5 py-0.5 rounded text-[10px] bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)]"
                      >
                        {m}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              <div className="space-y-1">
                <span className="text-slate-400 text-[11px] block">
                  All Catalog Models ({discoveryResult.models.length}):
                </span>
                <div className="max-h-40 overflow-y-auto p-2 rounded bg-black/60 border border-white/10 space-y-0.5 font-mono text-[11px] text-slate-300">
                  {discoveryResult.models.map((m) => (
                    <div key={m} className="hover:text-white select-all py-0.5">
                      {m}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}

          <div className="pt-2 border-t border-white/10 flex items-center justify-end">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setIsDiscoveryOpen(false)}
              className="h-8 text-xs font-mono"
            >
              Close
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};
