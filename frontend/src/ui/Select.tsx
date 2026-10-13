import React from 'react';
import { cn } from './cn';

export interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  mono?: boolean;
}

export const Select = React.forwardRef<HTMLSelectElement, SelectProps>(
  ({ className, mono = false, children, ...props }, ref) => {
    return (
      <select
        ref={ref}
        className={cn(
          'bg-[#030508] border border-white/[0.08] focus:border-cyan-500/60 rounded-lg px-2.5 py-1.5 text-xs text-slate-300 focus:outline-none transition-colors cursor-pointer',
          mono && 'font-mono',
          className
        )}
        {...props}
      >
        {children}
      </select>
    );
  }
);
Select.displayName = 'Select';
