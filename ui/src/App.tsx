import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Branch, Overview, Revision } from './types';

const STORAGE_KEY = 'tome.lastRepository';

function shortHash(hash: string) {
  return hash.slice(0, 8);
}

function formatTime(ms: number) {
  if (!ms) return '';
  const date = new Date(ms);
  return date.toLocaleString(undefined, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
}

/** Lane color from the branch id, so a renamed branch keeps its color. */
function laneColor(branchId: string) {
  let hash = 0;
  for (const ch of branchId) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return `hsl(${hash % 360} 55% 52%)`;
}

export default function App() {
  const [path, setPath] = useState(() => localStorage.getItem(STORAGE_KEY) ?? '');
  const [offline, setOffline] = useState(true);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [history, setHistory] = useState<Revision[]>([]);
  const [branchName, setBranchName] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [commands, setCommands] = useState<string[]>([]);

  async function open() {
    if (!path.trim()) return;
    setBusy(true);
    setError('');
    try {
      const result = await invoke<Overview>('open_repository', { path: path.trim(), offline });
      localStorage.setItem(STORAGE_KEY, path.trim());
      setOverview(result);
      setHistory(result.history);
      setBranchName(result.status.branch_name);
      setSelected(result.history[0]?.id ?? null);
      setCommands(result.commands);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function showBranch(branch: Branch) {
    if (!overview) return;
    setBusy(true);
    setError('');
    try {
      const revisions = await invoke<Revision[]>('branch_history', { path: path.trim(), branch: branch.name, offline });
      setHistory(revisions);
      setBranchName(branch.name);
      setSelected(revisions[0]?.id ?? null);
      setCommands([`lore history 200 --branch ${branch.name}${offline ? ' --offline' : ''}`]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (path) void open();
    // Open the last repository once at start.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const revision = useMemo(() => history.find((r) => r.id === selected) ?? null, [history, selected]);
  const status = overview?.status;
  const branches = (overview?.branches ?? []).filter((b) => !b.archived);

  return (
    <div className="app">
      <header className="toolbar">
        <span className="brand">TOME</span>
        <form
          className="open-form"
          onSubmit={(e) => {
            e.preventDefault();
            void open();
          }}
        >
          <label className="visually-hidden" htmlFor="repo-path">작업본 경로</label>
          <input id="repo-path" value={path} onChange={(e) => setPath(e.target.value)} placeholder="Lore 작업본 경로 (예: C:\Project\SampleProject)" spellCheck={false} />
          <label className="check">
            <input type="checkbox" checked={offline} onChange={(e) => setOffline(e.target.checked)} />
            오프라인
          </label>
          <button type="submit" disabled={busy}>{busy ? '여는 중…' : '열기'}</button>
        </form>
      </header>

      {error && <div className="error" role="alert">{error}</div>}

      <main className="panes">
        <nav className="pane branches" aria-label="브랜치">
          <h2>브랜치</h2>
          <ul>
            {branches.map((b) => (
              <li key={b.id}>
                <button className={b.name === branchName ? 'item active' : 'item'} onClick={() => void showBranch(b)}>
                  <span className="lane-dot" style={{ background: laneColor(b.id) }} />
                  <span className="name">{b.name}</span>
                  {b.current && <span className="badge">현재</span>}
                </button>
              </li>
            ))}
          </ul>
          {status && (
            <section className="working">
              <h2>작업본</h2>
              <p>
                {status.branch_name} · r{status.revision_number}
                {status.local_ahead && <span className="badge ahead">push 대기</span>}
                {status.remote_ahead && <span className="badge behind">새 버전</span>}
              </p>
              <p className="muted">{status.files.length ? `변경 파일 ${status.files.length}개` : '변경 없음'}</p>
            </section>
          )}
        </nav>

        <section className="pane smartlog" aria-label="히스토리">
          <h2>
            {branchName || '히스토리'} <span className="muted">{history.length}개 리비전</span>
          </h2>
          <ol className="log">
            {history.map((r) => (
              <li key={r.id}>
                <button className={r.id === selected ? 'row active' : 'row'} onClick={() => setSelected(r.id)}>
                  <span className="graph" aria-hidden="true">
                    <span className="node" style={{ borderColor: laneColor(r.branch_id) }} />
                    {r.parents.length > 1 && <span className="merge-mark">⑂</span>}
                  </span>
                  <span className="number">r{r.number}</span>
                  <span className="message">{r.message || '(메시지 없음)'}</span>
                  <span className="author">{r.author}</span>
                  <span className="time">{formatTime(r.timestamp)}</span>
                </button>
              </li>
            ))}
          </ol>
        </section>

        <aside className="pane details" aria-label="리비전 상세">
          {revision ? (
            <>
              <h2>r{revision.number}</h2>
              <p className="message-full">{revision.message}</p>
              <dl>
                <dt>작성</dt>
                <dd>{revision.author}</dd>
                <dt>시각</dt>
                <dd>{formatTime(revision.timestamp)}</dd>
                <dt>리비전</dt>
                <dd className="mono">{shortHash(revision.id)}</dd>
                <dt>부모</dt>
                <dd className="mono">{revision.parents.map(shortHash).join(' · ') || '없음'}</dd>
              </dl>
              {revision.metadata.length > 0 && (
                <div className="badges">
                  {revision.metadata.map(([key, value]) => (
                    <span key={key} className="badge meta">
                      {key}: {typeof value === 'string' ? value : JSON.stringify(value)}
                    </span>
                  ))}
                </div>
              )}
            </>
          ) : (
            <p className="muted">리비전을 고르세요</p>
          )}
        </aside>
      </main>

      <footer className="statusbar" aria-label="실행한 Lore 명령">
        {commands.map((c) => (
          <code key={c}>{c}</code>
        ))}
      </footer>
    </div>
  );
}
