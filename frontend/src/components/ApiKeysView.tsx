import React, { useEffect, useState } from 'react';
import {
  Key,
  Shield,
  ShieldAlert,
  ShieldCheck,
  Copy,
  Check,
  Plus,
  Trash2,
  Lock,
  Unlock,
  Globe,
  Cpu,
  Code,
  AlertCircle,
  Eye,
  CheckCircle2,
  RefreshCw,
  Sparkles,
} from 'lucide-react';
import { Card, Badge, Button, Modal, Input, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, cn } from '../ui';
import { AuthStatusResponse, ApiKeyRecord } from '../types';
import { fetchApiKeys, createApiKey, deleteApiKey } from '../api';

interface ApiKeysViewProps {
  authStatus: AuthStatusResponse | null;
}

export const ApiKeysView: React.FC<ApiKeysViewProps> = ({ authStatus }) => {
  const [activeSnippetTab, setActiveSnippetTab] = useState<'agents' | 'python' | 'node' | 'curl'>('agents');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Dynamic Keys State
  const [keys, setKeys] = useState<ApiKeyRecord[]>([]);
  const [loadingKeys, setLoadingKeys] = useState<boolean>(true);
  const [showCreateModal, setShowCreateModal] = useState<boolean>(false);
  const [newKeyName, setNewKeyName] = useState<string>('');
  const [newCustomKey, setNewCustomKey] = useState<string>('');
  const [submitting, setSubmitting] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [createdKeyResult, setCreatedKeyResult] = useState<ApiKeyRecord | null>(null);

  const originUrl = window.location.origin;
  const isCloud = authStatus?.is_cloud_env ?? (authStatus?.google_auth_enabled || window.location.hostname !== 'localhost');
  const hasKeys = keys.length > 0;
  const isApiProtected = isCloud || hasKeys || (authStatus?.api_auth_required ?? false);
  const isGoogleProtected = authStatus?.google_auth_enabled ?? false;

  const loadKeys = async () => {
    try {
      setLoadingKeys(true);
      const data = await fetchApiKeys();
      setKeys(data);
    } catch (err: any) {
      console.warn('Failed to load API keys:', err);
    } finally {
      setLoadingKeys(false);
    }
  };

  useEffect(() => {
    loadKeys();
  }, []);

  const handleCopy = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(id);
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const handleCreateKey = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newKeyName.trim()) {
      setErrorMsg('Please enter a key name');
      return;
    }

    try {
      setSubmitting(true);
      setErrorMsg(null);
      const res = await createApiKey(newKeyName.trim(), newCustomKey.trim() || undefined);
      setCreatedKeyResult(res);
      setNewKeyName('');
      setNewCustomKey('');
      setShowCreateModal(false);
      await loadKeys();
    } catch (err: any) {
      setErrorMsg(err.message || 'Failed to create API key');
    } finally {
      setSubmitting(false);
    }
  };

  const handleDeleteKey = async (id: string, name: string) => {
    if (!confirm(`Are you sure you want to revoke API key "${name}"? Any agent using this key will immediately lose access.`)) {
      return;
    }
    try {
      await deleteApiKey(id);
      await loadKeys();
    } catch (err: any) {
      alert(err.message || 'Failed to revoke API key');
    }
  };

  // Generate a sample active key or placeholder
  const activeKeySample = keys.length > 0 ? keys[0].key_preview : 'sk-wire-prod-12345678';

  const agentSnippet = `{
  "models": {
    "coder": {
      "provider": "openai-compatible",
      "baseURL": "${originUrl}/v1",
      "apiKey": "${keys.length > 0 ? 'YOUR_GENERATED_API_KEY' : 'sk-wire-...'}"
    },
    "planner": {
      "provider": "openai-compatible",
      "baseURL": "${originUrl}/v1",
      "apiKey": "${keys.length > 0 ? 'YOUR_GENERATED_API_KEY' : 'sk-wire-...'}"
    }
  }
}`;

  const pythonSnippet = `from openai import OpenAI

# Connect to WireLLM Gateway
client = OpenAI(
    base_url="${originUrl}/v1",
    api_key="${keys.length > 0 ? 'YOUR_GENERATED_API_KEY' : 'sk-wire-...'}"
)

# Route to virtual model (auto-cascades through healthy providers)
response = client.chat.completions.create(
    model="coder",  # or "planner", "orchestrator", "deepseek-chat"
    messages=[
        {"role": "system", "content": "You are an expert coding assistant."},
        {"role": "user", "content": "Implement an LRU Cache in Rust with unit tests."}
    ],
    temperature=0.2
)

print(response.choices[0].message.content)`;

  const nodeSnippet = `import OpenAI from 'openai';

// Connect to WireLLM Gateway
const client = new OpenAI({
  baseURL: '${originUrl}/v1',
  apiKey: '${keys.length > 0 ? 'YOUR_GENERATED_API_KEY' : 'sk-wire-...'}',
});

async function main() {
  const completion = await client.chat.completions.create({
    model: 'coder', // or "planner", "orchestrator"
    messages: [{ role: 'user', content: 'Explain Rust Send and Sync traits.' }],
  });

  console.log(completion.choices[0].message.content);
}

main().catch(console.error);`;

  const curlSnippet = `curl -X POST "${originUrl}/v1/chat/completions" \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer ${keys.length > 0 ? 'YOUR_GENERATED_API_KEY' : 'sk-wire-...'}" \\
  -d '{
    "model": "coder",
    "messages": [
      {"role": "user", "content": "Hello WireLLM!"}
    ]
  }'`;

  return (
    <div className="space-y-6">
      {/* 1. Header Overview & Primary Call-to-Action */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2.5 mb-1 flex-wrap">
            <div className="p-1.5 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)] shadow-[0_0_10px_var(--primary-glow-subtle)]">
              <Key className="w-4 h-4" />
            </div>
            <h1 className="text-xl font-black text-white font-mono tracking-tight">
              API ACCESS &amp; CREDENTIAL KEYS
            </h1>
            <Badge variant={isApiProtected ? 'solid-primary' : 'amber'} size="sm">
              {isApiProtected ? 'PROTECTED (TOKEN REQUIRED)' : 'PUBLIC PROXY MODE'}
            </Badge>
          </div>
          <p className="text-xs text-slate-400">
            Generate and revoke client Bearer tokens for AI coding agents (OpenCode, Cursor, Claude Code, Antigravity).
          </p>
        </div>

        {/* Primary CREATE API KEY Button */}
        <Button
          variant="primary"
          onClick={() => {
            setErrorMsg(null);
            setShowCreateModal(true);
          }}
          className="shrink-0 shadow-[0_0_20px_var(--primary-glow)] font-mono text-xs font-bold"
        >
          <Plus className="w-4 h-4 mr-1.5" />
          Create API Key
        </Button>
      </div>

      {/* Security Status Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Card 1: API Token Protection */}
        <Card className="p-4 flex flex-col justify-between">
          <div className="flex items-start justify-between">
            <div>
              <span className="text-[11px] font-mono text-slate-400">Cloud Proxy Access</span>
              <h3 className="text-sm font-bold text-white font-mono mt-1">
                {isApiProtected ? 'Restricted / Protected' : 'Open / Unprotected'}
              </h3>
            </div>
            <div className={cn('p-2 rounded-lg', isApiProtected ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20' : 'bg-amber-500/10 text-amber-400 border border-amber-500/20')}>
              {isApiProtected ? <Lock className="w-4 h-4" /> : <Unlock className="w-4 h-4" />}
            </div>
          </div>
          <p className="text-[11px] text-slate-400 mt-2">
            {isApiProtected
              ? 'Requests to /v1/* in cloud require a valid Bearer key or x-api-key.'
              : 'Requests to /v1/* can be executed without an API token.'}
          </p>
        </Card>

        {/* Card 2: Web UI Authentication */}
        <Card className="p-4 flex flex-col justify-between">
          <div className="flex items-start justify-between">
            <div>
              <span className="text-[11px] font-mono text-slate-400">Web UI Access</span>
              <h3 className="text-sm font-bold text-white font-mono mt-1">
                {isGoogleProtected ? 'Google OAuth 2.0' : 'Open Local UI'}
              </h3>
            </div>
            <div className="p-2 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)]">
              <ShieldCheck className="w-4 h-4" />
            </div>
          </div>
          <div className="mt-2 text-[11px] font-mono text-slate-400 truncate" title={authStatus?.user?.email || 'Localhost'}>
            {authStatus?.user ? `User: ${authStatus.user.email}` : 'Localhost Administrator'}
          </div>
        </Card>

        {/* Card 3: Localhost Loopback */}
        <Card className="p-4 flex flex-col justify-between">
          <div className="flex items-start justify-between">
            <div>
              <span className="text-[11px] font-mono text-slate-400">Localhost Bypass</span>
              <h3 className="text-sm font-bold text-emerald-400 font-mono mt-1">
                Active (Zero-Friction)
              </h3>
            </div>
            <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400">
              <Cpu className="w-4 h-4" />
            </div>
          </div>
          <p className="text-[11px] text-slate-400 mt-2">
            Requests from 127.0.0.1 or ::1 automatically bypass all authentication for instant local dev &amp; CLI.
          </p>
        </Card>

        {/* Card 4: Gateway Origin */}
        <Card className="p-4 flex flex-col justify-between">
          <div className="flex items-start justify-between">
            <div>
              <span className="text-[11px] font-mono text-slate-400">Gateway Origin</span>
              <h3 className="text-xs font-bold text-slate-200 font-mono mt-1 truncate max-w-[170px]" title={originUrl}>
                {originUrl}
              </h3>
            </div>
            <div className="p-2 rounded-lg bg-white/[0.04] border border-white/10 text-slate-400">
              <Globe className="w-4 h-4" />
            </div>
          </div>
          <div className="mt-2 flex items-center justify-between text-[11px] font-mono text-[var(--primary)]">
            <span>Base: {originUrl}/v1</span>
            <button
              onClick={() => handleCopy(`${originUrl}/v1`, 'origin')}
              className="text-slate-400 hover:text-white"
              title="Copy Base URL"
            >
              {copiedKey === 'origin' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
            </button>
          </div>
        </Card>
      </div>

      {/* 2. ACTIVE API KEYS TABLE */}
      <Card className="p-5 space-y-4">
        <div className="flex items-center justify-between border-b border-white/[0.06] pb-3">
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-[var(--primary)]" />
            <h2 className="text-sm font-bold text-white font-mono">
              ACTIVE API KEYS ({keys.length})
            </h2>
          </div>
          <Button
            size="sm"
            variant="secondary"
            onClick={loadKeys}
            title="Refresh keys list"
            className="h-7 px-2 text-xs"
          >
            <RefreshCw className={cn('w-3 h-3', loadingKeys && 'animate-spin text-[var(--primary)]')} />
          </Button>
        </div>

        {keys.length === 0 ? (
          <div className="py-8 text-center space-y-3 bg-black/30 rounded-xl border border-dashed border-white/10 p-6">
            <div className="w-10 h-10 rounded-full bg-amber-500/10 border border-amber-500/20 text-amber-400 flex items-center justify-center mx-auto">
              <Key className="w-5 h-5" />
            </div>
            <div>
              <p className="text-sm font-bold text-white font-mono">No API Keys Generated Yet</p>
              <p className="text-xs text-slate-400 max-w-md mx-auto mt-1 leading-relaxed">
                In cloud environments, remote requests to <code className="text-slate-200">/v1/*</code> are restricted until you generate an API key. Click below to create your first key.
              </p>
            </div>
            <Button
              variant="primary"
              size="sm"
              onClick={() => setShowCreateModal(true)}
              className="font-mono text-xs"
            >
              <Plus className="w-3.5 h-3.5 mr-1" />
              Generate First API Key
            </Button>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Key Name / Label</TableHead>
                  <TableHead>Key Identifier</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {keys.map((k) => (
                  <TableRow key={k.id}>
                    <TableCell className="font-semibold text-white font-mono">
                      {k.name}
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2 font-mono text-xs text-[var(--primary)]">
                        <span>{k.key_preview}</span>
                        <button
                          onClick={() => handleCopy(k.key_preview, k.id)}
                          title="Copy Key Identifier"
                          className="text-slate-500 hover:text-white"
                        >
                          {copiedKey === k.id ? (
                            <Check className="w-3 h-3 text-emerald-400" />
                          ) : (
                            <Copy className="w-3 h-3" />
                          )}
                        </button>
                      </div>
                    </TableCell>
                    <TableCell className="text-xs text-slate-400 font-mono">
                      {k.created_at}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => handleDeleteKey(k.id, k.name)}
                        className="h-7 px-2.5 text-xs text-rose-400 hover:text-rose-300 hover:bg-rose-500/10 border-rose-500/20"
                        title="Revoke this API Key"
                      >
                        <Trash2 className="w-3 h-3 mr-1" />
                        Revoke
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </Card>

      {/* 3. Integration Code Snippets */}
      <Card className="p-5 space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-white/[0.06] pb-3">
          <div>
            <h2 className="text-sm font-bold text-white font-mono flex items-center gap-2">
              <Code className="w-4 h-4 text-[var(--secondary)]" />
              CLIENT INTEGRATION SNIPPETS
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              Copy-pasteable configs for AI agents and OpenAI SDKs.
            </p>
          </div>

          {/* Snippet Tabs */}
          <div className="flex items-center gap-1 bg-[#010307] p-1 rounded-lg border border-white/10">
            {(
              [
                { id: 'agents', label: 'AI Agents (OpenCode / Cursor)' },
                { id: 'python', label: 'Python (OpenAI)' },
                { id: 'node', label: 'Node.js' },
                { id: 'curl', label: 'cURL' },
              ] as const
            ).map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveSnippetTab(tab.id)}
                className={cn(
                  'px-2.5 py-1 rounded text-xs font-mono transition-all',
                  activeSnippetTab === tab.id
                    ? 'bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)] font-bold'
                    : 'text-slate-400 hover:text-white'
                )}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </div>

        {/* Snippet Code Container */}
        <div className="relative">
          <pre className="p-4 rounded-xl bg-[#010409] border border-white/10 font-mono text-xs text-slate-300 overflow-x-auto leading-relaxed max-h-[380px]">
            {activeSnippetTab === 'agents' && agentSnippet}
            {activeSnippetTab === 'python' && pythonSnippet}
            {activeSnippetTab === 'node' && nodeSnippet}
            {activeSnippetTab === 'curl' && curlSnippet}
          </pre>

          <Button
            size="sm"
            variant="secondary"
            onClick={() => {
              const text =
                activeSnippetTab === 'agents'
                  ? agentSnippet
                  : activeSnippetTab === 'python'
                  ? pythonSnippet
                  : activeSnippetTab === 'node'
                  ? nodeSnippet
                  : curlSnippet;
              handleCopy(text, 'snippet');
            }}
            className="absolute top-3 right-3 bg-black/60 backdrop-blur border-white/20 hover:border-[var(--primary-border)] hover:text-[var(--primary)]"
          >
            {copiedKey === 'snippet' ? (
              <>
                <Check className="w-3.5 h-3.5 text-emerald-400" />
                <span>Copied!</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>Copy Code</span>
              </>
            )}
          </Button>
        </div>
      </Card>

      {/* CREATE API KEY MODAL */}
      <Modal
        isOpen={showCreateModal}
        onClose={() => setShowCreateModal(false)}
        title="Create New API Key"
        description="Generate an access key for coding agents (OpenCode, Cursor, Claude Code) or scripts."
      >
        <form onSubmit={handleCreateKey} className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-xs font-mono text-slate-300">Key Name / Identifier *</label>
            <Input
              placeholder="e.g. claude-code-dev, cursor-ide, agent-worker"
              value={newKeyName}
              onChange={(e) => setNewKeyName(e.target.value)}
              required
              autoFocus
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-mono text-slate-300">Custom Secret Key (Optional)</label>
            <Input
              placeholder="Leave blank to auto-generate a secure random sk-wire-... key"
              value={newCustomKey}
              onChange={(e) => setNewCustomKey(e.target.value)}
              mono
            />
          </div>

          {errorMsg && (
            <div className="p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-300 text-xs flex items-center gap-2">
              <AlertCircle className="w-4 h-4 shrink-0 text-rose-400" />
              <span>{errorMsg}</span>
            </div>
          )}

          <div className="flex justify-end gap-2 pt-2 border-t border-white/10">
            <Button
              type="button"
              variant="secondary"
              onClick={() => setShowCreateModal(false)}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              variant="primary"
              disabled={submitting}
              className="font-mono text-xs"
            >
              {submitting ? 'Generating...' : 'Create Key'}
            </Button>
          </div>
        </form>
      </Modal>

      {/* NEW KEY SUCCESS MODAL */}
      <Modal
        isOpen={!!createdKeyResult}
        onClose={() => setCreatedKeyResult(null)}
        title="API Key Created Successfully"
        description="Save your secret key now. For security purposes, you will not be able to view the full key again."
      >
        {createdKeyResult && (
          <div className="space-y-4">
            <div className="p-3 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-300 text-xs flex items-center gap-2">
              <CheckCircle2 className="w-4 h-4 shrink-0 text-emerald-400" />
              <span>Key <strong>{createdKeyResult.name}</strong> is now active and ready to use!</span>
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-mono text-slate-400">Your Secret Key:</label>
              <div className="p-3 rounded-xl bg-black border border-[var(--primary-border)] font-mono text-xs text-[var(--primary)] flex items-center justify-between gap-2 shadow-[0_0_15px_var(--primary-glow-subtle)]">
                <span className="truncate select-all font-bold">
                  {createdKeyResult.full_key || createdKeyResult.key_preview}
                </span>
                <Button
                  size="sm"
                  variant="primary"
                  onClick={() => handleCopy(createdKeyResult.full_key || createdKeyResult.key_preview, 'new_secret')}
                  className="h-7 px-3 text-xs shrink-0"
                >
                  {copiedKey === 'new_secret' ? (
                    <>
                      <Check className="w-3 h-3 mr-1 text-emerald-300" />
                      Copied!
                    </>
                  ) : (
                    <>
                      <Copy className="w-3 h-3 mr-1" />
                      Copy Key
                    </>
                  )}
                </Button>
              </div>
            </div>

            <div className="p-3 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-200 text-[11px] leading-relaxed">
              Pass this key in your client headers: <code className="text-amber-300 font-mono">Authorization: Bearer {createdKeyResult.key_preview}</code>
            </div>

            <div className="flex justify-end pt-2">
              <Button
                variant="secondary"
                onClick={() => setCreatedKeyResult(null)}
                className="font-mono text-xs"
              >
                I Have Saved My Key
              </Button>
            </div>
          </div>
        )}
      </Modal>
    </div>
  );
};
