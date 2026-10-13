import React from 'react';
import { cn } from './cn';

export type BadgeVariant = 'emerald' | 'cyan' | 'amber' | 'rose' | 'purple' | 'neutral' | 'outline';
export type BadgeSize = 'xs' | 'sm';

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
  size?: BadgeSize;
}

export const Badge = React.forwardRef<HTMLSpanElement, BadgeProps>(
  ({ className, variant = 'neutral', size = 'sm', children, ...props }, ref) => {
    const baseClasses =
      'inline-flex items-center gap-1 font-mono font-medium rounded border select-none transition-colors shrink-0';

    const variantClasses: Record<BadgeVariant, string> = {
      emerald: 'bg-emerald-950/60 border-emerald-800/60 text-emerald-400',
      cyan: 'bg-cyan-950/60 border-cyan-800/60 text-cyan-300',
      amber: 'bg-amber-950/60 border-amber-800/60 text-amber-400',
      rose: 'bg-rose-950/60 border-rose-800/60 text-rose-400',
      purple: 'bg-purple-950/60 border-purple-800/60 text-purple-300',
      neutral: 'bg-white/[0.04] border-white/[0.08] text-slate-300',
      outline: 'bg-transparent border-white/[0.12] text-slate-400',
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
