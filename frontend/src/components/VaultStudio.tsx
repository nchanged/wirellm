import React, { useState, useEffect, useMemo } from 'react';
import {
  ShieldCheck,
  Plus,
  Trash2,
  Key,
  Lock,
  Copy,
  Check,
  Eye,
  EyeOff,
  Sparkles,
  Search,
  ExternalLink,
  Bot,
  Layers,
  ChevronRight,
  RefreshCw,
  Cpu,
  Zap,
} from 'lucide-react';
import { ApiStatusResponse, VaultSecretRecord, DiscoverModelsResponse } from '../types';
import {
  fetchVaultSecrets,
  createVaultSecret,
  deleteVaultSecret,
  discoverModels,
} from '../api';
import { Card, Button, Badge, Input, Modal, cn } from '../ui';

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
  signupUrl: string;
  badge: string;
  color: string;
}

const VAULT_PRESETS: VaultPreset[] = [
  {
    id: 'nvidia',
    name: 'NVIDIA NIM',
    secretName: 'NVIDIA_API_KEY',
    baseUrl: 'https://integrate.api.nvidia.com/v1',
    description: 'Nemotron 3 Ultra 550B, Llama 3.3 70B, DeepSeek V3.1. ~1,000 free credits on signup.',
    signupUrl: 'https://build.nvidia.com',
    badge: '1,000 Free Credits',
    color: 'emerald',
  },
  {
    id: 'groq',
    name: 'GroqCloud',
    secretName: 'GROQ_API_KEY',
    baseUrl: 'https://api.groq.com/openai/v1',
    description: 'Fastest LPUs (500+ tps). GPT-OSS 120B, Qwen 2.5 72B, Llama 3.3 70B. 30 RPM free.',
    signupUrl: 'https://console.groq.com/keys',
    badge: 'Free 30 RPM',
    color: 'amber',
  },
  {
    id: 'gemini',
    name: 'Google Gemini Studio',
    secretName: 'GEMINI_API_KEY',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai/',
    description: 'Gemini 2.5 Flash, 2.0 Flash. 1 Million token context window. 15 RPM free tier.',
    signupUrl: 'https://aistudio.google.com/app/apikey',
    badge: 'Free 15 RPM / 1M Ctx',
    color: 'cyan',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    secretName: 'OPENROUTER_TOKEN',
    baseUrl: 'https://openrouter.ai/api/v1',
    description: '200+ AI models through a unified key. Includes free tier models (:free).',
    signupUrl: 'https://openrouter.ai/keys',
    badge: 'Free & Budget Routing',
    color: 'purple',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek Official',
    secretName: 'DEEPSEEK_API_KEY',
    baseUrl: 'https://api.deepseek.com/v1',
    description: 'DeepSeek-V3 and DeepSeek-R1 (Reasoner). Ultra-cheap state of the art reasoning.',
    signupUrl: 'https://platform.deepseek.com/api_keys',
    badge: 'Cheap SOTA Reasoning',
    color: 'blue',
  },
  {
    id: 'cerebras',
    name: 'Cerebras',
    secretName: 'CEREBRAS_API_KEY',
    baseUrl: 'https://api.cerebras.ai/v1',
    description: 'Wafer-scale instantaneous inference (1800+ tps). Llama 3.1 70B & 8B free.',
    signupUrl: 'https://cloud.cerebras.ai',
    badge: '1800+ Tokens/sec',
    color: 'orange',
  },
  {
    id: 'anthropic',
    name: 'Anthropic Claude',
    secretName: 'ANTHROPIC_API_KEY',
    baseUrl: 'https://api.anthropic.com/v1',
    description: 'Claude 3.5 Sonnet, Claude 3.5 Haiku. Premium coding benchmark leader.',
    signupUrl: 'https://console.anthropic.com',
    badge: 'Direct Claude Key',
    color: 'amber',
  },
  {
    id: 'openai',
    name: 'OpenAI',
    secretName: 'OPENAI_API_KEY',
    baseUrl: 'https://api.openai.com/v1',
    description: 'GPT-4o, o1, o3-mini. Direct OpenAI platform subscription key.',
    signupUrl: 'https://platform.openai.com/api-keys',
    badge: 'Direct OpenAI Key',
    color: 'emerald',
  },
];

