import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Branch, Done, Graph, Lock, Overview, Revision, Settings, Status, ViewChange } from './types';
import Changes from './Changes';
import LockBoard from './LockBoard';
import SetupDialog, { browseFolder } from './SetupDialog';
import Smartlog, { branchColor, isDraft } from './Smartlog';
import ViewDialog from './ViewDialog';

type Tab = 'history' | 'changes' | 'locks';

/** Before settings.json, the last working copy was kept here. */
const LEGACY_KEY = 'tome.lastRepository';
const RECENT_MAX = 8;

function shortHash(hash: string) {
  return hash.slice(0, 8);
}

function formatTime(ms: number) {
  if (!ms) return '';
  const date = new Date(ms);
  return date.toLocaleString(undefined, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
}

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [setup, setSetup] = useState(false);
  const [path, setPath] = useState('');
  const [offline, setOffline] = useState(true);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [history, setHistory] = useState<Revision[]>([]);
  const [branchName, setBranchName] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [commands, setCommands] = useState<string[]>([]);
  const [tab, setTab] = useState<Tab>('history');
  const [logMode, setLogMode] = useState<'stack' | 'all'>('stack');
  const [locks, setLocks] = useState<Lock[]>([]);
  const [graph, setGraph] = useState<Graph | null>(null);
  const [graphLoading, setGraphLoading] = useState(false);
  const [view, setView] = useState<{ lines: string[]; result: ViewChange | null } | null>(null);

  async function saveSettings(next: Settings) {
    setSettings(next);
    await invoke('save_settings', { settings: next }).catch((e) => setError(`설정 저장 실패: ${e}`));
  }

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

  async function open(target = path, readOffline = offline, base = settings) {
    const where = target.trim();
    if (!where) return;
    setPath(where);
    setBusy(true);
    setError('');
    try {
      const result = await invoke<Overview>('open_repository', { path: where, offline: readOffline });
      setOverview(result);
      setGraph(null);
      setHistory(result.history);
      setBranchName(result.status.branch_name);
      setSelected(result.history[0]?.id ?? null);
      setCommands(result.commands);
      setTab('history');
      setLogMode('stack');
      if (base) {
        const recent = [where, ...base.recent.filter((p) => p !== where)].slice(0, RECENT_MAX);
        void saveSettings({ ...base, recent, offline: readOffline });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void (async () => {
      const loaded = await invoke<Settings>('load_settings').catch(() => null);
      const base: Settings = loaded ?? { setup_done: false, server: '', recent: [], offline: true };
      let legacy: string | null = null;
      try {
        legacy = localStorage.getItem(LEGACY_KEY);
      } catch {
        // no storage: nothing to migrate
      }
      if (legacy && !base.recent.includes(legacy)) base.recent = [legacy, ...base.recent];
      setSettings(base);
      setOffline(base.offline);
      if (!base.setup_done) setSetup(true);
      else if (base.recent[0]) void open(base.recent[0], base.offline, base);
    })();
    // Load settings once at start.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Every branch in lanes; loaded when the full graph is first shown, and after changes. */
  async function loadGraph(where = path) {
    setGraphLoading(true);
    try {
      const done = await invoke<Done<Graph>>('graph', { path: where.trim(), offline });
      setGraph(done.value);
      setCommands(done.commands);
    } catch (e) {
      setError(String(e));
    } finally {
      setGraphLoading(false);
    }
  }

  function showMode(mode: 'stack' | 'all') {
    setLogMode(mode);
    if (mode === 'all' && !graph && !graphLoading) void loadGraph();
  }

  /** A branch in the list: show the full graph with its latest revision selected. */
  function showBranch(branch: Branch) {
    if (!overview) return;
    setTab('history');
    setBranchName(branch.name);
    setSelected(branch.latest);
    showMode('all');
    requestAnimationFrame(() => document.querySelector('.grow.active')?.scrollIntoView({ block: 'center' }));
  }

  const setStatus = (status: Status) => setOverview((o) => (o ? { ...o, status } : o));

  /** After a commit, push or sync: new status, and the current branch's history again. */
  async function refreshHistory(status: Status) {
    setStatus(status);
    const revisions = await invoke<Revision[]>('branch_history', { path: path.trim(), branch: status.branch_name, offline: true }).catch(() => null);
    if (revisions) {
      setHistory(revisions);
      setBranchName(status.branch_name);
      setSelected(revisions[0]?.id ?? null);
    }
    if (graph) void loadGraph();
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

  async function browse() {
    try {
      const picked = await browseFolder('작업본 폴더', path);
      if (picked) void open(picked);
    } catch (e) {
      setError(String(e));
    }
  }

  const revision = useMemo(
    () => history.find((r) => r.id === selected) ?? graph?.rows.find((row) => row.revision.id === selected)?.revision ?? null,
    [history, graph, selected],
  );
  const status = overview?.status;
  // Lore lists a branch once per location (local and remote): show each id once.
  const branches = (overview?.branches ?? []).filter((b, i, all) => !b.archived && all.findIndex((o) => o.id === b.id) === i);
  const changedCount = status ? status.files.filter((f) => !f.directory).length : 0;

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
          <input id="repo-path" list="recent-paths" value={path} onChange={(e) => setPath(e.target.value)} placeholder="Lore 작업본 경로 (예: C:\Project\SampleProject)" spellCheck={false} />
          <datalist id="recent-paths">
            {settings?.recent.map((p) => <option key={p} value={p} />)}
          </datalist>
          <button type="button" className="ghost" onClick={() => void browse()} disabled={busy}>
            찾아보기
          </button>
          <label className="check">
            <input type="checkbox" checked={offline} onChange={(e) => setOffline(e.target.checked)} />
            오프라인
          </label>
          <button type="submit" disabled={busy}>{busy ? '작업 중…' : '열기'}</button>
        </form>
        <button className="ghost" onClick={() => void openView()} disabled={!overview || busy}>View</button>
        <button className="ghost icon" onClick={() => setSetup(true)} disabled={!settings || busy} aria-label="설정" title="설정">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
          </svg>
        </button>
      </header>

      {error && <div className="error" role="alert">{error}</div>}

      <main className="panes">
        <nav className="pane branches" aria-label="브랜치">
          <h2>브랜치</h2>
          <ul>
            {branches.map((b) => (
              <li key={b.id}>
                <button className={b.name === branchName ? 'item active' : 'item'} onClick={() => showBranch(b)}>
                  <span className="lane-dot" style={{ background: branchColor(b.id, status?.branch_id ?? '') }} />
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
                {changedCount ? `변경 파일 ${changedCount}개` : '변경 확인'}
              </button>
            </section>
          )}
        </nav>

        <section className="pane smartlog">
          <div className="tabs" role="tablist">
            {(
              [
                ['history', 'Smartlog'],
                ['changes', '변경'],
                ['locks', '잠금'],
              ] as [Tab, string][]
            ).map(([id, label]) => (
              <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? 'tab active' : 'tab'} onClick={() => showTab(id)} disabled={!overview}>
                {label}
              </button>
            ))}
          </div>
          {!overview && (
            <div className="welcome">
              <p>작업본을 열면 내 스택과 히스토리가 여기에 나옵니다.</p>
              <button className="primary" onClick={() => setSetup(true)} disabled={!settings}>
                작업본 열기 · 서버에서 받기
              </button>
            </div>
          )}
          {tab === 'history' && status && (
            <Smartlog
              status={status}
              history={history}
              graph={graph}
              branches={branches}
              selected={selected}
              mode={logMode}
              busy={busy}
              onMode={showMode}
              onSelect={setSelected}
              onCommit={() => showTab('changes')}
              onSync={() => void run<Status>('sync', {}, (s) => void refreshHistory(s))}
            />
          )}
          {tab === 'changes' && status && (
            <Changes
              status={status}
              busy={busy}
              onRefresh={() => void run<Status>('working_status', { offline: true }, setStatus)}
              onStage={(paths, stage) => void run<Status>('stage_files', { paths, stage }, setStatus)}
              onCommit={(message, push) => run<Status>('commit', { message, push }, (s) => void refreshHistory(s))}
              onPush={() => void run<Status>('push', { branch: status.branch_name }, (s) => void refreshHistory(s))}
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
        </section>

        <aside className="pane details" aria-label="리비전 상세">
          {revision ? (
            <>
              <p className={isDraft(revision, status) ? 'state draft' : 'state'}>{isDraft(revision, status) ? 'draft · 미푸시' : 'public'}</p>
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

      {setup && settings && (
        <SetupDialog
          settings={settings}
          firstRun={!settings.setup_done}
          onDone={(next, target) => {
            setSetup(false);
            setOffline(next.offline);
            void saveSettings(next);
            if (target) void open(target, next.offline, next);
          }}
          onCancel={() => {
            setSetup(false);
            // "나중에" on first run still counts as done, so the dialog does not return every start.
            if (!settings.setup_done) void saveSettings({ ...settings, setup_done: true });
          }}
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
