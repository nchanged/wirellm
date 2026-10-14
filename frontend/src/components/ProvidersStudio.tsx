import React, { useState, useMemo, useEffect } from 'react';
import {
  Server,
  Plus,
  Edit,
  Trash2,
  Key,
  Lock,
  Play,
  CheckCircle,
  XCircle,
  Activity,
  Sparkles,
  Clock,
  Sliders,
  Search,
  Check,
  Copy,
  Globe,
  ShieldCheck,
  Eye,
  EyeOff,
} from 'lucide-react';
import { ApiStatusResponse, ProviderInput, TestConnectionResponse, VaultSecretRecord } from '../types';
import {
  createProvider,
  updateProvider,
  deleteProvider,
  testProvider,
  fetchVaultSecrets,
  createVaultSecret,
  deleteVaultSecret,
} from '../api';
import { Card, Button, Badge, Input, Select, Modal } from '../ui';

interface ProvidersStudioProps {
  status: ApiStatusResponse | null;
  onRefresh: () => void;
}

interface ProviderPreset {
  id: string;
  label: string;
  name: string;
  base_url: string;
  api_key_template: string;
  models: string[];
  rpm: number;
  min_interval_ms: number;
  max_concurrency: number;
  smartness_score: number;
}

const PRESETS: ProviderPreset[] = [
  {
    id: 'deepseek',
    label: 'DeepSeek (Official API)',
    name: 'deepseek',
    base_url: 'https://api.deepseek.com/v1',
    api_key_template: '${DEEPSEEK_API_KEY}',
    models: [
      'deepseek-chat',
      'deepseek-reasoner',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 95,
  },
  {
    id: 'opencode',
    label: 'OpenCode Zen / Go (Zen Models)',
    name: 'opencode',
    base_url: 'https://opencode.ai/zen/go/v1',
    api_key_template: '${OPENCODE_GO_KEY}',
    models: [
      'longcat-2.5-preview-free',
      'step-5-preview-free',
      'minimax-01',
    ],
    rpm: 20,
    min_interval_ms: 3000,
    max_concurrency: 2,
    smartness_score: 85,
  },
  {
    id: 'openrouter',
    label: 'OpenRouter (Multi-model)',
    name: 'openrouter',
    base_url: 'https://openrouter.ai/api/v1',
    api_key_template: '${OPENROUTER_TOKEN}',
    models: [
      'openrouter/free',
      'nvidia/nemotron-3.5-lightning:free',
      'cohere/north-mini-code:free',
    ],
    rpm: 20,
    min_interval_ms: 2500,
    max_concurrency: 2,
    smartness_score: 82,
  },
  {
    id: 'gemini',
    label: 'Google Gemini (OpenAI Compat)',
    name: 'gemini',
    base_url: 'https://generativelanguage.googleapis.com/v1beta/openai/',
    api_key_template: '${GEMINI_API_KEY}',
    models: [
      'gemini-2.5-flash',
      'gemini-2.5-pro',
      'gemini-1.5-flash',
    ],
    rpm: 15,
    min_interval_ms: 4000,
    max_concurrency: 1,
    smartness_score: 92,
  },
  {
    id: 'nvidia',
    label: 'NVIDIA NIM (Free Cloud Endpoints)',
    name: 'nvidia',
    base_url: 'https://integrate.api.nvidia.com/v1',
    api_key_template: '${NVIDIA_TOKEN}',
    models: [
      'deepseek-ai/deepseek-v4.1-flash',
      'z-ai/glm-5.3-flash',
      'meta/llama-3.3-70b-instruct',
      'nvidia/llama-3.1-nemotron-70b-instruct',
    ],
    rpm: 40,
    min_interval_ms: 1500,
    max_concurrency: 3,
    smartness_score: 94,
  },
  {
    id: 'groq',
    label: 'Groq Cloud (LPU Inference)',
    name: 'groq',
    base_url: 'https://api.groq.com/openai/v1',
    api_key_template: '${GROQ_API_KEY}',
    models: [
      'llama-3.3-70b-versatile',
      'llama-3.1-8b-instant',
      'mixtral-8x7b-32768',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 90,
  },
  {
    id: 'cerebras',
    label: 'Cerebras Inference',
    name: 'cerebras',
    base_url: 'https://api.cerebras.ai/v1',
    api_key_template: '${CEREBRAS_API_KEY}',
    models: [
      'llama3.1-70b',
      'llama3.1-8b',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 89,
  },
  {
    id: 'sambanova',
    label: 'SambaNova Cloud',
    name: 'sambanova',
    base_url: 'https://api.sambanova.ai/v1',
    api_key_template: '${SAMBANOVA_API_KEY}',
    models: [
      'Meta-Llama-3.3-70B-Instruct',
      'Qwen2.5-72B-Instruct',
    ],
    rpm: 20,
    min_interval_ms: 3000,
    max_concurrency: 2,
    smartness_score: 88,
  },
  {
    id: 'mistral',
    label: 'Mistral AI (Official)',
    name: 'mistral',
    base_url: 'https://api.mistral.ai/v1',
    api_key_template: '${MISTRAL_API_KEY}',
    models: [
      'mistral-large-latest',
      'mistral-small-latest',
      'codestral-latest',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 91,
  },
  {
    id: 'together',
    label: 'Together AI',
    name: 'together',
    base_url: 'https://api.together.xyz/v1',
    api_key_template: '${TOGETHER_API_KEY}',
    models: [
      'meta-llama/Llama-3.3-70B-Instruct-Turbo',
      'deepseek-ai/DeepSeek-V3',
      'Qwen/Qwen2.5-72B-Instruct-Turbo',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 89,
  },
  {
    id: 'fireworks',
    label: 'Fireworks AI',
    name: 'fireworks',
    base_url: 'https://api.fireworks.ai/inference/v1',
    api_key_template: '${FIREWORKS_API_KEY}',
    models: [
      'accounts/fireworks/models/llama-v3p3-70b-instruct',
      'accounts/fireworks/models/qwen2p5-72b-instruct',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 89,
  },
  {
    id: 'novita',
    label: 'Novita AI',
    name: 'novita',
    base_url: 'https://api.novita.ai/v3/openai',
    api_key_template: '${NOVITA_API_KEY}',
    models: [
      'meta-llama/llama-3.3-70b-instruct',
      'deepseek/deepseek-r1',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 87,
  },
  {
    id: 'siliconflow',
    label: 'SiliconFlow (Free & Paid Tiers)',
    name: 'siliconflow',
    base_url: 'https://api.siliconflow.cn/v1',
    api_key_template: '${SILICONFLOW_API_KEY}',
    models: [
      'deepseek-ai/DeepSeek-V3',
      'deepseek-ai/DeepSeek-R1',
      'Qwen/Qwen2.5-72B-Instruct',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 88,
  },
  {
    id: 'volcengine',
    label: 'Volcengine Ark (Doubao / DeepSeek)',
    name: 'volcengine',
    base_url: 'https://ark.cn-beijing.volces.com/api/v3',
    api_key_template: '${ARK_API_KEY}',
    models: [
      'ep-20250210123456-abcde',
    ],
    rpm: 60,
    min_interval_ms: 1000,
    max_concurrency: 5,
    smartness_score: 90,
  },
  {
    id: 'cohere',
    label: 'Cohere (V2 Chat)',
    name: 'cohere',
    base_url: 'https://api.cohere.ai/v2',
    api_key_template: '${CO_API_KEY}',
    models: [
      'command-r-plus-08-2024',
      'command-r-08-2024',
    ],
    rpm: 20,
    min_interval_ms: 3000,
    max_concurrency: 2,
    smartness_score: 88,
  },
  {
    id: 'ollama',
    label: 'Local Ollama Instance',
    name: 'ollama',
    base_url: 'http://127.0.0.1:11434/v1',
    api_key_template: 'ollama',
    models: [
      'llama3.2:3b',
      'deepseek-r1:8b',
      'qwen2.5:7b',
    ],
    rpm: 120,
    min_interval_ms: 100,
    max_concurrency: 4,
    smartness_score: 75,
  },
  {
    id: 'vllm',
    label: 'Local vLLM / SGLang Server',
    name: 'vllm',
    base_url: 'http://127.0.0.1:8000/v1',
    api_key_template: 'token-abc123',
    models: [
      'default',
    ],
    rpm: 300,
    min_interval_ms: 50,
    max_concurrency: 8,
    smartness_score: 85,
  },
  {
    id: 'custom',
    label: 'Custom OpenAI-Compatible Provider',
    name: 'custom',
    base_url: 'https://your-api-endpoint.com/v1',
    api_key_template: '${CUSTOM_API_KEY}',
    models: ['model-1'],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 80,
  },
];

const VAULT_PRESETS = [
  { name: 'NVIDIA_API_KEY', label: 'NVIDIA NIM', desc: 'NVIDIA NIM API key (integrate.api.nvidia.com)' },
  { name: 'GROQ_API_KEY', label: 'GroqCloud', desc: 'GroqCloud ultra-fast LPU API key' },
  { name: 'OPENROUTER_TOKEN', label: 'OpenRouter', desc: 'OpenRouter unified token' },
  { name: 'GEMINI_API_KEY', label: 'Google Gemini', desc: 'Google Gemini Studio API key' },
  { name: 'DEEPSEEK_API_KEY', label: 'DeepSeek', desc: 'DeepSeek official API key' },
  { name: 'ANTHROPIC_API_KEY', label: 'Anthropic', desc: 'Anthropic Claude API key' },
  { name: 'OPENAI_API_KEY', label: 'OpenAI', desc: 'OpenAI platform API key' },
];

export const ProvidersStudio: React.FC<ProvidersStudioProps> = ({ status, onRefresh }) => {
  const [editingProvider, setEditingProvider] = useState<string | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [selectedPresetId, setSelectedPresetId] = useState<string>('deepseek');
  const [searchQuery, setSearchQuery] = useState('');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Central Vault State
  const [vaultSecrets, setVaultSecrets] = useState<VaultSecretRecord[]>([]);
  const [activeSubTab, setActiveSubTab] = useState<'providers' | 'vault'>('providers');
  const [isVaultModalOpen, setIsVaultModalOpen] = useState(false);
  const [vaultSecretName, setVaultSecretName] = useState('');
  const [vaultSecretValue, setVaultSecretValue] = useState('');
  const [vaultSecretDesc, setVaultSecretDesc] = useState('');
  const [showSecretValue, setShowSecretValue] = useState(false);
  const [isSavingVaultSecret, setIsSavingVaultSecret] = useState(false);

  const loadVaultSecrets = async () => {
    try {
      const data = await fetchVaultSecrets();
      setVaultSecrets(data);
    } catch (err) {
      console.error('Failed to load vault secrets:', err);
    }
  };

  useEffect(() => {
    loadVaultSecrets();
  }, []);

  const openAddVaultModal = (preset?: { name: string; desc: string }) => {
    setVaultSecretName(preset ? preset.name : '');
    setVaultSecretValue('');
    setVaultSecretDesc(preset ? preset.desc : '');
    setShowSecretValue(false);
    setIsVaultModalOpen(true);
  };

  const handleSaveVaultSecret = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!vaultSecretName.trim() || !vaultSecretValue.trim()) {
      alert('Secret name and value are required');
      return;
    }
    setIsSavingVaultSecret(true);
    try {
      await createVaultSecret({
        name: vaultSecretName.trim().toUpperCase().replace(/-/g, '_'),
        value: vaultSecretValue.trim(),
        description: vaultSecretDesc.trim() || undefined,
      });
      setIsVaultModalOpen(false);
      await loadVaultSecrets();
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to save secret to vault');
    } finally {
      setIsSavingVaultSecret(false);
    }
  };

  const handleDeleteVaultSecret = async (name: string) => {
    if (!confirm(`Are you sure you want to delete secret "${name}" from the central Vault?`)) return;
    try {
      await deleteVaultSecret(name);
      await loadVaultSecrets();
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to delete secret');
    }
  };

  const [formData, setFormData] = useState<ProviderInput>({
    name: '',
    enabled: true,
    base_url: '',
    api_key_template: '${API_KEY}',
    raw_api_key: '',
    models: [],
    headers: {},
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 80,
  });

  const [apiKeyValue, setApiKeyValue] = useState<string>('');
  const [existingEncryptedKeyPreview, setExistingEncryptedKeyPreview] = useState<string | null>(null);
  const [modelsInput, setModelsInput] = useState('');
  const [testResults, setTestResults] = useState<Record<string, TestConnectionResponse & { testing?: boolean }>>({});

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(id);
    setTimeout(() => setCopiedKey(null), 1500);
  };

  const applyPreset = (presetId: string) => {
    setSelectedPresetId(presetId);
    const preset = PRESETS.find((p) => p.id === presetId);
    if (!preset) return;

    setFormData((prev) => ({
      ...prev,
      name: editingProvider ? prev.name : preset.name,
      base_url: preset.base_url,
      api_key_template: preset.api_key_template,
      models: preset.models,
      rpm: preset.rpm,
      min_interval_ms: preset.min_interval_ms,
      max_concurrency: preset.max_concurrency,
      smartness_score: preset.smartness_score,
    }));
    setApiKeyValue(preset.api_key_template);
    setExistingEncryptedKeyPreview(null);
    setModelsInput(preset.models.join('\n'));
  };

  const openAddModal = () => {
    setEditingProvider(null);
    setSelectedPresetId('deepseek');
    const p = PRESETS[0];
    setFormData({
      name: p.name,
      enabled: true,
      base_url: p.base_url,
      api_key_template: p.api_key_template,
      raw_api_key: '',
      models: p.models,
      headers: {},
      rpm: p.rpm,
      min_interval_ms: p.min_interval_ms,
      max_concurrency: p.max_concurrency,
      smartness_score: p.smartness_score,
    });
    setApiKeyValue(p.api_key_template);
    setExistingEncryptedKeyPreview(null);
    setModelsInput(p.models.join('\n'));
    setIsModalOpen(true);
  };

  const openEditModal = (p: any) => {
    setEditingProvider(p.name);
    const matchingPreset = PRESETS.find((item) => item.name === p.name);
    setSelectedPresetId(matchingPreset ? matchingPreset.id : 'custom');

    setExistingEncryptedKeyPreview(p.has_encrypted_key ? (p.key_preview || '••••••••') : null);
    setApiKeyValue(p.has_encrypted_key ? '' : (p.api_key_template || ''));

    setFormData({
      name: p.name,
      enabled: p.enabled,
      base_url: p.base_url,
      api_key_template: p.api_key_template || '${API_KEY}',
      raw_api_key: '',
      models: p.models,
      headers: {},
      rpm: p.max_rpm,
      min_interval_ms: p.min_interval_ms,
      max_concurrency: 2,
      smartness_score: p.smartness_score,
    });
    setModelsInput(p.models.join('\n'));
    setIsModalOpen(true);
  };

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    const models = modelsInput
      .split('\n')
      .map((s) => s.trim())
      .filter((s) => s.length > 0);

    const trimmedKey = apiKeyValue.trim();
    let finalApiKeyTemplate = formData.api_key_template.trim();
    let rawApiKey: string | undefined = undefined;

    if (trimmedKey.startsWith('$')) {
      finalApiKeyTemplate = trimmedKey;
      rawApiKey = trimmedKey;
    } else if (trimmedKey.length > 0) {
      rawApiKey = trimmedKey;
      if (!finalApiKeyTemplate.startsWith('$')) {
        finalApiKeyTemplate = `\${${formData.name.toUpperCase().replace(/-/g, '_')}_API_KEY}`;
      }
    } else if (existingEncryptedKeyPreview) {
      rawApiKey = undefined;
    }

    const payload: ProviderInput = {
      ...formData,
      api_key_template: finalApiKeyTemplate,
      raw_api_key: rawApiKey,
      models,
    };

    try {
      if (editingProvider) {
        await updateProvider(editingProvider, payload);
      } else {
        await createProvider(payload);
      }
      setIsModalOpen(false);
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to save provider');
    }
  };

  const handleDelete = async (name: string) => {
    if (!confirm(`Are you sure you want to delete provider "${name}"?`)) return;
    try {
      await deleteProvider(name);
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to delete provider');
    }
  };

  const handleTest = async (name: string) => {
    setTestResults((prev) => ({
      ...prev,
      [name]: { success: false, status_code: 0, latency_ms: 0, message: 'Testing...', testing: true },
    }));

    try {
      const res = await testProvider(name);
      setTestResults((prev) => ({
        ...prev,
        [name]: { ...res, testing: false },
      }));
    } catch (err: any) {
      setTestResults((prev) => ({
        ...prev,
        [name]: {
          success: false,
          status_code: 0,
          latency_ms: 0,
          message: 'Error testing provider',
          error: err.message,
          testing: false,
        },
      }));
    }
  };

  function getDomain(urlStr: string): string {
    try {
      const url = new URL(urlStr);
      return url.hostname;
    } catch {
      return urlStr.replace(/^https?:\/\//, '').split('/')[0] || 'custom';
    }
  }

  const filteredProviders = (status?.providers || []).filter((p) => {
    const q = searchQuery.toLowerCase();
    return (
      p.name.toLowerCase().includes(q) ||
      p.base_url.toLowerCase().includes(q) ||
      (p.api_key_template && p.api_key_template.toLowerCase().includes(q)) ||
      p.models.some((m) => m.toLowerCase().includes(q))
    );
  });

  const groupedProviders = useMemo(() => {
    const groups: Record<string, typeof filteredProviders> = {};
    for (const p of filteredProviders) {
      const domain = getDomain(p.base_url);
      if (!groups[domain]) {
        groups[domain] = [];
      }
      groups[domain].push(p);
    }
    return Object.entries(groups).sort((a, b) => a[0].localeCompare(b[0]));
  }, [filteredProviders]);

  const isEnvVarKey = apiKeyValue.trim().startsWith('$');
  const isDirectSecretKey = apiKeyValue.trim().length > 0 && !isEnvVarKey;

  return (
    <div className="space-y-6">
      {/* Subtab Toggle: Providers vs Vault */}
      <div className="flex items-center gap-2 border-b border-white/[0.08] pb-3">
        <button
          type="button"
          onClick={() => setActiveSubTab('providers')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-mono font-medium transition-all ${
            activeSubTab === 'providers'
              ? 'bg-[var(--primary)] text-slate-900 font-bold shadow-[0_0_15px_var(--primary-glow-subtle)]'
              : 'text-slate-400 hover:text-white hover:bg-white/[0.04]'
          }`}
        >
          <Server className="w-3.5 h-3.5" />
          <span>Configured Providers ({status?.providers.length || 0})</span>
        </button>

        <button
          type="button"
          onClick={() => setActiveSubTab('vault')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-mono font-medium transition-all ${
            activeSubTab === 'vault'
              ? 'bg-[var(--primary)] text-slate-900 font-bold shadow-[0_0_15px_var(--primary-glow-subtle)]'
              : 'text-slate-400 hover:text-white hover:bg-white/[0.04]'
          }`}
        >
          <ShieldCheck className="w-3.5 h-3.5" />
          <span>Central Vault Secrets ({vaultSecrets.length})</span>
        </button>
      </div>

      {activeSubTab === 'vault' ? (
        <div className="space-y-6">
          {/* Vault Action Bar */}
          <Card className="p-5 flex flex-col md:flex-row md:items-center justify-between gap-4">
            <div>
              <h2 className="text-base font-bold text-white flex items-center gap-2 font-mono">
                <ShieldCheck className="w-4 h-4 text-emerald-400" />
                CENTRAL ENCRYPTED VAULT
              </h2>
              <p className="text-xs text-slate-400 mt-1">
                Store upstream LLM credentials once with <code className="text-emerald-400 font-mono">AES-256-GCM</code> authenticated encryption. Reusable across multiple providers via <code className="text-amber-400 font-mono">${'{SECRET_NAME}'}</code> without copy-pasting.
              </p>
            </div>

            <Button
              variant="primary"
              size="md"
              onClick={() => openAddVaultModal()}
            >
              <Plus className="w-4 h-4" />
              Add Secret to Vault
            </Button>
          </Card>

          {/* Quick Presets Bar */}
          <div className="bg-[#050814] border border-white/[0.06] rounded-xl p-3 flex flex-wrap items-center gap-2">
            <span className="text-xs font-mono text-slate-400 flex items-center gap-1.5 mr-1">
              <Sparkles className="w-3.5 h-3.5 text-[var(--secondary)]" /> Quick Presets:
            </span>
            {VAULT_PRESETS.map((p) => {
              const alreadyExists = vaultSecrets.some((s) => s.name === p.name);
              return (
                <button
                  key={p.name}
                  type="button"
                  onClick={() => openAddVaultModal(p)}
                  className={`px-2.5 py-1 rounded-lg text-xs font-mono transition-all flex items-center gap-1.5 border ${
                    alreadyExists
                      ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
                      : 'bg-white/[0.03] border-white/10 text-slate-300 hover:text-white hover:border-[var(--primary)] hover:bg-[var(--primary)]/10'
                  }`}
                >
                  {alreadyExists ? <Check className="w-3 h-3 text-emerald-400" /> : <Plus className="w-3 h-3 text-slate-400" />}
                  <span>{p.label}</span>
                </button>
              );
            })}
          </div>

          {/* Vault Secrets Grid */}
          {vaultSecrets.length === 0 ? (
            <div className="bg-[#04060c] border border-white/[0.06] rounded-2xl p-12 text-center space-y-4">
              <div className="w-12 h-12 rounded-2xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 flex items-center justify-center mx-auto">
                <Lock className="w-6 h-6" />
              </div>
              <div className="space-y-1">
                <h3 className="text-sm font-bold text-white font-mono">No Vault Secrets Stored Yet</h3>
                <p className="text-xs text-slate-400 max-w-md mx-auto">
                  Add an API key once (e.g. NVIDIA, Groq, OpenRouter), and you can reuse it across multiple providers and virtual models without copy-pasting.
                </p>
              </div>
              <Button variant="primary" size="sm" onClick={() => openAddVaultModal()}>
                <Plus className="w-3.5 h-3.5" />
                Add Your First Secret
              </Button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {vaultSecrets.map((s) => {
                const templateRef = `\${${s.name}}`;
                return (
                  <Card key={s.name} className="p-4 flex flex-col justify-between space-y-3 relative group">
                    <div className="space-y-2">
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2">
                          <div className="w-7 h-7 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 flex items-center justify-center">
                            <Lock className="w-3.5 h-3.5" />
                          </div>
                          <span className="font-mono font-bold text-xs text-white tracking-wide truncate max-w-[180px]">
                            {s.name}
                          </span>
                        </div>
                        <button
                          type="button"
                          onClick={() => copyToClipboard(templateRef, s.name)}
                          className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-white/[0.05] hover:bg-white/[0.1] text-slate-300 hover:text-white border border-white/10 flex items-center gap-1 transition-all"
                          title="Copy template variable for providers"
                        >
                          {copiedKey === s.name ? <Check className="w-2.5 h-2.5 text-emerald-400" /> : <Copy className="w-2.5 h-2.5" />}
                          <span>{templateRef}</span>
                        </button>
                      </div>

                      <div className="bg-[#030508] border border-white/[0.06] rounded-lg p-2 flex items-center justify-between text-xs font-mono">
                        <span className="text-slate-400">Masked Key:</span>
                        <span className="text-emerald-400 font-semibold tracking-wider">{s.key_preview || '••••••••••••'}</span>
                      </div>

                      {s.description && (
                        <p className="text-[11px] text-slate-400 leading-snug line-clamp-2">
                          {s.description}
                        </p>
                      )}
                    </div>

                    <div className="pt-2 border-t border-white/[0.06] flex items-center justify-between text-[10px] text-slate-500 font-mono">
                      <span>Updated {s.updated_at?.split('T')[0] || s.created_at?.split('T')[0] || 'Recently'}</span>
                      <button
                        type="button"
                        onClick={() => handleDeleteVaultSecret(s.name)}
                        className="text-slate-500 hover:text-rose-400 p-1 rounded hover:bg-rose-500/10 transition-colors"
                        title="Delete from Vault"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </Card>
                );
              })}
            </div>
          )}
        </div>
      ) : (
        <div className="space-y-6">
          {/* Action Bar */}
          <Card className="p-5 flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h2 className="text-base font-bold text-white flex items-center gap-2 font-mono">
            <Server className="w-4 h-4 text-[var(--primary)]" />
            PROVIDERS STUDIO &amp; VAULT
          </h2>
          <p className="text-xs text-slate-400 mt-1">
            Configure upstream LLM APIs. Supports direct <code className="text-emerald-400 font-mono">AES-256-GCM</code> salted vault keys or dynamic shell variables (<code className="text-amber-400 font-mono">${'{VAR}'}</code>).
          </p>
        </div>

        <Button
          variant="primary"
          size="md"
          onClick={openAddModal}
        >
          <Plus className="w-4 h-4" />
          Add Provider
        </Button>
      </Card>

      {/* Filter / Search Bar */}
      <div className="flex items-center justify-between gap-4">
        <div className="relative flex-1 max-w-md">
          <Search className="w-4 h-4 text-slate-500 absolute left-3 top-2.5 pointer-events-none" />
          <Input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Filter by name, URL, model, or ${ENV_VAR}..."
            className="pl-9"
            mono
          />
        </div>
        <div className="text-xs text-slate-400 font-mono">
          {filteredProviders.length} of {status?.providers.length || 0} providers
        </div>
      </div>

      {/* Grouped by Domain List */}
      <div className="space-y-4">
        {groupedProviders.map(([domain, providers]) => (
          <Card key={domain} className="overflow-hidden">
            {/* Domain Group Header */}
            <div className="bg-[#04060b] px-4 py-2.5 border-b border-white/[0.05] flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Globe className="w-4 h-4 text-[var(--primary)]" />
                <span className="font-mono text-xs font-bold text-slate-200 tracking-wide">
                  {domain}
                </span>
                <Badge variant="neutral">
                  {providers.length} {providers.length === 1 ? 'provider' : 'providers'}
                </Badge>
              </div>
            </div>

            {/* Providers under this domain */}
            <div className="divide-y divide-white/[0.04]">
              {providers.map((p) => {
                const test = testResults[String(p.name)];
                const keyTemplate = p.api_key_template || '${API_KEY}';

                return (
                  <div
                    key={String(p.name)}
                    className="p-4 hover:bg-white/[0.02] transition-colors flex flex-col lg:flex-row lg:items-center justify-between gap-4 border-b border-white/[0.04] last:border-b-0"
                  >
                    {/* Column 1: Identity & Key */}
                    <div className="min-w-[280px] max-w-sm space-y-1.5">
                      <div className="flex items-center gap-2">
                        <span
                          className={`w-2.5 h-2.5 rounded-full ${
                            p.enabled ? 'bg-[var(--primary)] shadow-[0_0_8px_var(--primary)]' : 'bg-slate-700'
                          }`}
                          title={p.enabled ? 'Enabled' : 'Disabled'}
                        />
                        <h3 className="text-sm font-bold text-white font-mono">{p.name}</h3>
                        <Badge variant="neutral">{p.circuit}</Badge>
                      </div>

                      <p className="text-xs text-slate-400 font-mono truncate" title={p.base_url}>
                        {p.base_url}
                      </p>

                      {/* API Key Badge: Vault Secret or Env Var */}
                      <div className="flex items-center gap-1.5 pt-0.5">
                        {p.has_encrypted_key ? (
                          <Badge
                            variant="primary"
                            className="px-2.5 py-1"
                            title="Stored securely in ~/.wirellm/ using AES-256-GCM + Salt"
                          >
                            <Lock className="w-3 h-3 text-[var(--primary)] flex-shrink-0" />
                            <span>Vault Key: {p.key_preview || '••••••••'}</span>
                          </Badge>
                        ) : (
                          <div
                            className="inline-flex items-center gap-1.5 bg-amber-950/30 text-amber-300 border border-amber-800/40 font-mono text-[11px] px-2.5 py-0.5 rounded cursor-pointer hover:border-amber-700 transition-colors"
                            title="Click to copy env variable name"
                            onClick={() => copyToClipboard(keyTemplate, String(p.name))}
                          >
                            <Key className="w-3 h-3 text-amber-400 flex-shrink-0" />
                            <span>{keyTemplate}</span>
                            {copiedKey === String(p.name) ? (
                              <Check className="w-3 h-3 text-emerald-400 ml-1" />
                            ) : (
                              <Copy className="w-3 h-3 text-amber-400/60 ml-1 hover:text-amber-300" />
                            )}
                          </div>
                        )}
                      </div>
                    </div>

                    {/* Column 2: Models & Specifications */}
                    <div className="flex-1 space-y-2">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="px-2 py-0.5 rounded text-[11px] font-mono font-medium bg-[#090d16] text-slate-300 border border-white/[0.06] flex items-center gap-1">
                          <Sparkles className="w-3 h-3 text-amber-400" />
                          Score: {p.smartness_score}
                        </span>
                        {p.max_rpm && (
                          <span className="px-2 py-0.5 rounded text-[11px] font-mono font-medium bg-[#090d16] text-slate-300 border border-white/[0.06] flex items-center gap-1">
                            <Clock className="w-3 h-3 text-sky-400" />
                            {p.max_rpm} RPM
                          </span>
                        )}
                        {p.min_interval_ms && (
                          <span className="px-2 py-0.5 rounded text-[11px] font-mono font-medium bg-[#090d16] text-slate-300 border border-white/[0.06] flex items-center gap-1">
                            <Sliders className="w-3 h-3 text-emerald-400" />
                            {p.min_interval_ms}ms burst
                          </span>
                        )}
                        <span className="text-[11px] text-slate-500 font-mono">
                          {p.models.length} {p.models.length === 1 ? 'model' : 'models'}
                        </span>
                      </div>

                      {/* Models list */}
                      <div className="flex items-center gap-1.5 flex-wrap max-h-16 overflow-y-auto pr-1">
                        {p.models.map((m) => (
                          <span
                            key={m}
                            className="text-[11px] font-mono bg-[#030509] border border-white/[0.06] px-2 py-0.5 rounded text-slate-300"
                            title={m}
                          >
                            {m}
                          </span>
                        ))}
                      </div>
                    </div>

                    {/* Column 3: Testing & Action Controls */}
                    <div className="flex items-center gap-3 lg:self-center justify-between lg:justify-end border-t lg:border-t-0 pt-3 lg:pt-0 border-white/[0.06]">
                      {/* Test Connection Button */}
                      <div className="flex items-center gap-2">
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleTest(String(p.name))}
                          disabled={test?.testing}
                        >
                          {test?.testing ? (
                            <Activity className="w-3 h-3 animate-spin text-[var(--primary)]" />
                          ) : (
                            <Play className="w-3 h-3 text-[var(--primary)]" />
                          )}
                          Test
                        </Button>

                        {test && !test.testing && (
                          test.success ? (
                            <Badge variant="primary" className="px-2 py-1">
                              <CheckCircle className="w-3.5 h-3.5 text-[var(--primary)]" />
                              {test.latency_ms} ms
                            </Badge>
                          ) : (
                            <Badge
                              variant="rose"
                              className="px-2 py-1 cursor-help"
                              title={test.error || test.message}
                            >
                              <XCircle className="w-3.5 h-3.5" />
                              Failed
                            </Badge>
                          )
                        )}
                      </div>

                      {/* Edit / Delete Buttons */}
                      <div className="flex items-center gap-1 pl-2 border-l border-white/[0.06]">
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => openEditModal(p)}
                          title="Edit Provider"
                        >
                          <Edit className="w-4 h-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => handleDelete(String(p.name))}
                          title="Delete Provider"
                        >
                          <Trash2 className="w-4 h-4 text-slate-400 hover:text-rose-400" />
                        </Button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </Card>
        ))}

        {groupedProviders.length === 0 && (
          <Card className="p-12 text-center text-slate-500 font-mono text-xs">
            No providers found matching "{searchQuery}".
          </Card>
        )}
      </div>
    </div>
  )}

      {/* Edit/Add Modal */}
      <Modal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        title={editingProvider ? `Configure Provider "${editingProvider}"` : 'Add New Upstream Provider'}
        description="WireLLM auto-normalizes OpenAI chat completions format with intelligent failover."
        icon={<Server className="w-4 h-4 text-[var(--primary)]" />}
        footer={
          <>
            <Button variant="secondary" onClick={() => setIsModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={handleSave}>
              Save Provider
            </Button>
          </>
        }
      >
        <form onSubmit={handleSave} className="space-y-4">
          {/* Preset Picker */}
          <div>
            <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">
              Quick Setup Preset (Recommended)
            </label>
            <Select
              value={selectedPresetId}
              onChange={(e) => applyPreset(e.target.value)}
              className="w-full"
              mono
            >
              {PRESETS.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.label}
                </option>
              ))}
            </Select>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">
                Provider Name (Slug)
              </label>
              <Input
                type="text"
                required
                disabled={!!editingProvider}
                value={formData.name}
                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                placeholder="e.g. deepseek, groq"
                mono
              />
            </div>

            <div>
              <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">Status</label>
              <Select
                value={formData.enabled ? 'true' : 'false'}
                onChange={(e) => setFormData({ ...formData, enabled: e.target.value === 'true' })}
                className="w-full"
                mono
              >
                <option value="true">Enabled (Active in routing)</option>
                <option value="false">Disabled (Standby)</option>
              </Select>
            </div>
          </div>

          <div>
            <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">
              Base URL (OpenAI-compatible /v1 endpoint)
            </label>
            <Input
              type="text"
              required
              value={formData.base_url}
              onChange={(e) => setFormData({ ...formData, base_url: e.target.value })}
              placeholder="https://api.openai.com/v1"
              mono
            />
          </div>

          {/* Smart API Key / Vault Configuration */}
          <div className="bg-[#030508] border border-white/[0.06] rounded-xl p-3.5 space-y-2">
            <div className="flex items-center justify-between">
              <label className="block text-xs font-mono font-medium text-slate-200">
                API Key or Secret Vault
              </label>
              <Badge variant="primary">AES-256-GCM VAULT</Badge>
            </div>

            <div>
              <Input
                type="password"
                value={apiKeyValue}
                onChange={(e) => setApiKeyValue(e.target.value)}
                placeholder={
                  existingEncryptedKeyPreview
                    ? `Encrypted (${existingEncryptedKeyPreview}) - Leave empty to keep`
                    : 'Paste raw secret key (sk-...) OR $ENV_VAR reference'
                }
                mono
              />
            </div>

            {/* Dynamic Smart Key Mode Info Badge */}
            <div className="mt-1.5">
              {isEnvVarKey && (
                <div className="flex items-center gap-1.5 text-[11px] text-amber-300 bg-amber-950/30 border border-amber-900/40 rounded px-2.5 py-1">
                  <Key className="w-3.5 h-3.5 text-amber-400 flex-shrink-0" />
                  <span>
                    <strong>Dynamic Env Variable:</strong> Evaluated at runtime from shell environment or <code className="text-amber-200 font-mono">~/.wirellm/.env</code>.
                  </span>
                </div>
              )}

              {isDirectSecretKey && (
                <div className="flex items-center gap-1.5 text-[11px] text-emerald-300 bg-emerald-950/30 border border-emerald-900/40 rounded px-2.5 py-1">
                  <ShieldCheck className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
                  <span>
                    <strong>Direct Secret Key:</strong> Stored securely with <code className="text-emerald-200 font-mono">AES-256-GCM</code> + local salt in <code className="text-emerald-200 font-mono">~/.wirellm/</code>. Never exposed in plaintext.
                  </span>
                </div>
              )}

              {!isEnvVarKey && !isDirectSecretKey && existingEncryptedKeyPreview && (
                <div className="flex items-center gap-1.5 text-[11px] text-slate-300 bg-[#04060c] border border-white/[0.06] rounded px-2.5 py-1">
                  <Lock className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
                  <span>
                    Current vault key: <code className="text-emerald-300 font-mono">•••••••••••• (Encrypted)</code>. Enter a new key or <code className="text-amber-300 font-mono">$VAR</code> to update.
                  </span>
                </div>
              )}

              {!isEnvVarKey && !isDirectSecretKey && !existingEncryptedKeyPreview && (
                <span className="text-[11px] text-slate-500 block">
                  Tip: Enter <code className="text-amber-400 font-mono">$VAR</code> to read from environment, or paste your raw secret key to encrypt it in the local vault.
                </span>
              )}
            </div>

            {vaultSecrets.length > 0 && (
              <div className="pt-2 border-t border-white/[0.06] space-y-1.5">
                <div className="flex items-center justify-between text-[11px] font-mono text-slate-400">
                  <span className="flex items-center gap-1.5 text-slate-300">
                    <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
                    <span>Select from Central Vault:</span>
                  </span>
                  <span className="text-[10px] text-slate-500">1-click link</span>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {vaultSecrets.map((s) => (
                    <button
                      key={s.name}
                      type="button"
                      onClick={() => {
                        setApiKeyValue(`\${${s.name}}`);
                        setExistingEncryptedKeyPreview(null);
                      }}
                      className="px-2 py-1 rounded-md text-[11px] font-mono bg-white/[0.04] hover:bg-emerald-500/20 text-slate-300 hover:text-emerald-300 border border-white/10 hover:border-emerald-500/50 transition-all flex items-center gap-1.5 shadow-sm"
                    >
                      <Lock className="w-3 h-3 text-emerald-400 shrink-0" />
                      <span>{`\${${s.name}}`}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>

          <div>
            <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">
              Models (one per line)
            </label>
            <textarea
              rows={3}
              required
              value={modelsInput}
              onChange={(e) => setModelsInput(e.target.value)}
              placeholder="deepseek-chat&#10;deepseek-reasoner"
              className="w-full bg-[#030508] border border-white/[0.08] focus:border-[var(--primary)] rounded-lg px-3 py-2 text-xs text-white font-mono focus:outline-none transition-colors"
            />
          </div>

          <div className="grid grid-cols-3 gap-3">
            <div>
              <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">RPM Limit</label>
              <Input
                type="number"
                value={formData.rpm || ''}
                onChange={(e) =>
                  setFormData({
                    ...formData,
                    rpm: e.target.value ? parseInt(e.target.value) : undefined,
                  })
                }
                placeholder="30"
                mono
              />
            </div>

            <div>
              <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">Burst Spacing (ms)</label>
              <Input
                type="number"
                value={formData.min_interval_ms || ''}
                onChange={(e) =>
                  setFormData({
                    ...formData,
                    min_interval_ms: e.target.value ? parseInt(e.target.value) : undefined,
                  })
                }
                placeholder="2000"
                mono
              />
            </div>

            <div>
              <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">Smartness (0-100)</label>
              <Input
                type="number"
                value={formData.smartness_score || ''}
                onChange={(e) =>
                  setFormData({
                    ...formData,
                    smartness_score: e.target.value ? parseInt(e.target.value) : undefined,
                  })
                }
                placeholder="85"
                mono
              />
            </div>
          </div>
        </form>
      </Modal>

      {/* Add Secret to Vault Modal */}
      <Modal
        isOpen={isVaultModalOpen}
        onClose={() => setIsVaultModalOpen(false)}
        title="Add Secret to Central Vault"
        description="Store once with AES-256-GCM authenticated encryption. Reusable across any provider."
        icon={<Lock className="w-4 h-4 text-emerald-400" />}
        footer={
          <>
            <Button variant="secondary" onClick={() => setIsVaultModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={handleSaveVaultSecret} disabled={isSavingVaultSecret}>
              {isSavingVaultSecret ? 'Encrypting & Saving...' : 'Save to Vault'}
            </Button>
          </>
        }
      >
        <form onSubmit={handleSaveVaultSecret} className="space-y-4">
          <div>
            <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">
              Secret Name / Identifier
            </label>
            <Input
              type="text"
              required
              value={vaultSecretName}
              onChange={(e) => setVaultSecretName(e.target.value.toUpperCase().replace(/[\s-]/g, '_'))}
              placeholder="e.g. NVIDIA_API_KEY, GROQ_API_KEY"
              mono
            />
            <p className="text-[10px] text-slate-500 font-mono mt-1">
              Providers can reference this secret as <code className="text-amber-400">${'{' + (vaultSecretName || 'NAME') + '}'}</code>.
            </p>
          </div>

          <div>
            <div className="flex items-center justify-between mb-1.5">
              <label className="block text-xs font-mono font-medium text-slate-300">
                Secret Value (Token / Key)
              </label>
              <button
                type="button"
                onClick={() => setShowSecretValue(!showSecretValue)}
                className="text-[11px] font-mono text-slate-400 hover:text-white flex items-center gap-1"
              >
                {showSecretValue ? <EyeOff className="w-3 h-3" /> : <Eye className="w-3 h-3" />}
                <span>{showSecretValue ? 'Hide' : 'Show'}</span>
              </button>
            </div>
            <Input
              type={showSecretValue ? 'text' : 'password'}
              required
              value={vaultSecretValue}
              onChange={(e) => setVaultSecretValue(e.target.value)}
              placeholder="nvapi-..., gsk_..., or Bearer token"
              mono
            />
          </div>

          <div>
            <label className="block text-xs font-mono font-medium text-slate-300 mb-1.5">
              Description (Optional)
            </label>
            <Input
              type="text"
              value={vaultSecretDesc}
              onChange={(e) => setVaultSecretDesc(e.target.value)}
              placeholder="e.g. Primary NVIDIA NIM inference key"
            />
          </div>
        </form>
      </Modal>
    </div>
  );
};