export const VaultStudio: React.FC<VaultStudioProps> = ({
  status,
  onRefresh,
  onNavigateToProviders,
}) => {
  const [secrets, setSecrets] = useState<VaultSecretRecord[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [search, setSearch] = useState<string>('');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [copiedPrompt, setCopiedPrompt] = useState<boolean>(false);

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
      setSecretDescription(`API key for ${preset.name} (${preset.badge})`);
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
      setSaveError('Please enter a secret identifier name (e.g. NVIDIA_API_KEY)');
      return;
    }
    if (!secretValue.trim()) {
      setSaveError('Please enter the secret token or API key');
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
    if (!confirm(`Are you sure you want to remove '${name}' from the central Vault? Any provider referencing \${${name}} will fail to authenticate until reconfigured.`)) {
      return;
    }
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

    // Guess base URL from preset or existing provider
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
      setDiscoveryError(err.message || 'Failed to query upstream models endpoint');
    } finally {
      setDiscoveryLoading(false);
    }
  };

  // Check which providers use which secrets
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

      // Also map prefix without _API_KEY
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

  const totalLinkedCount = useMemo(() => {
    return secrets.filter((s) => (providerUsageMap[s.name]?.length || 0) > 0).length;
  }, [secrets, providerUsageMap]);

  const copyAgentPromptText = () => {
    const text = `Connect to WireLLM MCP server at ${window.location.origin}/mcp/sse.
I have added secrets to the WireLLM Central Vault (e.g. NVIDIA_API_KEY).
1. Inspect available vault items with 'wirellm_list_secrets'.
2. Discover upstream models with 'wirellm_discover_provider_models'.
3. Register the provider with 'wirellm_add_provider' referencing the vault secret.
4. Update virtual models 'coder' and 'planner' with 'wirellm_update_virtual_model'.
5. Verify health with 'wirellm_get_status'.`;
    navigator.clipboard.writeText(text);
    setCopiedPrompt(true);
    setTimeout(() => setCopiedPrompt(false), 2500);
  };

  return (
    <div className="space-y-6">
      {/* High-Tech Glassmorphic Header */}
      <div className="p-6 rounded-2xl bg-gradient-to-r from-[#030712] via-[#050b1d] to-[#040916] border-2 border-[var(--primary-border)] shadow-[0_15px_40px_rgba(0,0,0,0.8),0_0_25px_var(--primary-glow-subtle)] relative overflow-hidden">
        {/* Glow accent */}
        <div className="absolute top-0 right-0 w-96 h-96 bg-[var(--primary)]/10 rounded-full blur-[100px] pointer-events-none" />

        <div className="relative z-10 flex flex-col lg:flex-row items-start lg:items-center justify-between gap-6">
          <div className="space-y-2">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-xl bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] flex items-center justify-center shadow-[0_0_15px_var(--primary-glow)]">
                <Lock className="w-5 h-5 text-[var(--primary)]" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <h1 className="text-xl font-bold font-mono tracking-tight text-white">
                    Central Encrypted Vault
                  </h1>
                  <Badge variant="solid-primary" size="xs">
                    AES-256-GCM
                  </Badge>
                  <span className="hidden sm:inline-flex items-center gap-1 text-[11px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded-full border border-emerald-500/20">
                    <ShieldCheck className="w-3 h-3 text-emerald-400" />
                    Authenticated Storage
                  </span>
                </div>
                <p className="text-xs text-slate-400 font-mono mt-0.5">
                  Unified credential store. Save API keys once and reference them everywhere with{' '}
                  <code className="text-[var(--primary)] px-1 py-0.5 rounded bg-black/40 border border-white/10 font-bold">
                    ${`{SECRET_NAME}`}
                  </code>
                </p>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2.5 w-full sm:w-auto">
            <Button
              variant="secondary"
              size="sm"
              onClick={loadSecrets}
              disabled={loading}
              className="font-mono text-xs hover:border-[var(--primary-border)]"
            >
              <RefreshCw className={cn('w-3.5 h-3.5 mr-1.5', loading && 'animate-spin text-[var(--primary)]')} />
              Refresh
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => handleOpenAddModal()}
              className="font-mono text-xs shadow-[0_0_20px_var(--primary-glow)] font-bold flex-1 sm:flex-initial"
            >
              <Plus className="w-4 h-4 mr-1.5" />
              Add Secret to Vault
            </Button>
          </div>
        </div>
      </div>

      {/* Metrics Row */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4 font-mono text-xs">
        <div className="p-4 rounded-xl bg-[#030611]/90 border border-white/[0.08] flex items-center justify-between">
          <div>
            <span className="text-slate-500 text-[11px] block">STORED VAULT SECRETS</span>
            <span className="text-xl font-bold text-white tracking-tight mt-0.5 block">
              {secrets.length}
            </span>
          </div>
          <div className="w-9 h-9 rounded-lg bg-white/[0.04] border border-white/10 flex items-center justify-center">
            <Key className="w-4 h-4 text-[var(--primary)]" />
          </div>
        </div>

        <div className="p-4 rounded-xl bg-[#030611]/90 border border-white/[0.08] flex items-center justify-between">
          <div>
            <span className="text-slate-500 text-[11px] block">LINKED TO PROVIDERS</span>
            <span className="text-xl font-bold text-emerald-400 tracking-tight mt-0.5 block">
              {totalLinkedCount} / {secrets.length}
            </span>
          </div>
          <div className="w-9 h-9 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center">
            <Layers className="w-4 h-4 text-emerald-400" />
          </div>
        </div>

        <div className="p-4 rounded-xl bg-[#030611]/90 border border-white/[0.08] flex items-center justify-between">
          <div>
            <span className="text-slate-500 text-[11px] block">SECURITY AT REST</span>
            <span className="text-xs font-bold text-slate-200 tracking-tight mt-1.5 block">
              AES-256-GCM / 96-Bit Nonce
            </span>
          </div>
          <div className="w-9 h-9 rounded-lg bg-white/[0.04] border border-white/10 flex items-center justify-center">
            <ShieldCheck className="w-4 h-4 text-[var(--secondary)]" />
          </div>
        </div>

        <div className="p-4 rounded-xl bg-[#030611]/90 border border-white/[0.08] flex items-center justify-between">
          <div>
            <span className="text-slate-500 text-[11px] block">AGENT MCP ACCESS</span>
            <span className="text-xs font-bold text-amber-400 tracking-tight mt-1.5 block">
              Enabled (Zero Leakage)
            </span>
          </div>
          <div className="w-9 h-9 rounded-lg bg-amber-500/10 border border-amber-500/20 flex items-center justify-center">
            <Bot className="w-4 h-4 text-amber-400" />
          </div>
        </div>
      </div>

      {/* 1-Click Onboard Presets */}
      <div className="p-4 rounded-xl bg-[#02050f]/80 border border-white/[0.08] space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs font-mono text-slate-300">
            <Sparkles className="w-3.5 h-3.5 text-[var(--primary)]" />
            <span className="font-semibold">Quick 1-Click Presets:</span>
            <span className="text-slate-500 text-[11px]">Select a provider to prefill its standard Vault secret name</span>
          </div>
        </div>

        <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2">
          {VAULT_PRESETS.map((preset) => {
            const isConfigured = secrets.some(
              (s) => s.name === preset.secretName || s.name === preset.id.toUpperCase()
            );

            return (
              <button
                key={preset.id}
                onClick={() => handleOpenAddModal(preset)}
                className={cn(
                  'p-2.5 rounded-lg border text-left flex flex-col justify-between transition-all group relative',
                  isConfigured
                    ? 'bg-emerald-500/[0.05] border-emerald-500/20 hover:border-emerald-500/50'
                    : 'bg-white/[0.02] border-white/[0.06] hover:border-[var(--primary-border)] hover:bg-white/[0.04]'
                )}
              >
                <div className="space-y-1">
                  <div className="flex items-center justify-between">
                    <span className="text-[11px] font-bold font-mono text-white group-hover:text-[var(--primary)] transition-colors truncate">
                      {preset.name}
                    </span>
                    {isConfigured && (
                      <span title="Configured in Vault" className="w-1.5 h-1.5 rounded-full bg-emerald-400 shadow-[0_0_6px_#34d399]" />
                    )}
                  </div>
                  <span className="text-[9px] font-mono text-slate-500 block truncate">
                    ${`{${preset.secretName}}`}
                  </span>
                </div>
                <span className="text-[9px] text-[var(--primary)] mt-2 font-mono flex items-center gap-1 opacity-80 group-hover:opacity-100">
                  {isConfigured ? 'Update key' : '+ Add to Vault'}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Search Bar & Secrets Header */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
        <div className="relative flex-1 max-w-md">
          <Search className="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter secrets by name or description..."
            className="pl-9 h-9 text-xs font-mono bg-[#030612] border-white/10"
          />
        </div>
        <div className="text-xs font-mono text-slate-500 text-right">
          Showing {filteredSecrets.length} of {secrets.length} configured secrets
        </div>
      </div>

      {/* Configured Secrets Cards Grid */}
      {loading ? (
        <div className="p-12 text-center text-xs font-mono text-slate-500 space-y-2">
          <RefreshCw className="w-5 h-5 animate-spin mx-auto text-[var(--primary)]" />
          <span>QUERYING CENTRAL ENCRYPTED VAULT...</span>
        </div>
      ) : filteredSecrets.length === 0 ? (
        <div className="p-12 rounded-xl border border-dashed border-white/10 text-center space-y-4 bg-white/[0.01]">
          <div className="w-12 h-12 rounded-2xl bg-white/[0.03] border border-white/10 flex items-center justify-center mx-auto text-slate-500">
            <Lock className="w-6 h-6 text-slate-400" />
          </div>
          <div className="space-y-1">
            <h3 className="text-sm font-bold text-white font-mono">No Secrets in Vault Yet</h3>
            <p className="text-xs text-slate-400 max-w-md mx-auto">
              Add your provider API keys (NVIDIA, Groq, OpenRouter, DeepSeek) once to central storage, or let an AI agent configure them via MCP.
            </p>
          </div>
          <Button
            variant="primary"
            size="sm"
            onClick={() => handleOpenAddModal()}
            className="font-mono text-xs shadow-[0_0_15px_var(--primary-glow)] font-bold"
          >
            <Plus className="w-4 h-4 mr-1.5" />
            Add First Secret
          </Button>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {filteredSecrets.map((secret) => {
            const templatePill = `\${${secret.name}}`;
            const linkedProviders = providerUsageMap[secret.name] || [];
            const isCopied = copiedKey === secret.name;

            return (
              <div
                key={secret.name}
                className="p-4 rounded-xl bg-[#030612]/90 border border-white/[0.08] hover:border-[var(--primary-border)]/60 transition-all space-y-3.5 relative group shadow-lg"
              >
                {/* Header: Name, Template Pill, and Actions */}
                <div className="flex items-start justify-between gap-3">
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-bold font-mono text-white tracking-tight">
                        {secret.name}
                      </span>
                      <Badge variant="solid-primary" size="xs">
                        AES-256
                      </Badge>
                    </div>
                    {secret.description && (
                      <p className="text-[11px] text-slate-400 font-sans line-clamp-1">
                        {secret.description}
                      </p>
                    )}
                  </div>

                  <div className="flex items-center gap-1.5 shrink-0">
                    <button
                      onClick={() => handleOpenDiscovery(secret)}
                      title="Explore & discover models from upstream using this secret"
                      className="h-7 px-2 rounded bg-white/[0.04] hover:bg-[var(--primary-bg-badge)] border border-white/10 hover:border-[var(--primary-border)] text-[10px] font-mono text-slate-300 hover:text-[var(--primary)] flex items-center gap-1 transition-all"
                    >
                      <Cpu className="w-3 h-3 text-[var(--primary)]" />
                      <span>Explore Models</span>
                    </button>
                    <button
                      onClick={() => handleDeleteSecret(secret.name)}
                      title={`Remove '${secret.name}' from vault`}
                      className="h-7 w-7 rounded bg-white/[0.04] hover:bg-rose-500/20 border border-white/10 hover:border-rose-500/40 text-slate-400 hover:text-rose-400 flex items-center justify-center transition-all"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                {/* Key Preview & Template Copy */}
                <div className="p-2.5 rounded-lg bg-[#010307] border border-white/[0.06] flex items-center justify-between gap-3 text-xs font-mono">
                  <div className="flex items-center gap-2 text-slate-400">
                    <Key className="w-3.5 h-3.5 text-slate-500 shrink-0" />
                    <span className="tracking-widest text-[11px] text-slate-300 font-mono">
                      {secret.key_preview || '••••••••••••••••'}
                    </span>
                  </div>

                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={() => handleCopy(templatePill, secret.name)}
                      className={cn(
                        'px-2 py-1 rounded text-[11px] font-mono flex items-center gap-1 transition-all border',
                        isCopied
                          ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                          : 'bg-white/[0.03] text-slate-300 hover:text-white border-white/10 hover:border-white/20'
                      )}
                      title="Copy template variable for provider config"
                    >
                      {isCopied ? (
                        <>
                          <Check className="w-3 h-3 text-emerald-400" />
                          <span>Copied!</span>
                        </>
                      ) : (
                        <>
                          <Copy className="w-3 h-3 text-slate-400" />
                          <span className="text-[var(--primary)] font-semibold">{templatePill}</span>
                        </>
                      )}
                    </button>
                  </div>
                </div>

                {/* Linked Providers Status */}
                <div className="flex items-center justify-between pt-1 border-t border-white/[0.04] text-[11px] font-mono">
                  <div className="flex items-center gap-1.5">
                    <span className="text-slate-500">Linked Providers:</span>
                    {linkedProviders.length > 0 ? (
                      <div className="flex items-center gap-1">
                        {linkedProviders.map((p) => (
                          <span
                            key={p}
                            className="px-1.5 py-0.5 rounded text-[10px] bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-semibold"
                          >
                            {p}
                          </span>
                        ))}
                      </div>
                    ) : (
                      <span className="text-slate-500 italic">Standby (Ready for providers or MCP)</span>
                    )}
                  </div>

                  <span className="text-slate-600 text-[10px]">
                    Updated {secret.updated_at?.split(' ')[0] || 'recently'}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Interactive MCP Agent Onboarding Guide Card */}
      <div className="p-5 rounded-2xl bg-[#040816] border border-white/10 space-y-4 font-mono text-xs">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] flex items-center justify-center">
              <Bot className="w-4 h-4 text-[var(--primary)]" />
            </div>
            <div>
              <span className="text-sm font-bold text-white block">
                Agentic MCP Onboarding Workflow
              </span>
              <span className="text-[11px] text-slate-400 font-sans block">
                How autonomous coding agents (Cursor, Claude Code, Antigravity, OpenCode) leverage the Central Vault:
              </span>
            </div>
          </div>

          <Button
            variant="secondary"
            size="sm"
            onClick={copyAgentPromptText}
            className="text-xs font-mono hover:border-[var(--primary-border)] hover:text-[var(--primary)]"
          >
            {copiedPrompt ? (
              <>
                <Check className="w-3.5 h-3.5 mr-1.5 text-emerald-400" />
                Copied Prompt!
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5 mr-1.5" />
                Copy Agent Prompt
              </>
            )}
          </Button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          <div className="p-3 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-1.5">
            <div className="flex items-center gap-2 text-[var(--primary)] font-bold">
              <span className="w-4 h-4 rounded-full bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] flex items-center justify-center text-[10px]">1</span>
              <span>Add Key to Vault</span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">
              Enter your provider key once above (e.g. <code className="text-slate-300">NVIDIA_API_KEY</code>). It is encrypted at rest using AES-256-GCM.
            </p>
          </div>

          <div className="p-3 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-1.5">
            <div className="flex items-center gap-2 text-cyan-400 font-bold">
              <span className="w-4 h-4 rounded-full bg-cyan-500/10 border border-cyan-500/20 flex items-center justify-center text-[10px]">2</span>
              <span>MCP Explores Models</span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">
              The agent runs <code className="text-cyan-300">wirellm_discover_provider_models</code> to query the upstream catalog without exposing your plaintext key.
            </p>
          </div>

          <div className="p-3 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-1.5">
            <div className="flex items-center gap-2 text-emerald-400 font-bold">
              <span className="w-4 h-4 rounded-full bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-[10px]">3</span>
              <span>Auto Configures Mesh</span>
            </div>
            <p className="text-[11px] text-slate-400 font-sans">
              The agent calls <code className="text-emerald-300">wirellm_add_provider</code> with <code className="text-emerald-300">vault_secret</code> and binds models to virtual routing chains!
            </p>
          </div>
        </div>
      </div>

      {/* Add Secret Modal */}
      <Modal
        isOpen={isAddModalOpen}
        onClose={() => setIsAddModalOpen(false)}
        title="Add Secret to Central Vault"
      >
        <div className="space-y-4 font-mono text-xs">
          <div className="p-3 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] space-y-1">
            <div className="flex items-center gap-1.5 text-[var(--primary)] font-bold">
              <ShieldCheck className="w-4 h-4 text-[var(--primary)]" />
              <span>AES-256-GCM Authenticated Encryption</span>
            </div>
            <p className="text-[11px] text-slate-300 font-sans leading-relaxed">
              Secrets are encrypted before writing to SQLite with a unique 96-bit random nonce per entry. Plaintext is never returned across REST APIs or MCP.
            </p>
          </div>

          <div className="space-y-1.5">
            <label className="text-slate-300 font-bold block">
              Secret Identifier Name <span className="text-rose-400">*</span>
            </label>
            <Input
              value={secretName}
              onChange={(e) => setSecretName(e.target.value.toUpperCase().replace(/\s+/g, '_'))}
              placeholder="e.g. NVIDIA_API_KEY, GROQ_API_KEY, OPENROUTER_TOKEN"
              className="font-mono text-xs"
            />
            <span className="text-[10px] text-slate-500">
              Providers reference this as{' '}
              <code className="text-[var(--primary)] font-bold">
                ${`{${secretName || 'SECRET_NAME'}}`}
              </code>
            </span>
          </div>

          <div className="space-y-1.5">
            <label className="text-slate-300 font-bold block">
              Secret Key / Bearer Token <span className="text-rose-400">*</span>
            </label>
            <div className="relative">
              <Input
                type={showSecretValue ? 'text' : 'password'}
                value={secretValue}
                onChange={(e) => setSecretValue(e.target.value)}
                placeholder="nvapi-... or gsk_... or sk-or-v1-..."
                className="font-mono text-xs pr-10"
              />
              <button
                type="button"
                onClick={() => setShowSecretValue(!showSecretValue)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-200"
              >
                {showSecretValue ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
              </button>
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-slate-300 block">
              Description / Notes (Optional)
            </label>
            <Input
              value={secretDescription}
              onChange={(e) => setSecretDescription(e.target.value)}
              placeholder="e.g. Primary NVIDIA NIM key with 1000 credits"
              className="font-sans text-xs"
            />
          </div>

          {saveError && (
            <div className="p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-300 text-xs">
              {saveError}
            </div>
          )}

          <div className="pt-3 border-t border-white/10 flex items-center justify-end gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setIsAddModalOpen(false)}
              disabled={isSaving}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={handleSaveSecret}
              disabled={isSaving}
              className="font-bold shadow-[0_0_15px_var(--primary-glow)]"
            >
              {isSaving ? 'Encrypting & Saving...' : 'Encrypt & Save to Vault'}
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
        <div className="space-y-4 font-mono text-xs">
          <p className="text-[11px] text-slate-400 font-sans">
            WireLLM connects directly to the provider's upstream OpenAI-compatible <code className="text-slate-300">/v1/models</code> endpoint using the encrypted secret from your Central Vault to discover live available models.
          </p>

          <div className="space-y-1.5">
            <label className="text-slate-300 font-bold block">Upstream Base URL</label>
            <div className="flex items-center gap-2">
              <Input
                value={discoveryBaseUrl}
                onChange={(e) => setDiscoveryBaseUrl(e.target.value)}
                placeholder="https://integrate.api.nvidia.com/v1"
                className="font-mono text-xs flex-1"
              />
              <Button
                variant="primary"
                size="sm"
                onClick={handleExecuteDiscovery}
                disabled={discoveryLoading}
                className="shrink-0 font-bold shadow-[0_0_15px_var(--primary-glow)]"
              >
                {discoveryLoading ? (
                  <>
                    <RefreshCw className="w-3.5 h-3.5 animate-spin mr-1.5" />
                    Querying...
                  </>
                ) : (
                  <>
                    <Zap className="w-3.5 h-3.5 mr-1.5 text-amber-300" />
                    Discover Models
                  </>
                )}
              </Button>
            </div>
          </div>

          {discoveryError && (
            <div className="p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-300 text-xs">
              {discoveryError}
            </div>
          )}

          {discoveryResult && (
            <div className="space-y-3 pt-2 border-t border-white/10">
              <div className="flex items-center justify-between text-xs">
                <span className="text-emerald-400 font-bold">
                  ✓ Discovered {discoveryResult.total_models} Upstream Models
                </span>
                <span className="text-slate-500">{discoveryResult.base_url}</span>
              </div>

              {discoveryResult.recommended_coding.length > 0 && (
                <div className="p-3 rounded-lg bg-white/[0.02] border border-white/[0.08] space-y-1.5">
                  <span className="text-slate-300 font-semibold text-[11px] block">
                    Top Recommended Coding Models:
                  </span>
                  <div className="flex flex-wrap gap-1.5">
                    {discoveryResult.recommended_coding.map((m) => (
                      <span
                        key={m}
                        className="px-2 py-0.5 rounded text-[10px] bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)]"
                      >
                        {m}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {discoveryResult.recommended_reasoning.length > 0 && (
                <div className="p-3 rounded-lg bg-white/[0.02] border border-white/[0.08] space-y-1.5">
                  <span className="text-slate-300 font-semibold text-[11px] block">
                    Top Recommended Reasoning Models:
                  </span>
                  <div className="flex flex-wrap gap-1.5">
                    {discoveryResult.recommended_reasoning.map((m) => (
                      <span
                        key={m}
                        className="px-2 py-0.5 rounded text-[10px] bg-purple-500/10 text-purple-300 border border-purple-500/20"
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
                <div className="max-h-48 overflow-y-auto p-2 rounded-lg bg-black/60 border border-white/10 space-y-1">
                  {discoveryResult.models.map((m) => (
                    <div
                      key={m}
                      className="text-[11px] text-slate-300 hover:text-white select-all font-mono py-0.5 px-1 rounded hover:bg-white/[0.04]"
                    >
                      {m}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}

          <div className="pt-3 border-t border-white/10 flex items-center justify-end">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setIsDiscoveryOpen(false)}
            >
              Close
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};
