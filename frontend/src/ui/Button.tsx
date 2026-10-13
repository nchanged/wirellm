import React from 'react';
import { cn } from './cn';

export type ButtonVariant = 'primary' | 'cyan' | 'indigo' | 'secondary' | 'danger' | 'ghost' | 'outline';
export type ButtonSize = 'sm' | 'md' | 'lg' | 'icon';

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  isLoading?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = 'secondary', size = 'md', isLoading, disabled, children, ...props }, ref) => {
    const baseClasses =
      'inline-flex items-center justify-center font-medium transition-all select-none active:scale-[0.98] disabled:opacity-50 disabled:pointer-events-none rounded-lg focus:outline-none';

    const variantClasses: Record<ButtonVariant, string> = {
      primary:
        'bg-gradient-to-r from-cyan-500 to-cyan-400 text-black font-bold hover:from-cyan-400 hover:to-cyan-300 shadow-[0_0_16px_rgba(6,182,212,0.35)] border border-cyan-400/40',
      cyan:
        'bg-gradient-to-r from-cyan-500 to-cyan-400 text-black font-bold hover:from-cyan-400 hover:to-cyan-300 shadow-[0_0_16px_rgba(6,182,212,0.35)] border border-cyan-400/40',
      indigo:
        'bg-gradient-to-r from-indigo-500 to-indigo-400 text-white font-semibold hover:from-indigo-400 hover:to-indigo-300 shadow-[0_0_16px_rgba(99,102,241,0.35)] border border-indigo-400/40',
      secondary:
        'bg-white/[0.04] hover:bg-white/[0.08] text-slate-300 hover:text-white border border-white/[0.08] active:bg-white/[0.1] shadow-sm',
      danger:
        'bg-rose-950/40 hover:bg-rose-900/60 text-rose-300 hover:text-rose-100 border border-rose-800/60 active:bg-rose-900',
      ghost:
        'text-slate-400 hover:text-white hover:bg-white/[0.05] border border-transparent',
      outline:
        'border border-white/[0.1] text-slate-300 hover:text-white hover:bg-white/[0.04] hover:border-white/[0.2]',
    };

    const sizeClasses: Record<ButtonSize, string> = {
      sm: 'px-2.5 py-1 text-xs gap-1.5',
      md: 'px-3.5 py-1.5 text-xs gap-2',
      lg: 'px-4 py-2 text-sm gap-2',
      icon: 'p-1.5 rounded-lg shrink-0',
    };

    return (
      <button
        ref={ref}
        disabled={disabled || isLoading}
        className={cn(baseClasses, variantClasses[variant], sizeClasses[size], className)}
        {...props}
      >
        {children}
      </button>
    );
  }
);

Button.displayName = 'Button';
