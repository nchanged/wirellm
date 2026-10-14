import React, { useEffect } from 'react';
import { X } from 'lucide-react';
import { cn } from './cn';
import { Button } from './Button';

export interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: React.ReactNode;
  description?: React.ReactNode;
  icon?: React.ReactNode;
  maxWidth?: 'sm' | 'md' | 'lg' | 'xl';
  children: React.ReactNode;
  footer?: React.ReactNode;
}

export const Modal: React.FC<ModalProps> = ({
  isOpen,
  onClose,
  title,
  description,
  icon,
  maxWidth = 'md',
  children,
  footer,
}) => {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const maxWidthClasses = {
    sm: 'max-w-sm',
    md: 'max-w-lg',
    lg: 'max-w-2xl',
    xl: 'max-w-4xl',
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/85 backdrop-blur-md flex items-center justify-center p-4">
      <div
        className={cn(
          'bg-[#070a12] border border-white/[0.1] rounded-2xl shadow-[0_0_50px_rgba(0,0,0,0.85)] w-full relative overflow-hidden',
          maxWidthClasses[maxWidth]
        )}
      >
        {/* Header */}
        <div className="p-5 pb-3 border-b border-white/[0.07] flex items-center justify-between">
          <div className="flex items-center gap-3">
            {icon && (
              <div className="p-2 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)] shrink-0 shadow-[0_0_10px_var(--primary-glow-subtle)]">
                {icon}
              </div>
            )}
            <div>
              <h3 className="text-sm font-bold text-white font-mono">{title}</h3>
              {description && <p className="text-[11px] text-slate-400 mt-0.5">{description}</p>}
            </div>
          </div>
          <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close modal">
            <X className="w-4 h-4" />
          </Button>
        </div>

        {/* Content */}
        <div className="p-5 max-h-[75vh] overflow-y-auto">{children}</div>

        {/* Footer */}
        {footer && (
          <div className="p-4 pt-3 border-t border-white/[0.07] flex items-center justify-end gap-2.5 bg-[#030508]/50">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
};
