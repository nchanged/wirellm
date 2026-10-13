import React, { useState, useMemo } from 'react';
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
} from 'lucide-react';
import { ApiStatusResponse, ProviderInput, TestConnectionResponse } from '../types';
import { createProvider, updateProvider, deleteProvider, testProvider } from '../api';

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
    max_concurrency: 2,
    smartness_score: 92,
  },
  {
    id: 'groq',
    label: 'Groq (Ultra-fast LPU Inference)',
    name: 'groq',
    base_url: 'https://api.groq.com/openai/v1',
    api_key_template: '${GROQ_API_KEY}',
    models: [
      'llama-3.3-70b-versatile',
      'llama-3.1-8b-instant',
      'openai/gpt-oss-120b',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 84,
  },
  {
    id: 'cerebras',
    label: 'Cerebras (Fast Inference)',
    name: 'cerebras',
    base_url: 'https://api.cerebras.ai/v1',
    api_key_template: '${CEREBRAS_API_KEY}',
    models: [
      'llama3.3-70b',
      'llama3.1-8b',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 84,
  },
  {
    id: 'sambanova',
    label: 'SambaNova (Fast Inference)',
    name: 'sambanova',
    base_url: 'https://api.sambanova.ai/v1',
    api_key_template: '${SAMBANOVA_API_KEY}',
    models: [
      'Meta-Llama-3.3-70B-Instruct',
      'DeepSeek-R1-Distill-Llama-70B',
    ],
    rpm: 30,
    min_interval_ms: 2000,
    max_concurrency: 2,
    smartness_score: 86,
  },
];

export const ProvidersStudio: React.FC<ProvidersStudioProps> = ({ status, onRefresh }) => {
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, TestConnectionResponse & { testing?: boolean }>>({});
  const [searchQuery, setSearchQuery] = useState('');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Modal Form State
  const [selectedPresetId, setSelectedPresetId] = useState<string>('custom');
  const [apiKeyValue, setApiKeyValue] = useState<string>('');
  const [existingEncryptedKeyPreview, setExistingEncryptedKeyPreview] = useState<string | null>(null);

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

  const [modelsInput, setModelsInput] = useState('');

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(id);
    setTimeout(() => setCopiedKey(null), 1500);
  };

  const applyPreset = (presetId: string) => {
    setSelectedPresetId(presetId);
    if (presetId === 'custom') return;

    const p = PRESETS.find((item) => item.id === presetId);
    if (!p) return;

    setFormData((prev) => ({
      ...prev,
      name: p.name,
      base_url: p.base_url,
      api_key_template: p.api_key_template,
      rpm: p.rpm,
      min_interval_ms: p.min_interval_ms,
      max_concurrency: p.max_concurrency,
      smartness_score: p.smartness_score,
    }));
    setApiKeyValue(p.api_key_template);
    setModelsInput(p.models.join('\n'));
  };

  const openAddModal = () => {
    setEditingProvider(null);
    setSelectedPresetId('deepseek');
    const defaultPreset = PRESETS[0]; // DeepSeek
    setFormData({
      name: defaultPreset.name,
      enabled: true,
      base_url: defaultPreset.base_url,
      api_key_template: defaultPreset.api_key_template,
      raw_api_key: '',
      models: defaultPreset.models,
      headers: {},
      rpm: defaultPreset.rpm,
      min_interval_ms: defaultPreset.min_interval_ms,
      max_concurrency: defaultPreset.max_concurrency,
      smartness_score: defaultPreset.smartness_score,
    });
    setApiKeyValue(defaultPreset.api_key_template);
    setModelsInput(defaultPreset.models.join('\n'));
    setExistingEncryptedKeyPreview(null);
    setIsModalOpen(true);
  };

  const openEditModal = (p: any) => {
    setEditingProvider(p.name);
    // Find matching preset if any
    const matchingPreset = PRESETS.find((item) => item.name === p.name);
    setSelectedPresetId(matchingPreset ? matchingPreset.id : 'custom');

    setExistingEncryptedKeyPreview(p.has_encrypted_key ? (p.key_preview || '••••••••') : null);
    // If has encrypted key, leave input empty as placeholder; otherwise pre-fill with api_key_template
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
      // User entered an environment variable reference like $VAR or ${VAR}
      finalApiKeyTemplate = trimmedKey;
      rawApiKey = trimmedKey; // In backend, starting with $ clears encrypted_api_key
    } else if (trimmedKey.length > 0) {
      // User entered a literal secret key
      rawApiKey = trimmedKey;
      if (!finalApiKeyTemplate.startsWith('$')) {
        finalApiKeyTemplate = `\${${formData.name.toUpperCase().replace(/-/g, '_')}_API_KEY}`;
      }
    } else if (existingEncryptedKeyPreview) {
      // User kept input blank when editing an existing encrypted provider -> don't overwrite raw_api_key
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
      {/* Action Bar */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg">
        <div>
          <h2 className="text-lg font-semibold text-white flex items-center gap-2">
            <Server className="w-5 h-5 text-brand-500" />
            Providers Studio
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Configure upstream LLM APIs. Supports direct <code className="text-emerald-400 font-mono">AES-256-GCM</code> salted vault keys or dynamic shell variables (<code className="text-amber-400 font-mono">${'{VAR}'}</code>).
          </p>
        </div>

        <button
          onClick={openAddModal}
          className="px-4 py-2 bg-brand-600 hover:bg-brand-500 text-white rounded-lg text-xs font-semibold flex items-center gap-2 shadow-lg transition-colors self-start md:self-auto"
        >
          <Plus className="w-4 h-4" />
          Add Provider
        </button>
      </div>

      {/* Filter / Search Bar */}
      <div className="flex items-center justify-between gap-4">
        <div className="relative flex-1 max-w-md">
          <Search className="w-4 h-4 text-slate-500 absolute left-3 top-2.5" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Filter by name, URL, model, or ${ENV_VAR}..."
            className="w-full bg-slate-900 border border-slate-800 rounded-lg pl-9 pr-3 py-1.5 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-brand-500"
          />
        </div>
        <div className="text-xs text-slate-400 font-mono">
          {filteredProviders.length} of {status?.providers.length || 0} providers
        </div>
      </div>

      {/* Grouped by Domain List */}
      <div className="space-y-4">
        {groupedProviders.map(([domain, providers]) => (
          <div
            key={domain}
            className="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-xl"
          >
            {/* Domain Group Header */}
            <div className="bg-slate-950/80 px-4 py-2.5 border-b border-slate-800 flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Globe className="w-4 h-4 text-brand-400" />
                <span className="font-mono text-xs font-bold text-slate-200 tracking-wide">
                  {domain}
                </span>
                <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-slate-800 text-slate-400 border border-slate-700">
                  {providers.length} {providers.length === 1 ? 'provider' : 'providers'}
                </span>
              </div>
            </div>

            {/* Providers under this domain */}
            <div className="divide-y divide-slate-800/80">
              {providers.map((p) => {
                const test = testResults[String(p.name)];
                const keyTemplate = p.api_key_template || '${API_KEY}';

                return (
                  <div
                    key={String(p.name)}
                    className="p-4 hover:bg-slate-800/40 transition-colors flex flex-col lg:flex-row lg:items-center justify-between gap-4"
                  >
                    {/* Column 1: Identity & Key */}
                    <div className="min-w-[280px] max-w-sm space-y-1.5">
                      <div className="flex items-center gap-2">
                        <span
                          className={`w-2.5 h-2.5 rounded-full ${
                            p.enabled ? 'bg-emerald-400' : 'bg-slate-600'
                          }`}
                          title={p.enabled ? 'Enabled' : 'Disabled'}
                        />
                        <h3 className="text-sm font-bold text-white font-mono">{p.name}</h3>
                        <span className="text-[10px] text-slate-500 uppercase px-1.5 py-0.2 rounded bg-slate-950 border border-slate-800">
                          {p.circuit}
                        </span>
                      </div>

                      <p className="text-xs text-slate-400 font-mono truncate" title={p.base_url}>
                        {p.base_url}
                      </p>

                      {/* API Key Badge: Vault Secret or Env Var */}
                      <div className="flex items-center gap-1.5 pt-0.5">
                        {p.has_encrypted_key ? (
                          <div
                            className="inline-flex items-center gap-1.5 bg-emerald-950/40 text-emerald-300 border border-emerald-800/50 font-mono text-[11px] px-2 py-0.5 rounded"
                            title="Stored securely in ~/.wirellm/ using AES-256-GCM + Salt"
                          >
                            <Lock className="w-3 h-3 text-emerald-400 flex-shrink-0" />
                            <span>Vault Key: {p.key_preview || '••••••••'}</span>
                          </div>
                        ) : (
                          <div
                            className="inline-flex items-center gap-1.5 bg-amber-950/40 text-amber-300 border border-amber-800/50 font-mono text-[11px] px-2 py-0.5 rounded cursor-pointer hover:border-amber-700 transition-colors"
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
                        <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-slate-800/90 text-slate-300 border border-slate-700 flex items-center gap-1">
                          <Sparkles className="w-3 h-3 text-amber-400" />
                          Score: {p.smartness_score}
                        </span>
                        {p.max_rpm && (
                          <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-slate-800/90 text-slate-300 border border-slate-700 flex items-center gap-1">
                            <Clock className="w-3 h-3 text-sky-400" />
                            {p.max_rpm} RPM
                          </span>
                        )}
                        {p.min_interval_ms && (
                          <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-slate-800/90 text-slate-300 border border-slate-700 flex items-center gap-1">
                            <Sliders className="w-3 h-3 text-emerald-400" />
                            {p.min_interval_ms}ms burst
                          </span>
                        )}
                        <span className="text-[11px] text-slate-400 font-mono">
                          {p.models.length} {p.models.length === 1 ? 'model' : 'models'}
                        </span>
                      </div>

                      {/* Models list */}
                      <div className="flex items-center gap-1.5 flex-wrap max-h-16 overflow-y-auto pr-1">
                        {p.models.map((m) => (
                          <span
                            key={m}
                            className="text-[11px] font-mono bg-slate-950 border border-slate-800/90 px-2 py-0.5 rounded text-slate-300"
                            title={m}
                          >
                            {m}
                          </span>
                        ))}
                      </div>
                    </div>

                    {/* Column 3: Testing & Action Controls */}
                    <div className="flex items-center gap-3 lg:self-center justify-between lg:justify-end border-t lg:border-t-0 pt-3 lg:pt-0 border-slate-800">
                      {/* Test Connection Button */}
                      <div className="flex items-center gap-2">
                        <button
                          onClick={() => handleTest(String(p.name))}
                          disabled={test?.testing}
                          className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-xs font-medium text-slate-200 rounded-lg flex items-center gap-1.5 transition-colors border border-slate-700"
                        >
                          {test?.testing ? (
                            <Activity className="w-3 h-3 animate-spin text-brand-400" />
                          ) : (
                            <Play className="w-3 h-3 text-brand-400" />
                          )}
                          Test
                        </button>

                        {test && !test.testing && (
                          test.success ? (
                            <span className="text-emerald-400 font-mono text-xs font-semibold flex items-center gap-1 bg-emerald-950/40 border border-emerald-800/60 px-2 py-1 rounded">
                              <CheckCircle className="w-3.5 h-3.5" />
                              {test.latency_ms} ms
                            </span>
                          ) : (
                            <span
                              className="text-rose-400 font-mono text-xs font-semibold flex items-center gap-1 bg-rose-950/40 border border-rose-800/60 px-2 py-1 rounded cursor-help"
                              title={test.error || test.message}
                            >
                              <XCircle className="w-3.5 h-3.5" />
                              Failed
                            </span>
                          )
                        )}
                      </div>

                      {/* Edit / Delete Buttons */}
                      <div className="flex items-center gap-1 pl-2 border-l border-slate-800">
                        <button
                          onClick={() => openEditModal(p)}
                          title="Edit Provider"
                          className="p-1.5 text-slate-400 hover:text-white rounded-lg hover:bg-slate-800 transition-colors"
                        >
                          <Edit className="w-4 h-4" />
                        </button>
                        <button
                          onClick={() => handleDelete(String(p.name))}
                          title="Delete Provider"
                          className="p-1.5 text-slate-400 hover:text-rose-400 rounded-lg hover:bg-slate-800 transition-colors"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ))}

        {groupedProviders.length === 0 && (
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-8 text-center text-xs text-slate-500 shadow-xl">
            No providers match "{searchQuery}".
          </div>
        )}
      </div>

      {/* Add / Edit Modal */}
      {isModalOpen && (
        <div className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 w-full max-w-lg shadow-2xl relative max-h-[90vh] overflow-y-auto">
            <h3 className="text-lg font-bold text-white mb-1">
              {editingProvider ? `Edit Provider "${editingProvider}"` : 'Add New LLM Provider'}
            </h3>
            <p className="text-xs text-slate-400 mb-5">
              Saved directly to SQLite in <code className="text-brand-400 font-mono">~/.wirellm/wirellm.db</code>.
            </p>

            <form onSubmit={handleSave} className="space-y-4">
              {/* Preset Selector Dropdown */}
              <div>
                <label className="block text-xs font-semibold text-slate-300 mb-1">
                  Provider Template / Recommended Preset
                </label>
                <select
                  value={selectedPresetId}
                  onChange={(e) => applyPreset(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-brand-300 font-medium focus:outline-none focus:border-brand-500"
                >
                  <option value="custom">-- Custom Provider (Blank) --</option>
                  {PRESETS.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.label}
                    </option>
                  ))}
                </select>
                <span className="text-[11px] text-slate-500 mt-1 block">
                  Selecting a preset pre-fills recommended endpoint URL, default models, RPM, and burst limits.
                </span>
              </div>

              <div>
                <label className="block text-xs font-medium text-slate-300 mb-1">Provider ID / Name</label>
                <input
                  type="text"
                  required
                  disabled={!!editingProvider}
                  value={formData.name}
                  onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                  placeholder="e.g. deepseek, opencode, nvidia"
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-brand-500 disabled:opacity-50"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-slate-300 mb-1">Base URL</label>
                <input
                  type="url"
                  required
                  value={formData.base_url}
                  onChange={(e) => setFormData({ ...formData, base_url: e.target.value })}
                  placeholder="https://api.deepseek.com/v1"
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-brand-500 font-mono"
                />
              </div>

              {/* API Key Input with auto-detection for $VAR vs Encrypted Secret */}
              <div>
                <label className="text-xs font-medium text-slate-300 flex items-center gap-1.5 mb-1">
                  {isDirectSecretKey ? (
                    <Lock className="w-3.5 h-3.5 text-emerald-400" />
                  ) : (
                    <Key className="w-3.5 h-3.5 text-amber-400" />
                  )}
                  API Key or Environment Variable
                </label>

                <div className="relative">
                  <input
                    type={apiKeyValue.trim().startsWith('$') ? 'text' : 'password'}
                    value={apiKeyValue}
                    onChange={(e) => setApiKeyValue(e.target.value)}
                    autoComplete="new-password"
                    spellCheck={false}
                    placeholder={
                      existingEncryptedKeyPreview
                        ? '•••••••••••• (Encrypted in vault — leave blank to keep)'
                        : '${DEEPSEEK_API_KEY} or paste secret key...'
                    }
                    className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-brand-500 font-mono"
                  />
                </div>

                {/* Dynamic Smart Key Mode Info Badge */}
                <div className="mt-1.5">
                  {isEnvVarKey && (
                    <div className="flex items-center gap-1.5 text-[11px] text-amber-300 bg-amber-950/30 border border-amber-900/40 rounded px-2.5 py-1">
                      <Key className="w-3.5 h-3.5 text-amber-400 flex-shrink-0" />
                      <span>
                        <strong>Dynamic Env Variable:</strong> Evaluated at runtime from shell environment or <code className="text-amber-200">~/.wirellm/.env</code>.
                      </span>
                    </div>
                  )}

                  {isDirectSecretKey && (
                    <div className="flex items-center gap-1.5 text-[11px] text-emerald-300 bg-emerald-950/30 border border-emerald-900/40 rounded px-2.5 py-1">
                      <ShieldCheck className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
                      <span>
                        <strong>Direct Secret Key:</strong> Stored securely with <code className="text-emerald-200">AES-256-GCM</code> + local salt (<code className="text-emerald-200">0600</code>) in <code className="text-emerald-200">~/.wirellm/</code>. Never exposed in plaintext.
                      </span>
                    </div>
                  )}

                  {!isEnvVarKey && !isDirectSecretKey && existingEncryptedKeyPreview && (
                    <div className="flex items-center gap-1.5 text-[11px] text-slate-300 bg-slate-800/40 border border-slate-700/50 rounded px-2.5 py-1">
                      <Lock className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
                      <span>
                        Current vault key: <code className="text-emerald-300">•••••••••••• (Encrypted)</code>. Enter a new key or <code className="text-amber-300">$VAR</code> to update.
                      </span>
                    </div>
                  )}

                  {!isEnvVarKey && !isDirectSecretKey && !existingEncryptedKeyPreview && (
                    <span className="text-[11px] text-slate-500 block">
                      Tip: Enter <code className="text-amber-400 font-mono">$VAR</code> to read from environment, or paste your raw secret key to encrypt it in the local vault.
                    </span>
                  )}
                </div>
              </div>

              <div>
                <label className="block text-xs font-medium text-slate-300 mb-1">
                  Models (one per line)
                </label>
                <textarea
                  rows={3}
                  required
                  value={modelsInput}
                  onChange={(e) => setModelsInput(e.target.value)}
                  placeholder="deepseek-chat&#10;deepseek-reasoner"
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-white font-mono focus:outline-none focus:border-brand-500"
                />
              </div>

              <div className="grid grid-cols-3 gap-3">
                <div>
                  <label className="block text-xs font-medium text-slate-300 mb-1">RPM Limit</label>
                  <input
                    type="number"
                    value={formData.rpm || ''}
                    onChange={(e) =>
                      setFormData({
                        ...formData,
                        rpm: e.target.value ? parseInt(e.target.value) : undefined,
                      })
                    }
                    placeholder="30"
                    className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-brand-500"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-slate-300 mb-1">Min Spacing (ms)</label>
                  <input
                    type="number"
                    value={formData.min_interval_ms || ''}
                    onChange={(e) =>
                      setFormData({
                        ...formData,
                        min_interval_ms: e.target.value ? parseInt(e.target.value) : undefined,
                      })
                    }
                    placeholder="2000"
                    className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-brand-500"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-slate-300 mb-1">Smartness (0-100)</label>
                  <input
                    type="number"
                    value={formData.smartness_score || ''}
                    onChange={(e) =>
                      setFormData({
                        ...formData,
                        smartness_score: e.target.value ? parseInt(e.target.value) : undefined,
                      })
                    }
                    placeholder="85"
                    className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-brand-500"
                  />
                </div>
              </div>

              <div className="flex justify-end gap-3 pt-3 border-t border-slate-800">
                <button
                  type="button"
                  onClick={() => setIsModalOpen(false)}
                  className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-xs font-medium text-slate-300 rounded-lg transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-4 py-2 bg-brand-600 hover:bg-brand-500 text-xs font-semibold text-white rounded-lg transition-colors"
                >
                  Save Provider
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
