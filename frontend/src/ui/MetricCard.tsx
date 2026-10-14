import React from 'react';
import { cn } from './cn';

export interface MetricCardProps {
  label: string;
  value: React.ReactNode;
  icon?: React.ReactNode;
  subtext?: React.ReactNode;
  badge?: React.ReactNode;
  accent?: 'primary' | 'secondary' | 'emerald' | 'cyan' | 'amber' | 'purple' | 'slate';
  className?: string;
}

export const MetricCard: React.FC<MetricCardProps> = ({
  label,
  value,
  icon,
  subtext,
  badge,
  accent = 'primary',
  className,
}) => {
  const iconAccentClasses = {
    primary: 'bg-[var(--primary-bg-badge)] border-[var(--primary-border)] text-[var(--primary)]',
    secondary: 'bg-[var(--secondary-bg-badge)] border-[var(--secondary-border)] text-[var(--secondary)]',
    cyan: 'bg-[var(--secondary-bg-badge)] border-[var(--secondary-border)] text-[var(--secondary)]',
    emerald: 'bg-emerald-950/40 border-emerald-800/40 text-emerald-400',
    amber: 'bg-amber-950/40 border-amber-800/40 text-amber-400',
    purple: 'bg-purple-950/40 border-purple-800/40 text-purple-400',
    slate: 'bg-white/[0.04] border-white/[0.08] text-slate-400',
  };

  const glowAccentClasses = {
    primary: 'group-hover:border-[var(--primary-border)]',
    secondary: 'group-hover:border-[var(--secondary-border)]',
    cyan: 'group-hover:border-[var(--secondary-border)]',
    emerald: 'group-hover:border-emerald-500/30',
    amber: 'group-hover:border-amber-500/30',
    purple: 'group-hover:border-purple-500/30',
    slate: 'group-hover:border-white/20',
  };

  return (
    <div
      className={cn(
        'bg-[var(--bg-card)] border border-[var(--border-subtle)] rounded-xl p-5 shadow-2xl relative overflow-hidden transition-all group',
        'hover:border-[var(--primary-border)] hover:shadow-[0_0_25px_var(--primary-glow-subtle)]',
        className
      )}
    >
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-mono uppercase tracking-wider text-slate-400">
          {label}
        </span>
        <div className="flex items-center gap-2">
          {badge}
          {icon && (
            <div className={cn('p-1.5 rounded-lg border', iconAccentClasses[accent])}>
              {icon}
            </div>
          )}
        </div>
      </div>
      <div className="mt-3 text-3xl font-extrabold text-white font-mono tracking-tight group-hover:text-white transition-colors">
        {value}
      </div>
      {subtext && <div className="mt-2 text-[11px] font-mono text-slate-500">{subtext}</div>}
    </div>
  );
};
