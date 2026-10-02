import { useState } from 'react';
import type { Lock } from './types';

interface Props {
  branch: string;
  /** My identity, to mark my locks. */
  me: string;
  locks: Lock[];
  busy: boolean;
  onRefresh: () => void;
  onLock: (paths: string[], lock: boolean) => void;
  onContext: (e: React.MouseEvent, files: string[]) => void;
}

function formatTime(ms: number) {
  return ms ? new Date(ms).toLocaleString(undefined, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '';
}

/** Every lock on the branch: who is working on which file. */
export default function LockBoard({ branch, me, locks, busy, onRefresh, onLock, onContext }: Props) {
  const [filter, setFilter] = useState('');
  const [newPath, setNewPath] = useState('');
  const needle = filter.trim().toLowerCase();
  const shown = locks.filter((l) => !needle || l.path.toLowerCase().includes(needle) || l.owner.toLowerCase().includes(needle));
  const owners = new Set(locks.map((l) => l.owner)).size;

  return (
    <div className="locks">
      <div className="pane-actions">
        <input className="filter" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="경로·작업자 찾기" aria-label="잠금 찾기" />
        <button className="ghost" onClick={onRefresh} disabled={busy}>새로 고침</button>
      </div>
      <p className="muted summary">
        {branch} · 잠금 {locks.length}개 · 작업자 {owners}명
      </p>

      {shown.length === 0 ? (
        <p className="muted">{locks.length ? '찾는 잠금이 없습니다' : '잠긴 파일이 없습니다'}</p>
      ) : (
        <div className="table-scroll">
          <table className="lock-table">
            <thead>
              <tr>
                <th>파일</th>
                <th>작업자</th>
                <th>잠근 시각</th>
                <th aria-label="해제" />
              </tr>
            </thead>
            <tbody>
              {shown.map((l) => (
                <tr key={l.path} onContextMenu={(e) => onContext(e, [l.path])}>
                  <td className="mono">{l.path}</td>
                  <td>
                    {l.owner === '<unknown>' ? (
                      <span className="muted" title="인증 없는 서버는 잠근 사람을 기록하지 않습니다(Lore 서버가 소유자를 로그인 토큰에서만 가져옴)">알 수 없음</span>
                    ) : l.owner === me ? (
                      <strong>나</strong>
                    ) : (
                      l.owner
                    )}
                  </td>
                  <td className="time">{formatTime(l.locked_at)}</td>
                  <td>
                    <button className="link" onClick={() => onLock([l.path], false)} disabled={busy}>해제</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <form
        className="lock-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (newPath.trim()) {
            onLock([newPath.trim()], true);
            setNewPath('');
          }
        }}
      >
        <label htmlFor="lock-path">작업 시작(잠금)</label>
        <input id="lock-path" className="mono" value={newPath} onChange={(e) => setNewPath(e.target.value)} placeholder="Content/Characters/Hero.uasset" spellCheck={false} />
        <button type="submit" className="primary" disabled={busy || !newPath.trim()}>잠금</button>
      </form>
    </div>
  );
}
