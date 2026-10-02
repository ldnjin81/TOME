import type { JobProgress } from './types';

function size(bytes: number) {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/** A running clone, sync or push: what it is, how far, and a cancel button. */
export default function JobPanel({ label, progress, cancelling, onCancel }: { label: string; progress: JobProgress | null; cancelling: boolean; onCancel: () => void }) {
  const fraction = progress
    ? progress.bytes_total > 0
      ? progress.bytes / progress.bytes_total
      : progress.total > 0
        ? progress.done / progress.total
        : null
    : null;
  const unit = progress?.phase === 'push' ? '조각' : '파일';
  return (
    <section className="job-panel" aria-label="진행 중인 작업">
      <div className="job-head">
        <strong>{label}</strong>
        <span className="muted job-numbers">
          {progress && progress.total > 0 && `${unit} ${progress.done.toLocaleString()} / ${progress.total.toLocaleString()}`}
          {progress && progress.bytes_total > 0 && ` · ${size(progress.bytes)} / ${size(progress.bytes_total)}`}
          {!progress && '준비 중…'}
        </span>
        <button className="ghost small-btn" onClick={onCancel} disabled={cancelling} title="작업을 멈춥니다. 다시 실행하면 이어서 진행합니다">
          {cancelling ? '취소하는 중…' : '취소'}
        </button>
      </div>
      <div className="job-bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={fraction === null ? undefined : Math.round(fraction * 100)}>
        <div className={fraction === null ? 'job-fill busy' : 'job-fill'} style={fraction === null ? undefined : { width: `${Math.max(2, fraction * 100)}%` }} />
      </div>
    </section>
  );
}
