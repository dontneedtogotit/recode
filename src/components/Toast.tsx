import { createContext, useContext, useState, useCallback, ReactNode } from 'react';

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export type Toast = {
  id: string;
  type: ToastType;
  title: string;
  message?: string;
  duration?: number;
};

type ToastContextValue = {
  toasts: Toast[];
  showToast: (toast: Omit<Toast, 'id'>) => string;
  dismissToast: (id: string) => void;
};

const ToastContext = createContext<ToastContextValue | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const showToast = useCallback((toast: Omit<Toast, 'id'>) => {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    const newToast = { ...toast, id, duration: toast.duration ?? 4000 };
    setToasts((prev) => [...prev, newToast]);
    
    if (newToast.duration && newToast.duration > 0) {
      setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
      }, newToast.duration);
    }
    
    return id;
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ toasts, showToast, dismissToast }}>
      {children}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </ToastContext.Provider>
  );
}

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider');
  }
  return context;
}

function ToastContainer({ toasts, onDismiss }: { toasts: Toast[]; onDismiss: (id: string) => void }) {
  if (toasts.length === 0) return null;

  return (
    <div
      className="toast-container"
      style={{
        position: 'fixed',
        bottom: 24,
        right: 24,
        zIndex: 100,
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        pointerEvents: 'none',
      }}
      aria-live="polite"
      aria-atomic="true"
    >
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: (id: string) => void }) {
  const typeStyles: Record<ToastType, { bg: string; border: string; icon: string }> = {
    success: { bg: 'var(--success-soft)', border: 'rgba(34, 197, 94, 0.32)', icon: '✓' },
    error: { bg: 'var(--danger-soft)', border: 'rgba(239, 68, 68, 0.32)', icon: '✕' },
    warning: { bg: 'var(--warning-soft)', border: 'rgba(245, 158, 11, 0.32)', icon: '⚠' },
    info: { bg: 'var(--info-soft)', border: 'rgba(96, 165, 250, 0.32)', icon: 'ℹ' },
  };

  const style = typeStyles[toast.type];

  return (
    <div
      className="toast-item"
      style={{
        pointerEvents: 'auto',
        display: 'flex',
        alignItems: 'flex-start',
        gap: 10,
        padding: '12px 14px',
        background: style.bg,
        border: `1px solid ${style.border}`,
        borderRadius: 'var(--radius-md)',
        boxShadow: 'var(--shadow-soft)',
        minWidth: 280,
        maxWidth: 420,
        animation: 'toastIn 200ms ease-out',
      }}
    >
      <span style={{ fontSize: 16, color: style.border.replace('0.32', '1'), flexShrink: 0 }}>
        {style.icon}
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14, color: 'var(--text-primary)' }}>
          {toast.title}
        </div>
        {toast.message && (
          <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginTop: 2 }}>
            {toast.message}
          </div>
        )}
      </div>
      <button
        type="button"
        onClick={() => onDismiss(toast.id)}
        style={{
          background: 'transparent',
          border: 'none',
          color: 'var(--text-muted)',
          cursor: 'pointer',
          padding: 2,
          fontSize: 16,
          lineHeight: 1,
          flexShrink: 0,
        }}
        aria-label="Dismiss"
      >
        ✕
      </button>
    </div>
  );
}