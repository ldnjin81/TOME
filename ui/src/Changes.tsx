import { useEffect, useState } from 'react';
import type { ChangedFile, Resolution, Status } from './types';

const ACTION_LABEL: Record<string, string> = { add: '추가', modify: '수정', delete: '삭제', move: '이동' };

interface Props {
  status: Status;
  busy: boolean;
  onRefresh: () => void;
  onStage: (paths: string[], stage: boolean) => void;
  onCommit: (message: string, push: boolean) => Promise<boolean>;
  onPush: () => void;
  /** Right-click on a file: the custom tools menu. */
  onContext: (e: React.MouseEvent, files: string[]) => void;
  /** Show the file's uncommitted diff. */
  onDiff: (path: string) => void;
  /** While a merge is in progress: what is being merged (branch name if known). */
  mergeLabel: string;
  /** Settle conflicted files, or with null mark them conflicted again. */
  onResolve: (paths: string[], how: Resolution | null) => void;
  onAbortMerge: () => void;
  /** Paths locked on the server (lock board), marked on the files. */
  locked: Set<string>;
}

const RESOLUTION_LABEL: Record<string, string> = { mine: '내 것', theirs: '상대 것', edited: '직접 수정', auto: '자동 병합' };

/** The files of a merge in progress that conflicted, with how each is (to be) settled. */
function Conflicts({ files, label, busy, onResolve, onAbort, onDiff }: { files: ChangedFile[]; label: string; busy: boolean; onResolve: (paths: string[], how: Resolution | null) => void; onAbort: () => void; onDiff: (path: string) => void }) {
  const open = files.filter((f) => f.unresolved);
  return (
    <section className="conflicts">
      <h3>
        병합 중 <span className="mono">{label}</span>
        <span className={open.length ? 'tag conflict-tag' : 'tag'}>{open.length ? `충돌 ${open.length}개 남음` : '충돌 모두 해결'}</span>
        <button className="link danger" onClick={onAbort} disabled={busy}>
          병합 중단
        </button>
      </h3>
      <p className="muted small">텍스트 파일은 충돌 표시(&lt;&lt;&lt;&lt;&lt;&lt;&lt;)를 직접 고친 뒤 “수정 완료”를 누릅니다. 바이너리 에셋은 내 것과 상대 것 중 하나를 고릅니다.</p>
      <ul className="file-list">
        {files.map((f) => (
          <li key={f.path} className="conflict-row">
            <span className={f.unresolved ? 'cstate open' : 'cstate'}>{f.unresolved ? '미해결' : RESOLUTION_LABEL[f.resolution] ?? f.resolution}</span>
            <span className="file-path mono" title={f.path}>
              {f.path}
            </span>
            <span className="conflict-actions">
              {f.unresolved ? (
                <>
                  <button className="ghost small-btn" onClick={() => onResolve([f.path], 'mine')} disabled={busy} title="현재 브랜치의 내용을 씁니다">
                    내 것
                  </button>
                  <button className="ghost small-btn" onClick={() => onResolve([f.path], 'theirs')} disabled={busy} title="병합하는 브랜치의 내용을 씁니다">
                    상대 것
                  </button>
                  <button className="ghost small-btn" onClick={() => onResolve([f.path], 'edited')} disabled={busy} title="직접 고친 지금 내용을 씁니다">
                    수정 완료
                  </button>
                </>
              ) : (
                <button className="ghost small-btn" onClick={() => onResolve([f.path], null)} disabled={busy}>
                  되돌리기
                </button>
              )}
              <button className="diff-btn" onClick={() => onDiff(f.path)}>
                diff
              </button>
            </span>
          </li>
        ))}
      </ul>
      {open.length > 1 && (
        <div className="all-buttons">
          <button className="link" onClick={() => onResolve(open.map((f) => f.path), 'mine')} disabled={busy}>
            남은 것 모두 내 것
          </button>
          <button className="link" onClick={() => onResolve(open.map((f) => f.path), 'theirs')} disabled={busy}>
            남은 것 모두 상대 것
          </button>
        </div>
      )}
    </section>
  );
}

