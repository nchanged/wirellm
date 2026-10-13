import React, { useState } from 'react';
import { ApiStatusResponse, VirtualModelDetail, VirtualModelTarget } from '../types';
import {
  Plus,
  Trash2,
  Edit3,
  Sliders,
  Sparkles,
  Zap,
  ShieldCheck,
  ArrowRight,
  Boxes,
  PlusCircle,
} from 'lucide-react';
import { upsertVirtualModel, deleteVirtualModel } from '../api';
import { Button, Card, Badge, Input, Modal } from '../ui';

interface VirtualModelsStudioProps {
  status: ApiStatusResponse | null;
  onRefresh: () => void;
}

export const VirtualModelsStudio: React.FC<VirtualModelsStudioProps> = ({ status, onRefresh }) => {
  const [editingModel, setEditingModel] = useState<string | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);

  const [modelName, setModelName] = useState('');
  const [description, setDescription] = useState('');
  const [weightSmartness, setWeightSmartness] = useState(0.5);
  const [weightSpeed, setWeightSpeed] = useState(0.3);
  const [weightReliability, setWeightReliability] = useState(0.2);
  const [targets, setTargets] = useState<VirtualModelTarget[]>([]);

  const openAddModal = () => {
    setEditingModel(null);
    setModelName('');
    setDescription('Ultra-low latency reasoning ladder with automatic fallback');
    setWeightSmartness(0.5);
    setWeightSpeed(0.3);
    setWeightReliability(0.2);
    setTargets([
      { provider: 'nvidia', model: 'deepseek-ai/deepseek-v4.1-flash' },
      { provider: 'groq', model: 'llama-3.3-70b-versatile' },
    ]);
    setIsModalOpen(true);
  };

  const openEditModal = (vm: VirtualModelDetail) => {
    setEditingModel(vm.name);
    setModelName(vm.name);
    setDescription(vm.description);
    setWeightSmartness(vm.weight_smartness);
    setWeightSpeed(vm.weight_speed);
    setWeightReliability(vm.weight_reliability);
    setTargets([...vm.targets]);
    setIsModalOpen(true);
  };

  const handleAddTarget = () => {
    setTargets([...targets, { provider: '', model: '' }]);
  };

  const handleRemoveTarget = (index: number) => {
    setTargets(targets.filter((_, i) => i !== index));
  };

  const handleTargetChange = (index: number, field: 'provider' | 'model', value: string) => {
    const updated = [...targets];
    updated[index][field] = value;
    setTargets(updated);
  };

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!modelName) return;

    try {
      await upsertVirtualModel(modelName, {
        description,
        weight_smartness: weightSmartness,
        weight_speed: weightSpeed,
        weight_reliability: weightReliability,
        targets: targets.filter((t) => t.provider && t.model),
      });
      setIsModalOpen(false);
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to save virtual model');
    }
  };

  const handleDelete = async (name: string) => {
    if (!confirm(`Are you sure you want to delete virtual model "${name}"?`)) return;
    try {
      await deleteVirtualModel(name);
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to delete virtual model');
    }
  };

  return (
    <div className="space-y-6">
      {/* Header Bar */}
      <Card className="p-5 flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-start gap-3.5">
          <div className="p-2.5 rounded-lg bg-cyan-950/40 border border-cyan-800/40 text-cyan-400 shrink-0">
            <Boxes className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-base font-bold text-white tracking-wide flex items-center gap-2">
              Virtual Models &amp; Dynamic Ladders
              <Badge variant="cyan">L2 MATRIX</Badge>
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              Aggregate upstream LLMs into unified virtual endpoints like <code className="text-cyan-400 font-mono font-semibold">smart</code> or <code className="text-cyan-400 font-mono font-semibold">fast</code>. Automated failover triggers within 2ms if upstream throws 429/5xx.
            </p>
          </div>
        </div>

        <Button variant="cyan" size="md" onClick={openAddModal}>
          <Plus className="w-4 h-4 stroke-[2.5]" />
          Create Virtual Alias
        </Button>
      </Card>

      {/* Grid of Virtual Models */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
        {status?.virtual_models.map((vm) => (
          <Card key={vm.name} hoverEffect className="p-5 space-y-4 flex flex-col justify-between">
            <div>
              {/* Card Header */}
              <div className="flex items-start justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-bold text-white font-mono tracking-tight group-hover:text-cyan-400 transition-colors">
                      {vm.name}
                    </span>
                    <Badge variant="neutral">{vm.targets.length} targets</Badge>
                  </div>
                  <p className="text-xs text-slate-400 mt-1 line-clamp-2">
                    {vm.description || 'No description configured'}
                  </p>
                </div>
                <div className="flex items-center gap-1">
                  <Button variant="ghost" size="icon" onClick={() => openEditModal(vm)} title="Configure Virtual Model">
                    <Edit3 className="w-3.5 h-3.5" />
                  </Button>
                  <Button variant="ghost" size="icon" onClick={() => handleDelete(vm.name)} title="Delete Virtual Model">
                    <Trash2 className="w-3.5 h-3.5 hover:text-rose-400" />
                  </Button>
                </div>
              </div>

              {/* Weight Distribution Metrics */}
              <div className="mt-4 grid grid-cols-3 gap-2 bg-[#030508] p-2.5 rounded-lg border border-white/[0.05] text-center">
                <div className="space-y-0.5">
                  <div className="text-[10px] text-slate-500 uppercase tracking-wider flex items-center justify-center gap-1">
                    <Sparkles className="w-2.5 h-2.5 text-amber-400/80" />
                    Smartness
                  </div>
                  <div className="text-xs font-mono font-bold text-amber-400">
                    {(vm.weight_smartness * 100).toFixed(0)}%
                  </div>
                </div>
                <div className="space-y-0.5 border-x border-white/[0.04]">
                  <div className="text-[10px] text-slate-500 uppercase tracking-wider flex items-center justify-center gap-1">
                    <Zap className="w-2.5 h-2.5 text-cyan-400/80" />
                    Speed
                  </div>
                  <div className="text-xs font-mono font-bold text-cyan-400">
                    {(vm.weight_speed * 100).toFixed(0)}%
                  </div>
                </div>
                <div className="space-y-0.5">
                  <div className="text-[10px] text-slate-500 uppercase tracking-wider flex items-center justify-center gap-1">
                    <ShieldCheck className="w-2.5 h-2.5 text-emerald-400/80" />
                    Reliability
                  </div>
                  <div className="text-xs font-mono font-bold text-emerald-400">
                    {(vm.weight_reliability * 100).toFixed(0)}%
                  </div>
                </div>
              </div>

              {/* Failover Target Ladder */}
              <div className="mt-4 space-y-2">
                <div className="text-[11px] font-mono text-slate-500 uppercase tracking-wider flex items-center justify-between">
                  <span>Failover Sequence</span>
                  <span className="text-[10px] text-slate-600">Priority order</span>
                </div>
                <div className="space-y-1.5">
                  {vm.targets.map((tgt, i) => (
                    <div
                      key={`${tgt.provider}-${tgt.model}-${i}`}
                      className="flex items-center justify-between text-xs bg-[#030508] px-3 py-2 rounded-lg border border-white/[0.05] hover:border-white/[0.1] transition-colors"
                    >
                      <div className="flex items-center gap-2.5 min-w-0">
                        <span className="w-4 h-4 rounded bg-white/[0.05] border border-white/[0.08] text-slate-400 text-[10px] flex items-center justify-center font-mono font-bold shrink-0">
                          {i + 1}
                        </span>
                        <span className="font-semibold text-white capitalize text-xs tracking-tight">
                          {tgt.provider}
                        </span>
                        <ArrowRight className="w-3 h-3 text-slate-600 shrink-0" />
                        <span className="text-slate-400 font-mono text-[11px] truncate">
                          {tgt.model}
                        </span>
                      </div>
                      {i === 0 ? (
                        <Badge variant="emerald" size="xs">PRIMARY</Badge>
                      ) : (
                        <Badge variant="neutral" size="xs">BACKUP</Badge>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            </div>

            {/* Quick Curl Endpoint Helper */}
            <div className="pt-3 border-t border-white/[0.05] flex items-center justify-between text-[11px] text-slate-500 font-mono">
              <span>Endpoint:</span>
              <code className="text-cyan-400/90 text-[11px] bg-black/40 px-2 py-0.5 rounded border border-white/[0.04]">
                "model": "{vm.name}"
              </code>
            </div>
          </Card>
        ))}

        {(!status?.virtual_models || status.virtual_models.length === 0) && (
          <Card className="col-span-full p-12 text-center">
            <Boxes className="w-10 h-10 text-slate-600 mx-auto mb-3" />
            <h3 className="text-sm font-bold text-white">No Virtual Models Defined</h3>
            <p className="text-xs text-slate-400 mt-1 max-w-md mx-auto">
              Create a unified model alias to give your agents ultra-reliable fallback chains across Groq, Nvidia NIM, Anthropic, and OpenAI.
            </p>
            <Button variant="cyan" size="md" onClick={openAddModal} className="mt-4">
              <Plus className="w-3.5 h-3.5 stroke-[2.5]" />
              Create First Virtual Model
            </Button>
          </Card>
        )}
      </div>

      {/* Edit/Add Modal */}
      <Modal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        title={editingModel ? `Edit Virtual Model "${editingModel}"` : 'Create Virtual Model Alias'}
        description="Compiled directly to SQLite with atomic runtime failovers."
        icon={<Sliders className="w-4 h-4" />}
        footer={
          <>
            <Button variant="secondary" onClick={() => setIsModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="cyan" onClick={handleSave}>
              Save Virtual Model
            </Button>
          </>
        }
      >
        <form onSubmit={handleSave} className="space-y-4">
          <div>
            <label className="block text-xs font-mono text-slate-300 mb-1">
              Virtual Model Name / Target Alias
            </label>
            <Input
              type="text"
              required
              disabled={!!editingModel}
              value={modelName}
              onChange={(e) => setModelName(e.target.value)}
              placeholder="e.g. smart, fast, code, creative"
              mono
            />
          </div>

          <div>
            <label className="block text-xs font-mono text-slate-300 mb-1">Description</label>
            <Input
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Primary high-capability reasoner with instant failover"
            />
          </div>

          {/* Weight Tuning Sliders */}
          <div className="bg-[#030508] border border-white/[0.06] rounded-xl p-3.5 space-y-3">
            <div className="text-[11px] font-mono text-slate-400 uppercase tracking-wider flex items-center justify-between">
              <span>Routing Optimization Weights</span>
              <span className="text-[10px] text-slate-500">Auto-balanced scoring</span>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between text-xs">
                <span className="text-amber-400 flex items-center gap-1 font-mono">
                  <Sparkles className="w-3 h-3" /> Smartness
                </span>
                <span className="font-mono text-white text-xs">{(weightSmartness * 100).toFixed(0)}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={weightSmartness}
                onChange={(e) => setWeightSmartness(parseFloat(e.target.value))}
                className="w-full accent-amber-400 h-1 bg-white/[0.1] rounded cursor-pointer"
              />
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between text-xs">
                <span className="text-cyan-400 flex items-center gap-1 font-mono">
                  <Zap className="w-3 h-3" /> Speed
                </span>
                <span className="font-mono text-white text-xs">{(weightSpeed * 100).toFixed(0)}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={weightSpeed}
                onChange={(e) => setWeightSpeed(parseFloat(e.target.value))}
                className="w-full accent-cyan-400 h-1 bg-white/[0.1] rounded cursor-pointer"
              />
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between text-xs">
                <span className="text-emerald-400 flex items-center gap-1 font-mono">
                  <ShieldCheck className="w-3 h-3" /> Reliability
                </span>
                <span className="font-mono text-white text-xs">{(weightReliability * 100).toFixed(0)}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={weightReliability}
                onChange={(e) => setWeightReliability(parseFloat(e.target.value))}
                className="w-full accent-emerald-400 h-1 bg-white/[0.1] rounded cursor-pointer"
              />
            </div>
          </div>

          {/* Target Ladder Rows */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <label className="text-xs font-mono text-slate-300">
                Prioritized Target Ladder
              </label>
              <button
                type="button"
                onClick={handleAddTarget}
                className="text-xs text-cyan-400 hover:text-cyan-300 font-mono font-medium flex items-center gap-1 transition-colors"
              >
                <PlusCircle className="w-3.5 h-3.5" />
                Add Candidate
              </button>
            </div>

            <div className="space-y-2 max-h-48 overflow-y-auto pr-1">
              {targets.map((tgt, i) => (
                <div key={i} className="flex items-center gap-2">
                  <span className="text-xs font-mono text-slate-500 w-4 text-center">{i + 1}</span>
                  <div className="w-1/3">
                    <Input
                      type="text"
                      required
                      placeholder="Provider (e.g. nvidia)"
                      value={tgt.provider}
                      onChange={(e) => handleTargetChange(i, 'provider', e.target.value)}
                      mono
                    />
                  </div>
                  <div className="flex-1">
                    <Input
                      type="text"
                      required
                      placeholder="Model ID (e.g. deepseek-ai/deepseek-v4.1-flash)"
                      value={tgt.model}
                      onChange={(e) => handleTargetChange(i, 'model', e.target.value)}
                      mono
                    />
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={() => handleRemoveTarget(i)}
                  >
                    <Trash2 className="w-3.5 h-3.5 text-slate-500 hover:text-rose-400" />
                  </Button>
                </div>
              ))}
            </div>
          </div>
        </form>
      </Modal>
    </div>
  );
};
