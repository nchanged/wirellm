import React, { useState } from 'react';
import {
  Key,
  Shield,
  ShieldAlert,
  ShieldCheck,
  Copy,
  Check,
  Terminal,
  Code,
  Globe,
  Lock,
  Unlock,
  AlertTriangle,
  Cpu,
  Server,
  ExternalLink,
} from 'lucide-react';
import { Card, MetricCard, Badge, Button, cn } from '../ui';
import { AuthStatusResponse } from '../types';

interface ApiKeysViewProps {
  authStatus: AuthStatusResponse | null;
}

export const ApiKeysView: React.FC<ApiKeysViewProps> = ({ authStatus }) => {
  const [activeSnippetTab, setActiveSnippetTab] = useState<'agents' | 'python' | 'node' | 'curl'>('agents');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const originUrl = window.location.origin;
  const isApiProtected = authStatus?.api_auth_required ?? false;
  const isGoogleProtected = authStatus?.google_auth_enabled ?? false;

  const handleCopy = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(id);
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const agentSnippet = `{
  "models": {
    "coder": {
      "provider": "openai-compatible",
      "baseURL": "${originUrl}/v1",
      "apiKey": "${isApiProtected ? 'YOUR_WIRELLM_API_KEY' : 'unprotected'}"
    },
    "planner": {
      "provider": "openai-compatible",
      "baseURL": "${originUrl}/v1",
      "apiKey": "${isApiProtected ? 'YOUR_WIRELLM_API_KEY' : 'unprotected'}"
    }
  }
}`;

  const pythonSnippet = `from openai import OpenAI

# Connect to WireLLM Gateway
client = OpenAI(
    base_url="${originUrl}/v1",
    api_key="${isApiProtected ? 'YOUR_WIRELLM_API_KEY' : 'unprotected'}"
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
  apiKey: '${isApiProtected ? 'YOUR_WIRELLM_API_KEY' : 'unprotected'}',
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
  ${isApiProtected ? '-H "Authorization: Bearer YOUR_WIRELLM_API_KEY" \\' : '# -H "Authorization: Bearer <key>" (Optional: API is currently open) \\'}
  -d '{
    "model": "coder",
    "messages": [
      {"role": "user", "content": "Hello WireLLM!"}
    ]
  }'`;

  return (
    <div className="space-y-6">
      {/* 1. Header Overview & Security Matrix */}
      <div>
        <div className="flex items-center gap-2.5 mb-1">
          <div className="p-1.5 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)] shadow-[0_0_10px_var(--primary-glow-subtle)]">
            <Key className="w-4 h-4" />
          </div>
          <h1 className="text-xl font-black text-white font-mono tracking-tight">
            API ACCESS &amp; CREDENTIAL KEYS
          </h1>
          <Badge variant={isApiProtected ? 'solid-primary' : 'amber'} size="sm">
            {isApiProtected ? 'TOKEN PROTECTED' : 'PUBLIC PROXY MODE'}
          </Badge>
        </div>
        <p className="text-xs text-slate-400">
          Manage gateway authentication, client Bearer tokens, Google OAuth sessions, and integration snippets for AI coding agents.
        </p>
      </div>

      {/* Security Status Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Card 1: API Token Protection */}
        <Card className="p-4 flex flex-col justify-between">
          <div className="flex items-start justify-between">
            <div>
              <span className="text-[11px] font-mono text-slate-400">API Proxy Access</span>
              <h3 className="text-sm font-bold text-white font-mono mt-1">
                {isApiProtected ? 'Protected' : 'Open / Unprotected'}
              </h3>
            </div>
            <div className={cn('p-2 rounded-lg', isApiProtected ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20' : 'bg-amber-500/10 text-amber-400 border border-amber-500/20')}>
              {isApiProtected ? <Lock className="w-4 h-4" /> : <Unlock className="w-4 h-4" />}
            </div>
          </div>
          <p className="text-[11px] text-slate-400 mt-2">
            {isApiProtected
              ? 'Requests to /v1/* require a valid Bearer token or x-api-key header.'
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
          <div className="mt-2 text-[11px] font-mono text-slate-400">
            {authStatus?.user ? (
              <span className="text-slate-200 truncate block" title={authStatus.user.email}>
                User: {authStatus.user.email}
              </span>
            ) : (
              <span>Loopback / Local Admin</span>
            )}
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
            Requests originating from 127.0.0.1 or ::1 automatically bypass all authentication checks for instant CLI &amp; local agent work.
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

      {/* 2. API Key Configuration & Instructions */}
      <Card className="p-5 space-y-4">
        <div className="flex items-center justify-between border-b border-white/[0.06] pb-3">
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-[var(--primary)]" />
            <h2 className="text-sm font-bold text-white font-mono">
              HOW TO CONFIGURE API TOKENS
            </h2>
          </div>
          <Badge variant="primary">Cluster &amp; Gateway Security</Badge>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
          {/* Method A: Kubernetes / Helm */}
          <div className="p-4 rounded-xl bg-black/40 border border-white/[0.08] space-y-2">
            <div className="flex items-center justify-between text-white font-mono font-bold">
              <span>Option A: Helm Cluster Deployment</span>
              <Badge variant="neutral" size="xs">values.yaml</Badge>
            </div>
            <p className="text-slate-400 text-[11px] leading-relaxed">
              Define the API keys list under <code className="text-[var(--primary)]">auth.apiKeys</code> in your Helm values file:
            </p>
            <pre className="p-2.5 rounded-lg bg-[#030508] border border-white/10 font-mono text-[11px] text-slate-300 overflow-x-auto">
{`auth:
  apiKeys:
    - "sk-wire-my-secret-key-1"
    - "sk-wire-agent-worker-2"`}
            </pre>
          </div>

          {/* Method B: Environment Variable */}
          <div className="p-4 rounded-xl bg-black/40 border border-white/[0.08] space-y-2">
            <div className="flex items-center justify-between text-white font-mono font-bold">
              <span>Option B: Environment Variable</span>
              <Badge variant="neutral" size="xs">Docker / CLI</Badge>
            </div>
            <p className="text-slate-400 text-[11px] leading-relaxed">
              Pass a comma-separated list of accepted Bearer keys via <code className="text-[var(--primary)]">WIRELLM_API_KEYS</code>:
            </p>
            <pre className="p-2.5 rounded-lg bg-[#030508] border border-white/10 font-mono text-[11px] text-slate-300 overflow-x-auto">
{`export WIRELLM_API_KEYS="sk-wire-my-secret-key-1,sk-wire-agent-worker-2"
wirellm start`}
            </pre>
          </div>
        </div>

        <div className="p-3.5 rounded-xl bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] flex items-start gap-3 text-xs">
          <ShieldAlert className="w-4 h-4 text-[var(--primary)] shrink-0 mt-0.5" />
          <div className="space-y-1 text-slate-300">
            <p className="font-semibold text-white">How Clients Authenticate</p>
            <p className="text-[11px] text-slate-400">
              Clients can authenticate using standard HTTP headers:
              <code className="text-[var(--primary)] mx-1">Authorization: Bearer &lt;key&gt;</code> or
              <code className="text-[var(--primary)] mx-1">x-api-key: &lt;key&gt;</code>.
              For WebSocket and SSE endpoints (such as <code className="text-[var(--primary)]">/ws/live</code> or <code className="text-[var(--primary)]">/mcp/sse</code>), clients can also pass <code className="text-[var(--primary)]">?token=&lt;key&gt;</code>.
            </p>
          </div>
        </div>
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
              Drop-in OpenAI SDK &amp; agent configurations to immediately start using WireLLM.
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
    </div>
  );
};