function FileRow({ file, onToggle, busy, onContext, onDiff, locked }: { file: ChangedFile; onToggle: () => void; busy: boolean; onContext: (e: React.MouseEvent, files: string[]) => void; onDiff: (path: string) => void; locked: boolean }) {
  return (
    <li className="file-li">
      <button className="file-row" onClick={onToggle} onContextMenu={(e) => onContext(e, [file.path])} disabled={busy} title={file.staged ? '스테이징 해제' : '스테이징'}>
        <span className={`action ${file.action}`}>{ACTION_LABEL[file.action] ?? file.action}</span>
        <span className="file-path mono">{file.path}</span>
        {file.conflict && <span className="badge conflict">충돌</span>}
        {locked && (
          <span className="badge lock" title="서버에 잠긴 파일입니다. 잠근 사람이 풀기 전에는 push할 때 거부될 수 있습니다">
            잠김
          </span>
        )}
        <span className="move" aria-hidden="true">{file.staged ? '−' : '+'}</span>
      </button>
      {file.action !== 'add' && file.action !== 'delete' && (
        <button className="diff-btn" onClick={() => onDiff(file.path)} title="변경 내용 보기" aria-label={`${file.path} 변경 내용 보기`}>
          diff
        </button>
      )}
    </li>
  );
}

/** The working copy's changes: stage, commit, push. */
export default function Changes({ status, busy, onRefresh, onStage, onCommit, onPush, onContext, onDiff, mergeLabel, onResolve, onAbortMerge, locked }: Props) {
  const [message, setMessage] = useState('');
  const merging = !!status.merging;
  const conflicts = status.files.filter((f) => !f.directory && f.conflict);
  const unresolved = conflicts.filter((f) => f.unresolved).length;
  // A merge in progress suggests its commit message.
  useEffect(() => {
    if (merging) setMessage((m) => m || `Merge ${mergeLabel}`);
  }, [merging, mergeLabel]);
  const files = status.files.filter((f) => !f.directory && !(merging && f.conflict));
  const staged = files.filter((f) => f.staged);
  const unstaged = files.filter((f) => !f.staged);
  const canCommit = !busy && (staged.length > 0 || merging) && unresolved === 0 && message.trim().length > 0;

  async function commit(push: boolean) {
    if (await onCommit(message.trim(), push)) setMessage('');
  }

  return (
    <div className="changes">
      <div className="pane-actions">
        <button className="ghost" onClick={onRefresh} disabled={busy}>다시 검사</button>
        {status.local_ahead && (
          <button className="ghost" onClick={onPush} disabled={busy || merging} title={merging ? '병합을 끝낸 뒤 push합니다' : undefined}>push</button>
        )}
      </div>

      {merging && <Conflicts files={conflicts} label={mergeLabel} busy={busy} onResolve={onResolve} onAbort={onAbortMerge} onDiff={onDiff} />}

      <section>
        <h3>
          스테이징됨 <span className="muted">{staged.length}</span>
          {staged.length > 0 && (
            <button className="link" onClick={() => onStage(staged.map((f) => f.path), false)} disabled={busy}>모두 해제</button>
          )}
        </h3>
        <ul className="file-list">
          {staged.map((f) => <FileRow key={f.path} file={f} busy={busy} onContext={onContext} onDiff={onDiff} locked={locked.has(f.path)} onToggle={() => onStage([f.path], false)} />)}
        </ul>
      </section>

      <section>
        <h3>
          변경 <span className="muted">{unstaged.length}</span>
          {unstaged.length > 0 && (
            <button className="link" onClick={() => onStage(unstaged.map((f) => f.path), true)} disabled={busy}>모두 스테이징</button>
          )}
        </h3>
        {unstaged.length === 0 && staged.length === 0 ? (
          <p className="muted">변경 없음</p>
        ) : (
          <ul className="file-list">
            {unstaged.map((f) => <FileRow key={f.path} file={f} busy={busy} onContext={onContext} onDiff={onDiff} locked={locked.has(f.path)} onToggle={() => onStage([f.path], true)} />)}
          </ul>
        )}
      </section>

      <form
        className="commit-box"
        onSubmit={(e) => {
          e.preventDefault();
          if (canCommit) void commit(false);
        }}
      >
        <label htmlFor="commit-message">커밋 메시지</label>
        <textarea id="commit-message" value={message} onChange={(e) => setMessage(e.target.value)} rows={3} placeholder={merging ? `Merge ${mergeLabel}` : '무엇을 바꿨나요'} />
        {merging && unresolved > 0 && <p className="muted small">충돌을 모두 정하면 커밋할 수 있습니다.</p>}
        <div className="commit-buttons">
          <button type="submit" className="primary" disabled={!canCommit}>커밋</button>
          <button type="button" className="primary outline" disabled={!canCommit} onClick={() => void commit(true)}>커밋 후 push</button>
        </div>
      </form>
    </div>
  );
}
