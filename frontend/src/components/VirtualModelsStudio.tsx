import React, { useState } from 'react';
import { ApiStatusResponse, VirtualModelDetail, VirtualModelTarget } from '../types';
import { Layers, Plus, Trash2, Edit, Save, ArrowDown } from 'lucide-react';
import { upsertVirtualModel, deleteVirtualModel } from '../api';

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
    setDescription('Balanced fallback chain');
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
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg">
        <div>
          <h2 className="text-lg font-semibold text-white flex items-center gap-2">
            <Layers className="w-5 h-5 text-brand-500" />
            Virtual Models Studio
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Create unified aliases like <code className="text-brand-400 font-mono">smart</code> or <code className="text-brand-400 font-mono">fast</code> that dynamically fail over across providers.
          </p>
        </div>

        <button
          onClick={openAddModal}
          className="px-3.5 py-2 bg-brand-600 hover:bg-brand-500 text-white rounded-lg text-xs font-semibold flex items-center gap-1.5 shadow transition-colors"
        >
          <Plus className="w-4 h-4" />
          New Virtual Model
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {status?.virtual_models.map((vm) => (
          <div
            key={vm.name}
            className="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg space-y-4 flex flex-col justify-between"
          >
            <div>
              <div className="flex items-start justify-between">
                <div>
                  <h3 className="text-base font-bold text-brand-400 font-mono">{vm.name}</h3>
                  <p className="text-xs text-slate-400 mt-0.5">{vm.description || 'No description provided'}</p>
                </div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => openEditModal(vm)}
                    className="p-1.5 text-slate-400 hover:text-white rounded hover:bg-slate-800 transition-colors"
                    title="Edit Virtual Model"
                  >
                    <Edit className="w-4 h-4" />
                  </button>
                  <button
                    onClick={() => handleDelete(vm.name)}
                    className="p-1.5 text-slate-400 hover:text-rose-400 rounded hover:bg-slate-800 transition-colors"
                    title="Delete Virtual Model"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>

              {/* Ranking Weights */}
              <div className="mt-3 grid grid-cols-3 gap-2 bg-slate-950/60 p-2.5 rounded-lg border border-slate-800/80 text-center">
                <div>
                  <div className="text-[10px] text-slate-500 uppercase tracking-wide">Smartness</div>
                  <div className="text-xs font-bold text-amber-400 font-mono">
                    {(vm.weight_smartness * 100).toFixed(0)}%
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-slate-500 uppercase tracking-wide">Speed</div>
                  <div className="text-xs font-bold text-sky-400 font-mono">
                    {(vm.weight_speed * 100).toFixed(0)}%
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-slate-500 uppercase tracking-wide">Reliability</div>
                  <div className="text-xs font-bold text-emerald-400 font-mono">
                    {(vm.weight_reliability * 100).toFixed(0)}%
                  </div>
                </div>
              </div>

              {/* Ladder Targets */}
              <div className="mt-4 space-y-2">
                <div className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
                  Failover Targets ({vm.targets.length})
                </div>
                {vm.targets.map((tgt, i) => (
                  <div
                    key={`${tgt.provider}-${tgt.model}-${i}`}
                    className="flex items-center justify-between text-xs bg-slate-950 px-3 py-2 rounded-lg border border-slate-800/80"
                  >
                    <div className="flex items-center gap-2">
                      <span className="w-4 h-4 rounded-full bg-slate-800 text-slate-400 text-[10px] flex items-center justify-center font-bold">
                        {i + 1}
                      </span>
                      <span className="font-semibold text-slate-200 capitalize">{tgt.provider}</span>
                    </div>
                    <span className="text-slate-400 font-mono text-[11px] truncate max-w-[200px]">
                      {tgt.model}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Edit/Add Modal */}
      {isModalOpen && (
        <div className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 w-full max-w-lg shadow-2xl relative">
            <h3 className="text-lg font-bold text-white mb-1">
              {editingModel ? `Edit Virtual Model "${editingModel}"` : 'New Virtual Model'}
            </h3>
            <p className="text-xs text-slate-400 mb-4">
              Stored in SQLite with priority order and automated health checks.
            </p>

            <form onSubmit={handleSave} className="space-y-4">
              <div>
                <label className="block text-xs font-medium text-slate-300 mb-1">Model Name / Alias</label>
                <input
                  type="text"
                  required
                  disabled={!!editingModel}
                  value={modelName}
                  onChange={(e) => setModelName(e.target.value)}
                  placeholder="e.g. smart, fast, code"
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white font-mono focus:outline-none focus:border-brand-500 disabled:opacity-50"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-slate-300 mb-1">Description</label>
                <input
                  type="text"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  placeholder="Primary high-capability reasoner with instant failover"
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-brand-500"
                />
              </div>

              {/* Target Ladder Rows */}
              <div>
                <div className="flex items-center justify-between mb-2">
                  <label className="text-xs font-medium text-slate-300">
                    Prioritized Target Models
                  </label>
                  <button
                    type="button"
                    onClick={handleAddTarget}
                    className="text-xs text-brand-400 hover:text-brand-300 font-medium flex items-center gap-1"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    Add Candidate
                  </button>
                </div>

                <div className="space-y-2 max-h-48 overflow-y-auto pr-1">
                  {targets.map((tgt, i) => (
                    <div key={i} className="flex items-center gap-2">
                      <span className="text-xs font-mono text-slate-500 w-4">{i + 1}.</span>
                      <input
                        type="text"
                        required
                        placeholder="Provider (e.g. nvidia)"
                        value={tgt.provider}
                        onChange={(e) => handleTargetChange(i, 'provider', e.target.value)}
                        className="w-1/3 bg-slate-950 border border-slate-800 rounded px-2.5 py-1.5 text-xs text-white focus:outline-none focus:border-brand-500"
                      />
                      <input
                        type="text"
                        required
                        placeholder="Model ID (e.g. deepseek-ai/deepseek-v4.1-flash)"
                        value={tgt.model}
                        onChange={(e) => handleTargetChange(i, 'model', e.target.value)}
                        className="flex-1 bg-slate-950 border border-slate-800 rounded px-2.5 py-1.5 text-xs text-white font-mono focus:outline-none focus:border-brand-500"
                      />
                      <button
                        type="button"
                        onClick={() => handleRemoveTarget(i)}
                        className="p-1 text-slate-500 hover:text-rose-400 rounded"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  ))}
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
                  Save Virtual Model
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
