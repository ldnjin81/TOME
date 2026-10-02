import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Branch, Done, Lock, Overview, Revision, Status, ViewChange } from './types';
import Changes from './Changes';
import LockBoard from './LockBoard';
import ViewDialog from './ViewDialog';

type Tab = 'history' | 'changes' | 'locks';

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
  const [tab, setTab] = useState<Tab>('history');
  const [locks, setLocks] = useState<Lock[]>([]);
  const [view, setView] = useState<{ lines: string[]; result: ViewChange | null } | null>(null);

  /** Runs a command that returns a Done<T>; shows its Lore commands and any error. */
  async function run<T>(command: string, args: Record<string, unknown>, apply: (value: T) => void): Promise<boolean> {
    setBusy(true);
    setError('');
    try {
      const done = await invoke<Done<T>>(command, { path: path.trim(), ...args });
      apply(done.value);
      setCommands(done.commands);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  }

  const setStatus = (status: Status) => setOverview((o) => (o ? { ...o, status } : o));

  async function refreshAfterCommit(status: Status) {
    setStatus(status);
    const revisions = await invoke<Revision[]>('branch_history', { path: path.trim(), branch: status.branch_name, offline: true }).catch(() => null);
    if (revisions) {
      setHistory(revisions);
      setBranchName(status.branch_name);
      setSelected(revisions[0]?.id ?? null);
    }
  }

  function showTab(next: Tab) {
    setTab(next);
    if (!overview) return;
    if (next === 'changes') void run<Status>('working_status', { offline: true }, setStatus);
    if (next === 'locks') void run<Lock[]>('lock_board', { branch: overview.status.branch_name }, setLocks);
  }

  async function openView() {
    try {
      const lines = await invoke<string[]>('read_view', { path: path.trim() });
      setView({ lines, result: null });
    } catch (e) {
      setError(String(e));
    }
  }

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
          <button type="submit" disabled={busy}>{busy ? '작업 중…' : '열기'}</button>
        </form>
        <button className="ghost" onClick={() => void openView()} disabled={!overview || busy}>View</button>
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
              <button className="link" onClick={() => showTab('changes')}>
                {status.files.some((f) => !f.directory) ? `변경 파일 ${status.files.filter((f) => !f.directory).length}개` : '변경 확인'}
              </button>
            </section>
          )}
        </nav>

        <section className="pane smartlog">
          <div className="tabs" role="tablist">
            {(
              [
                ['history', '히스토리'],
                ['changes', '변경'],
                ['locks', '잠금'],
              ] as [Tab, string][]
            ).map(([id, label]) => (
              <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? 'tab active' : 'tab'} onClick={() => showTab(id)} disabled={!overview}>
                {label}
              </button>
            ))}
          </div>
          {tab === 'changes' && status && (
            <Changes
              status={status}
              busy={busy}
              onRefresh={() => void run<Status>('working_status', { offline: true }, setStatus)}
              onStage={(paths, stage) => void run<Status>('stage_files', { paths, stage }, setStatus)}
              onCommit={(message, push) => run<Status>('commit', { message, push }, (s) => void refreshAfterCommit(s))}
              onPush={() => void run<Status>('push', { branch: status.branch_name }, setStatus)}
            />
          )}
          {tab === 'locks' && status && (
            <LockBoard
              branch={status.branch_name}
              locks={locks}
              busy={busy}
              onRefresh={() => void run<Lock[]>('lock_board', { branch: status.branch_name }, setLocks)}
              onLock={(paths, lock) => void run<Lock[]>('lock_files', { branch: status.branch_name, paths, lock }, setLocks)}
            />
          )}
          {tab === 'history' && (
          <>
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
          </>
          )}
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

      {view && (
        <ViewDialog
          initial={view.lines}
          busy={busy}
          result={view.result}
          onApply={(lines) =>
            void run<ViewChange>('apply_view', { lines }, (result) => {
              setView({ lines, result });
              // Refresh quietly, so the status bar keeps the view commands.
              invoke<Done<Status>>('working_status', { path: path.trim(), offline: true }).then((d) => setStatus(d.value), () => {});
            })
          }
          onClose={() => setView(null)}
        />
      )}

      <footer className="statusbar" aria-label="실행한 Lore 명령">
        {commands.map((c) => (
          <code key={c}>{c}</code>
        ))}
      </footer>
    </div>
  );
}
