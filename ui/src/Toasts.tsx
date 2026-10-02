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
      {toasts.slice(-4).map((t) => (
        <div key={t.id} className={`toast ${t.tone}`}>
          <span>{t.text}</span>
          <button className="toast-x" onClick={() => onDone(t.id)} aria-label="닫기">
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
