import React, { useState } from 'react';
import {
  Bot,
  Sparkles,
  Copy,
  Check,
  ShieldAlert,
  Terminal,
  Cpu,
  Zap,
  ChevronDown,
  ChevronUp,
  Workflow,
  Radio,
  ExternalLink,
  BookOpen,
} from 'lucide-react';
import { Card, Badge, Button, cn } from '../ui';

interface AgentSetupHeroProps {
  onNavigateToProviders?: () => void;
  isCloudOrProtected?: boolean;
}

export const AgentSetupHero: React.FC<AgentSetupHeroProps> = ({
  onNavigateToProviders,
  isCloudOrProtected = false,
}) => {
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [showFullDoc, setShowFullDoc] = useState(false);

  const originUrl = window.location.origin;
  const mcpUrl = `${originUrl}/mcp/sse`;

  const handleCopy = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(id);
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const agentPrompt = `Connect to the WireLLM MCP server at ${mcpUrl}
${isCloudOrProtected ? "NOTE: Since WireLLM is running in the cloud with auth enabled, provide the Authorization header: 'Bearer <YOUR_WIRELLM_API_KEY>'" : "NOTE: If running on localhost, authentication is automatically bypassed."}

1. Call 'wirellm_get_setup_recommendations' to inspect recommended always-free and cheap providers (NVIDIA NIM, Groq, OpenRouter, DeepSeek) and virtual model fallback ladders ('coder', 'planner', 'orchestrator').
2. Ask me for API keys for any providers I have available (or inspect my local environment).
3. Use 'wirellm_add_provider' to register available providers with their recommended rate limits and burst intervals.
4. Use 'wirellm_update_virtual_model' to set up the fallback chains for 'coder', 'planner', and 'orchestrator'.
5. Verify health and connectivity with 'wirellm_get_status'.`;

  const mcpConfigSnippet = `{
  "mcpServers": {
    "wirellm": {
      "url": "${mcpUrl}"${
        isCloudOrProtected
          ? `,
      "headers": {
        "Authorization": "Bearer YOUR_WIRELLM_API_KEY"
      }`
          : ''
      }
    }
  }
}`;

  return (
    <Card className="p-6 border-2 border-[var(--primary-border)] bg-gradient-to-b from-[#050b14] to-[#020509] shadow-[0_10px_40px_rgba(0,0,0,0.8),0_0_30px_var(--primary-glow-subtle)] space-y-6">
      {/* 1. Header Banner */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 border-b border-white/[0.08] pb-5">
        <div className="flex items-start gap-3.5">
          <div className="w-12 h-12 rounded-xl bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] flex items-center justify-center text-[var(--primary)] shadow-[0_0_20px_var(--primary-glow)] shrink-0">
            <Bot className="w-6 h-6 animate-pulse" />
          </div>
          <div>
            <div className="flex items-center gap-2.5 flex-wrap">
              <h2 className="text-lg font-black text-white font-mono tracking-tight">
                ZERO PROVIDERS CONFIGURED
              </h2>
              <Badge variant="solid-primary" size="sm">
                <Sparkles className="w-3 h-3 mr-1 inline" />
                AUTONOMOUS AGENT SETUP
              </Badge>
            </div>
            <p className="text-xs text-slate-300 mt-1 max-w-2xl leading-relaxed">
              Why configure model chains and base URLs manually? Connect your AI coding agent (Cursor, OpenCode, Claude Code, Antigravity) to WireLLM's native <strong>Model Context Protocol (MCP)</strong> server, and let your agent configure everything autonomously in seconds.
            </p>
          </div>
        </div>

        {onNavigateToProviders && (
          <Button
            variant="secondary"
            size="sm"
            onClick={onNavigateToProviders}
            className="shrink-0 font-mono text-xs border-white/20 hover:border-[var(--primary-border)] hover:text-[var(--primary)]"
          >
            Or Configure Manually &rarr;
          </Button>
        )}
      </div>

      {/* 2. Step 1: MCP Server Address & Authentication Note */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        {/* Left: Endpoint & Config */}
        <div className="lg:col-span-6 space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono font-bold text-white flex items-center gap-1.5">
              <Radio className="w-3.5 h-3.5 text-[var(--primary)]" />
              1. WIRELLM MCP SERVER ENDPOINT
            </span>
            <span className="text-[10px] font-mono text-slate-400">SSE Transport</span>
          </div>

          <div className="flex items-center gap-2 p-2.5 rounded-xl bg-black/60 border border-white/10 font-mono text-xs text-[var(--primary)]">
            <span className="truncate flex-1 font-semibold">{mcpUrl}</span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => handleCopy(mcpUrl, 'mcp_url')}
              className="h-7 px-2.5 text-xs bg-white/[0.06] hover:bg-white/[0.12] text-slate-200 border-none shrink-0"
            >
              {copiedKey === 'mcp_url' ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
            </Button>
          </div>

          {/* Cloud Auth Alert */}
          <div className="p-3.5 rounded-xl bg-amber-500/10 border border-amber-500/20 text-xs flex items-start gap-2.5 text-amber-200/90 leading-relaxed font-sans">
            <ShieldAlert className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
            <div className="space-y-1">
              <p className="font-semibold text-amber-300 font-mono text-[11px]">
                Authentication Requirement:
              </p>
              <p className="text-[11px] text-slate-300">
                {isCloudOrProtected ? (
                  <>
                    When running in the <strong>cloud</strong> with API token protection enabled, your agent must include an <code className="text-amber-300 font-mono">Authorization: Bearer &lt;TOKEN&gt;</code> header in its MCP config.
                  </>
                ) : (
                  <>
                    When running on <strong>localhost</strong> (or when no API tokens are set in cloud), authentication is <strong>completely bypassed</strong> for zero-friction agent connectivity.
                  </>
                )}
              </p>
            </div>
          </div>
        </div>

        {/* Right: Quick MCP Client JSON Snippet */}
        <div className="lg:col-span-6 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono font-bold text-white flex items-center gap-1.5">
              <Terminal className="w-3.5 h-3.5 text-[var(--secondary)]" />
              MCP CLIENT CONFIG (claude_desktop_config / opencode)
            </span>
            <button
              onClick={() => handleCopy(mcpConfigSnippet, 'mcp_json')}
              className="text-[11px] font-mono text-slate-400 hover:text-[var(--primary)] flex items-center gap-1"
            >
              {copiedKey === 'mcp_json' ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
              <span>{copiedKey === 'mcp_json' ? 'Copied' : 'Copy JSON'}</span>
            </button>
          </div>

          <pre className="p-3 rounded-xl bg-black/80 border border-white/10 font-mono text-[11px] text-slate-300 overflow-x-auto leading-relaxed max-h-[140px]">
            {mcpConfigSnippet}
          </pre>
        </div>
      </div>

      {/* 3. Step 2: Agent Instruction Prompt (1-Click Copy) */}
      <div className="p-4 rounded-xl bg-black/50 border border-white/10 space-y-3">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
          <div>
            <h3 className="text-xs font-mono font-bold text-white flex items-center gap-2">
              <Zap className="w-3.5 h-3.5 text-amber-400" />
              2. COPY THIS PROMPT &amp; PASTE INTO YOUR AI AGENT
            </h3>
            <p className="text-[11px] text-slate-400 mt-0.5">
              Paste this into Cursor, OpenCode, Claude Code, or Antigravity chat. The agent will discover tools and set up your provider mesh.
            </p>
          </div>

          <Button
            size="sm"
            variant="primary"
            onClick={() => handleCopy(agentPrompt, 'agent_prompt')}
            className="shrink-0 font-mono text-xs shadow-[0_0_15px_var(--primary-glow-subtle)]"
          >
            {copiedKey === 'agent_prompt' ? (
              <>
                <Check className="w-3.5 h-3.5 mr-1 text-emerald-300" />
                Copied Prompt!
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5 mr-1" />
                Copy Agent Prompt
              </>
            )}
          </Button>
        </div>

        <pre className="p-3 rounded-lg bg-[#010409] border border-white/[0.08] font-mono text-xs text-slate-300 whitespace-pre-wrap leading-relaxed">
          {agentPrompt}
        </pre>
      </div>

      {/* 4. Recommended Provider Grid (Bundled in Binary) */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Cpu className="w-3.5 h-3.5 text-[var(--primary)]" />
            <span className="text-xs font-mono font-bold text-white">
              RECOMMENDED ALWAYS-FREE &amp; CHEAP PROVIDERS (Bundled Catalog)
            </span>
          </div>
          <button
            onClick={() => setShowFullDoc(!showFullDoc)}
            className="text-[11px] font-mono text-[var(--primary)] hover:underline flex items-center gap-1"
          >
            <BookOpen className="w-3 h-3" />
            <span>{showFullDoc ? 'Hide Catalog Details' : 'View Full Catalog & Ratings'}</span>
            {showFullDoc ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
          </button>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          {/* Card 1: NVIDIA NIM */}
          <div className="p-3.5 rounded-xl bg-black/40 border border-white/10 space-y-2">
            <div className="flex items-center justify-between">
              <span className="font-mono font-bold text-white text-xs">NVIDIA NIM</span>
              <Badge variant="solid-primary" size="xs">Free 1k Credits</Badge>
            </div>
            <p className="text-[11px] text-slate-400 font-mono">
              Nemotron-3-Ultra (550B), Nemotron 3.5 Lightning, Llama-3.3-70B
            </p>
            <div className="flex items-center justify-between text-[10px] font-mono text-slate-500 pt-1 border-t border-white/[0.06]">
              <span>Code: 9.6 &bull; Reason: 9.4</span>
              <span className="text-emerald-400">RPM: 40</span>
            </div>
          </div>

          {/* Card 2: GroqCloud */}
          <div className="p-3.5 rounded-xl bg-black/40 border border-white/10 space-y-2">
            <div className="flex items-center justify-between">
              <span className="font-mono font-bold text-white text-xs">GroqCloud</span>
              <Badge variant="emerald" size="xs">Always Free</Badge>
            </div>
            <p className="text-[11px] text-slate-400 font-mono">
              GPT-OSS-120B, Qwen3.8-27B, Llama-3.3-70B (500+ tok/s)
            </p>
            <div className="flex items-center justify-between text-[10px] font-mono text-slate-500 pt-1 border-t border-white/[0.06]">
              <span>Speed: 10/10 LPU</span>
              <span className="text-emerald-400">RPM: 30</span>
            </div>
          </div>

          {/* Card 3: DeepSeek */}
          <div className="p-3.5 rounded-xl bg-black/40 border border-white/10 space-y-2">
            <div className="flex items-center justify-between">
              <span className="font-mono font-bold text-white text-xs">DeepSeek Official</span>
              <Badge variant="secondary" size="xs">Ultra Cheap</Badge>
            </div>
            <p className="text-[11px] text-slate-400 font-mono">
              DeepSeek-Chat (V3), DeepSeek-Reasoner (R1 CoT)
            </p>
            <div className="flex items-center justify-between text-[10px] font-mono text-slate-500 pt-1 border-t border-white/[0.06]">
              <span>Code: 9.8 &bull; Reason: 9.9</span>
              <span className="text-emerald-400">$0.14 / 1M</span>
            </div>
          </div>

          {/* Card 4: OpenRouter Free */}
          <div className="p-3.5 rounded-xl bg-black/40 border border-white/10 space-y-2">
            <div className="flex items-center justify-between">
              <span className="font-mono font-bold text-white text-xs">OpenRouter</span>
              <Badge variant="neutral" size="xs">Community Free</Badge>
            </div>
            <p className="text-[11px] text-slate-400 font-mono">
              openrouter/free, cohere/north-mini-code:free
            </p>
            <div className="flex items-center justify-between text-[10px] font-mono text-slate-500 pt-1 border-t border-white/[0.06]">
              <span>Zero-Cost Fallback</span>
              <span className="text-emerald-400">RPM: 20</span>
            </div>
          </div>
        </div>

        {/* Collapsible Details */}
        {showFullDoc && (
          <div className="p-4 rounded-xl bg-black/80 border border-white/10 space-y-3 text-xs text-slate-300 font-mono">
            <div className="flex items-center justify-between text-white font-bold border-b border-white/10 pb-2">
              <span>Recommended Virtual Model Routing Ladders</span>
              <Badge variant="primary" size="xs">Auto-Failover</Badge>
            </div>
            <ul className="space-y-2 text-[11px] text-slate-400 list-disc list-inside">
              <li>
                <strong className="text-white">coder</strong>: NVIDIA Nemotron Ultra &rarr; NVIDIA Lightning &rarr; Groq GPT-OSS 120B &rarr; DeepSeek Chat &rarr; OpenRouter Free
              </li>
              <li>
                <strong className="text-white">planner</strong>: DeepSeek Reasoner &rarr; NVIDIA Nemotron Ultra &rarr; Groq Qwen 27B
              </li>
              <li>
                <strong className="text-white">orchestrator</strong>: Groq GPT-OSS 120B &rarr; NVIDIA Lightning &rarr; DeepSeek Chat
              </li>
            </ul>
            <p className="text-[11px] text-slate-500">
              The full guide is bundled directly in the binary and accessible by AI agents calling the <code className="text-[var(--primary)]">wirellm_get_setup_recommendations</code> MCP tool.
            </p>
          </div>
        )}
      </div>
    </Card>
  );
};
