import { t, getLocale } from './i18n';
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
  return ms ? new Date(ms).toLocaleString(getLocale(), { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '';
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
        <input className="filter" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={t("경로·작업자 찾기")} aria-label={t("잠금 찾기")} />
        <button className="ghost" onClick={onRefresh} disabled={busy}>{t("새로 고침")}</button>
      </div>
      <p className="muted summary">
        {t('{p0} · 잠금 {p1}개 · 작업자 {p2}명', { p0: branch, p1: locks.length, p2: owners })}
      </p>

      {shown.length === 0 ? (
        <p className="muted">{locks.length ? t("찾는 잠금이 없습니다") : t("잠긴 파일이 없습니다")}</p>
      ) : (
        <div className="table-scroll">
          <table className="lock-table">
            <thead>
              <tr>
                <th>{t("파일")}</th>
                <th>{t("작업자")}</th>
                <th>{t("잠근 시각")}</th>
                <th aria-label={t("해제")} />
              </tr>
            </thead>
            <tbody>
              {shown.map((l) => (
                <tr key={l.path} onContextMenu={(e) => onContext(e, [l.path])}>
                  <td className="mono">{l.path}</td>
                  <td>
                    {l.owner === '<unknown>' ? (
                      <span className="muted" title={t("인증 없는 서버는 잠근 사람을 기록하지 않습니다(Lore 서버가 소유자를 로그인 토큰에서만 가져옴)")}>{t("알 수 없음")}</span>
                    ) : l.owner === me ? (
                      <strong>{t("나")}</strong>
                    ) : (
                      l.owner
                    )}
                  </td>
                  <td className="time">{formatTime(l.locked_at)}</td>
                  <td>
                    <button className="link" onClick={() => onLock([l.path], false)} disabled={busy}>{t("해제")}</button>
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
        <label htmlFor="lock-path">{t("작업 시작(잠금)")}</label>
        <input id="lock-path" className="mono" value={newPath} onChange={(e) => setNewPath(e.target.value)} placeholder="Content/Characters/Hero.uasset" spellCheck={false} />
        <button type="submit" className="primary" disabled={busy || !newPath.trim()}>{t("잠금")}</button>
      </form>
    </div>
  );
}
