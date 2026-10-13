import React from 'react';
import { cn } from './cn';

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  mono?: boolean;
}

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className, mono = false, ...props }, ref) => {
    return (
      <input
        ref={ref}
        className={cn(
          'w-full bg-[#030508] border border-white/[0.08] focus:border-cyan-500/60 rounded-lg px-3 py-1.5 text-xs text-white placeholder-slate-600 focus:outline-none transition-colors disabled:opacity-50 disabled:cursor-not-allowed',
          mono && 'font-mono',
          className
        )}
        {...props}
      />
    );
  }
);
Input.displayName = 'Input';
