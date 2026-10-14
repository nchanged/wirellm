import React, { useState, useRef, useEffect } from 'react';
import { Palette, Check } from 'lucide-react';
import { useTheme, THEMES, ThemeId } from '../theme';
import { cn } from '../ui';

export const ThemeSelector: React.FC = () => {
  const { theme, setTheme, currentConfig } = useTheme();
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen]);

  return (
    <div ref={containerRef} className="relative font-mono">
      {/* Trigger Button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className={cn(
          'h-8 px-2.5 rounded-lg border text-xs flex items-center gap-2 transition-all select-none',
          isOpen ? 'border-[var(--primary)] shadow-[0_0_12px_var(--primary-glow-subtle)]' : 'border-white/15 hover:border-white/30'
        )}
        style={{ backgroundColor: '#05070f' }}
        title="Change website color theme"
      >
        <Palette className="w-3.5 h-3.5 text-[var(--primary)]" />
        <div className="flex items-center -space-x-1 shrink-0">
          <span
            className="w-2.5 h-2.5 rounded-full inline-block shrink-0 shadow-[0_0_6px_currentColor] border border-white/20 z-10"
            style={{ backgroundColor: currentConfig.primary, color: currentConfig.primary }}
          />
          <span
            className="w-2.5 h-2.5 rounded-full inline-block shrink-0 shadow-[0_0_6px_currentColor] border border-white/20"
            style={{ backgroundColor: currentConfig.secondary, color: currentConfig.secondary }}
          />
        </div>
        <span className="hidden sm:inline font-semibold text-white">
          {currentConfig.shortName}
        </span>
      </button>

      {/* Dropdown Menu - 100% Solid Pitch Black with Ultra High Contrast */}
      {isOpen && (
        <div
          className="absolute right-0 mt-2 w-80 rounded-2xl border-2 border-[var(--primary-border)] shadow-[0_25px_60px_rgba(0,0,0,0.98),0_0_35px_var(--primary-glow-subtle)] p-2 z-50 animate-in fade-in zoom-in-95 duration-150"
          style={{ backgroundColor: '#04060c' }}
        >
          {/* Header */}
          <div
            className="px-3 py-2.5 rounded-xl border border-white/10 mb-2 flex items-center justify-between"
            style={{ backgroundColor: '#080c16' }}
          >
            <div>
              <div className="text-[11px] font-extrabold text-white tracking-widest flex items-center gap-2">
                <span>COLOR THEME</span>
                <span className="text-[9px] px-1.5 py-0.2 rounded font-mono font-bold uppercase tracking-wider text-[var(--primary)] bg-[var(--primary-bg-badge)] border border-[var(--primary-border)]">
                  Active
                </span>
              </div>
              <p className="text-[10px] text-slate-300 mt-0.5 font-sans font-medium">
                Dual-tone cybernetic palettes
              </p>
            </div>
            <div className="flex items-center -space-x-1.5 shrink-0">
              <span
                className="w-3.5 h-3.5 rounded-full shrink-0 shadow-[0_0_10px_currentColor] border border-white/40 z-10"
                style={{ backgroundColor: currentConfig.primary, color: currentConfig.primary }}
              />
              <span
                className="w-3.5 h-3.5 rounded-full shrink-0 shadow-[0_0_10px_currentColor] border border-white/40"
                style={{ backgroundColor: currentConfig.secondary, color: currentConfig.secondary }}
              />
            </div>
          </div>

          {/* Theme Options */}
          <div className="space-y-1.5 max-h-[380px] overflow-y-auto pr-0.5">
            {THEMES.map((item) => {
              const isSelected = item.id === theme;
              return (
                <button
                  key={item.id}
                  onClick={() => {
                    setTheme(item.id as ThemeId);
                    setIsOpen(false);
                  }}
                  className={cn(
                    'w-full px-3 py-2.5 rounded-xl text-left text-xs flex items-center justify-between gap-3 transition-all group border',
                    isSelected
                      ? 'border-[var(--primary)] text-white shadow-[0_0_16px_var(--primary-glow-subtle)]'
                      : 'border-white/10 hover:border-white/25 text-slate-200 hover:text-white'
                  )}
                  style={{
                    backgroundColor: isSelected ? 'var(--primary-bg-badge)' : '#090d18',
                  }}
                >
                  <div className="flex items-center gap-3 min-w-0">
                    <div className="flex items-center -space-x-1.5 shrink-0">
                      <span
                        className="w-4 h-4 rounded-full inline-block shrink-0 transition-transform group-hover:scale-110 border border-white/30 z-10"
                        style={{
                          backgroundColor: item.primary,
                          boxShadow: `0 0 10px ${item.glow}`,
                        }}
                      />
                      <span
                        className="w-4 h-4 rounded-full inline-block shrink-0 transition-transform group-hover:scale-110 border border-white/30"
                        style={{
                          backgroundColor: item.secondary,
                          boxShadow: `0 0 10px ${item.secondaryGlow}`,
                        }}
                      />
                    </div>
                    <div className="min-w-0">
                      <div className="font-bold text-white text-xs truncate flex items-center gap-1.5">
                        <span>{item.name}</span>
                        {item.id === 'magenta' && (
                          <span className="text-[9px] font-mono font-bold px-1.5 py-0.2 rounded bg-[#2a0018] text-[#ff1493] border border-[#ff1493]/60">
                            DEFAULT
                          </span>
                        )}
                      </div>
                      <div className="text-[10px] text-slate-300 group-hover:text-slate-200 truncate mt-0.5 font-sans font-medium">
                        {item.description}
                      </div>
                    </div>
                  </div>

                  {isSelected ? (
                    <Check className="w-4 h-4 text-[var(--primary)] shrink-0 drop-shadow-[0_0_6px_var(--primary)]" />
                  ) : (
                    <span
                      className="w-2 h-2 rounded-full opacity-0 group-hover:opacity-40 transition-opacity shrink-0"
                      style={{ backgroundColor: item.primary }}
                    />
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
};
