import React from 'react';
import { cn } from './cn';

export type BadgeVariant =
  | 'primary'
  | 'solid-primary'
  | 'secondary'
  | 'solid-secondary'
  | 'emerald'
  | 'cyan'
  | 'amber'
  | 'rose'
  | 'purple'
  | 'neutral'
  | 'outline';
export type BadgeSize = 'xs' | 'sm';

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
  size?: BadgeSize;
}

export const Badge = React.forwardRef<HTMLSpanElement, BadgeProps>(
  ({ className, variant = 'neutral', size = 'sm', children, ...props }, ref) => {
    const baseClasses =
      'inline-flex items-center gap-1 font-mono font-medium rounded border select-none transition-all shrink-0';

    const variantClasses: Record<BadgeVariant, string> = {
      primary:
        'bg-[var(--primary-bg-badge)] border-[var(--primary-border)] text-[var(--primary)] shadow-[0_0_8px_var(--primary-glow-subtle)] font-bold',
      'solid-primary':
        'bg-[var(--primary)] border-transparent text-black font-extrabold shadow-[0_0_12px_var(--primary-glow)]',
      secondary:
        'bg-[var(--secondary-bg-badge)] border-[var(--secondary-border)] text-[var(--secondary)] shadow-[0_0_8px_var(--secondary-glow-subtle)] font-bold',
      'solid-secondary':
        'bg-[var(--secondary)] border-transparent text-black font-extrabold shadow-[0_0_12px_var(--secondary-glow)]',
      cyan: 'bg-[var(--secondary-bg-badge)] border-[var(--secondary-border)] text-[var(--secondary)] shadow-[0_0_8px_var(--secondary-glow-subtle)]',
      emerald: 'bg-emerald-950/70 border-emerald-700/60 text-emerald-400 font-semibold',
      amber: 'bg-amber-950/70 border-amber-700/60 text-amber-400 font-semibold',
      rose: 'bg-rose-950/70 border-rose-700/60 text-rose-400 font-semibold',
      purple: 'bg-purple-950/70 border-purple-700/60 text-purple-300 font-semibold',
      neutral: 'bg-white/[0.05] border-white/10 text-slate-300 font-medium',
      outline: 'bg-transparent border-white/15 text-slate-400 font-medium',
    };

    const sizeClasses: Record<BadgeSize, string> = {
      xs: 'text-[9px] px-1.5 py-0.2',
      sm: 'text-[10px] px-2 py-0.5',
    };

    return (
      <span
        ref={ref}
        className={cn(baseClasses, variantClasses[variant], sizeClasses[size], className)}
        {...props}
      >
        {children}
      </span>
    );
  }
);

Badge.displayName = 'Badge';
