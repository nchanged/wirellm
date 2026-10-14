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
          'bg-[#01040a] hover:bg-white/[0.04]',
          isOpen ? 'border-[var(--primary)] shadow-[0_0_12px_var(--primary-glow-subtle)]' : 'border-white/[0.08]'
        )}
        title="Change website color theme"
      >
        <Palette className="w-3.5 h-3.5 text-[var(--primary)]" />
        <span
          className="w-2 h-2 rounded-full inline-block shrink-0 shadow-[0_0_8px_currentColor]"
          style={{ backgroundColor: currentConfig.primary, color: currentConfig.primary }}
        />
        <span className="hidden sm:inline font-medium text-slate-200">
          {currentConfig.shortName}
        </span>
      </button>

      {/* Dropdown Menu */}
      {isOpen && (
        <div className="absolute right-0 mt-1.5 w-60 rounded-xl bg-[#030712]/98 border border-[var(--primary-border)] shadow-[0_12px_35px_rgba(0,0,0,0.8),0_0_20px_var(--primary-glow-subtle)] backdrop-blur-xl p-1.5 z-50 animate-in fade-in zoom-in-95 duration-150">
          <div className="px-2.5 py-1.5 border-b border-white/[0.06] mb-1">
            <div className="text-[11px] font-bold text-white tracking-wider flex items-center justify-between">
              <span>COLOR THEME</span>
              <span className="text-[10px] text-[var(--primary)]">CYBERNETIC</span>
            </div>
            <p className="text-[10px] text-slate-400 mt-0.5">
              Select a color palette for the entire dashboard
            </p>
          </div>

          <div className="space-y-0.5">
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
                    'w-full px-2.5 py-2 rounded-lg text-left text-xs flex items-center justify-between gap-2.5 transition-all group',
                    isSelected
                      ? 'bg-white/[0.08] text-white border border-[var(--primary-border)]'
                      : 'text-slate-300 hover:text-white hover:bg-white/[0.04] border border-transparent'
                  )}
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    <span
                      className="w-3 h-3 rounded-full inline-block shrink-0 transition-transform group-hover:scale-110"
                      style={{
                        backgroundColor: item.primary,
                        boxShadow: `0 0 10px ${item.glow}`,
                      }}
                    />
                    <div className="min-w-0">
                      <div className="font-semibold truncate flex items-center gap-1.5">
                        <span>{item.name}</span>
                        {item.id === 'magenta' && (
                          <span className="text-[9px] px-1 rounded bg-[#2a0018] text-[#ff1493] border border-[#ff1493]/40">
                            DEFAULT
                          </span>
                        )}
                      </div>
                      <div className="text-[10px] text-slate-400 truncate">
                        {item.description}
                      </div>
                    </div>
                  </div>

                  {isSelected && (
                    <Check className="w-3.5 h-3.5 text-[var(--primary)] shrink-0" />
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
