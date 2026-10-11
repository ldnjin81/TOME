import { t } from './i18n';
import { useEffect } from 'react';

export interface Toast {
  id: number;
  text: string;
  tone: 'lock' | 'unlock' | 'push' | 'info';
}

/** Short notices in the corner; each one leaves after a few seconds. */
export default function Toasts({ toasts, onDone }: { toasts: Toast[]; onDone: (id: number) => void }) {
  useEffect(() => {
    const timers = toasts.map((t) => window.setTimeout(() => onDone(t.id), 7000));
    return () => timers.forEach(window.clearTimeout);
  }, [toasts, onDone]);
  if (toasts.length === 0) return null;
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.slice(-4).map((toast) => (
        <div key={toast.id} className={`toast ${toast.tone}`}>
          <span>{toast.text}</span>
          <button className="toast-x" onClick={() => onDone(toast.id)} aria-label={t("닫기")}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
