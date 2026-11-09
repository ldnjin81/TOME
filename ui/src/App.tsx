import { t, setLanguage, useLanguage, getLocale } from './i18n';
import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Asset, AssetPreview, Keep, PendingRestack, RestackOutcome, RestackPlan, StackInfo, JobEvent, JobOp, JobProgress, LoreNotification, AuthState, BranchState, Restored, DiffFile, FilePatch, Resolution, RevisionChanges, Branch, Done, Graph, Lock, Overview, Revision, Settings, Status, Tool, ToolContext, ToolOutput, ToolSelection, ToolSet, ViewChange } from './types';
import { ContextMenu, OutputPanel, RunDialog, ToolManager, ToolMenu, toolsFor, type ToolEntry } from './Tools';
import Changes from './Changes';
import Updater from './Updater';
import LockBoard from './LockBoard';
import SetupDialog, { browseFolder } from './SetupDialog';
import BranchExplorer from './BranchExplorer';
import type { ExplorerLayout } from './branchExplorerLogic';
import Smartlog, { branchColor, isDraft, type RestackRequest } from './Smartlog';
import { FoldDialog, RestackDialog, RestackPanel } from './Restack';
import type { FoldRequest } from './stackLogic';
import { explainError, needsLogin } from './errors';
import AuthPanel, { AuthDialog } from './AuthPanel';
import ViewDialog, { PRESETS } from './ViewDialog';
import { viewSummary } from './viewLogic';
import { joinUnder, nestedRootOf } from './nestedLogic';
import { ARCHIVED_GROUP, archivedOf, lastRevisionOf, switchAction } from './branchExplorerLogic';
import InstanceDialog from './InstanceDialog';
import Toasts, { type Toast } from './Toasts';
import FileHistory from './FileHistory';
import JobPanel from './JobPanel';
import { MergeDialog, NewBranchDialog } from './BranchDialogs';
import { ACTION_MARK, DiffDialog, countLines } from './DiffView';
import Assets, { AssetDetails, assetMarks } from './Assets';
import { orderBranches } from './branchLogic';

type Tab = 'branches' | 'history' | 'assets' | 'changes' | 'locks';

/** Before settings.json, the last working copy was kept here. */
const LEGACY_KEY = 'tome.lastRepository';
const RECENT_MAX = 8;

function shortHash(hash: string) {
  return hash.slice(0, 8);
}

function formatTime(ms: number) {
  if (!ms) return '';
  const date = new Date(ms);
  return date.toLocaleString(getLocale(), { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
}

export default function App() {
  useLanguage();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [setup, setSetup] = useState(false);
  const [updateSettings, setUpdateSettings] = useState(false);
  const [login, setLogin] = useState(false);
  const [path, setPath] = useState('');
  const [offline, setOffline] = useState(true);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [instance, setInstance] = useState(false);
  const [history, setHistory] = useState<Revision[]>([]);
  const [branchName, setBranchName] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [commands, setCommands] = useState<string[]>([]);
  const [tab, setTab] = useState<Tab>('history');
  const [logMode, setLogMode] = useState<'stack' | 'all'>('stack');
  const [locks, setLocks] = useState<Lock[]>([]);
  const [explorer, setExplorer] = useState<ExplorerLayout | null>(null);
  /** 복원 중인 보관 브랜치 id. */
  const [restoring, setRestoring] = useState<string | null>(null);
  const [graph, setGraph] = useState<Graph | null>(null);
  const [graphLoading, setGraphLoading] = useState(false);
  const [toolSet, setToolSet] = useState<ToolSet | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; title: string; context: ToolContext; selection: ToolSelection } | null>(null);
  const [pending, setPending] = useState<{ entry: ToolEntry; selection: ToolSelection } | null>(null);
  const [toolRun, setToolRun] = useState<{ name: string; running: boolean; output: ToolOutput | null; error: string } | null>(null);
  const [managing, setManaging] = useState(false);
  const [auth, setAuth] = useState<AuthState | null>(null);
  const [newBranch, setNewBranch] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [historyOf, setHistoryOf] = useState<string | null>(null);
  /** The running clone / sync / push (one at a time). */
  const [job, setJob] = useState<{ id: number; label: string; progress: JobProgress | null; cancelling: boolean } | null>(null);
  const jobDone = useRef<Map<number, (ok: boolean, error: string, cancelled: boolean) => void>>(new Map());
  /** Live notifications: 'on', or why they are off. */
  const [watch, setWatch] = useState<{ on: boolean; reason: string } | null>(null);
  const [mergeFrom, setMergeFrom] = useState<string | null>(null);
  /** "f → main" for the merge this window started; a merge found on open shows its revision. */
  const [mergeLabel, setMergeLabel] = useState('');
  const [changes, setChanges] = useState<{ id: string; data: RevisionChanges | null; error: string } | null>(null);
  const [diff, setDiff] = useState<{ title: string; files: DiffFile[]; patches: FilePatch[]; initial: string; note?: string } | null>(null);
  const [view, setView] = useState<{ lines: string[]; result: ViewChange | null } | null>(null);
  /** 툴바 View 알약에 보이는 현재 작업본의 `.lore/view` 규칙(못 읽었으면 null). */
  const [viewLines, setViewLines] = useState<string[] | null>(null);
  /** 변경 목록의 경로가 놓인 중첩 저장소의 루트들(작업본 기준 상대 경로). */
  const [nested, setNested] = useState<string[]>([]);
  const [asset, setAsset] = useState<Asset | null>(null);
  const [previews, setPreviews] = useState<Record<string, AssetPreview>>({});
  const [stackInfo, setStackInfo] = useState<StackInfo | null>(null);
  const [restackReq, setRestackReq] = useState<RestackRequest | null>(null);
  const [foldReq, setFoldReq] = useState<FoldRequest | null>(null);
  const [pendingRestack, setPendingRestack] = useState<PendingRestack | null>(null);
  const [restackChoices, setRestackChoices] = useState<Record<string, Keep>>({});

  async function saveSettings(next: Settings) {
    setLanguage(next.language ?? '');
    setSettings(next);
    await invoke('save_settings', { settings: next }).catch((e) => setError(t("설정 저장 실패: {p0}", { p0: e })));
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
      setExplorer(null);
      void loadTools(where);
      void refreshLocksQuietly(result.status.branch_name, where);
      setWatch(null);
      invoke('watch_repository', { path: where }).then(
        () => setWatch({ on: true, reason: '' }),
        (e) => setWatch({ on: false, reason: String(e) }),
      );
      invoke<AuthState>('auth_state', { path: where }).then(setAuth, () => setAuth(null));
      invoke<string[]>('read_view', { path: where }).then(setViewLines, () => setViewLines(null));
      setHistory(result.history);
      setBranchName(result.status.branch_name);
      setSelected(result.history[0]?.id ?? null);
      setCommands(result.commands);
      setTab(base?.mode === 'artist' ? 'assets' : 'history');
      setAsset(null);
      setPreviews({});
      setStackInfo(null);
      void loadStack(where);
      invoke<PendingRestack | null>('restack_pending', { path: where }).then(setPendingRestack, () => setPendingRestack(null));
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
      const base: Settings = loaded ?? { setup_done: false, server: '', recent: [], offline: true, identity: '', tools: [], trusted_tools: {}, mode: '', collapsed_branch_groups: [], language: '' };
      setLanguage(base.language ?? '');
      let legacy: string | null = null;
      try {
        legacy = localStorage.getItem(LEGACY_KEY);
      } catch {
        // no storage: nothing to migrate
      }
      if (legacy && !base.recent.includes(legacy)) base.recent = [legacy, ...base.recent];
      setSettings(base);
      setOffline(base.offline);
      void loadTools('');
      if (!base.setup_done) setSetup(true);
      else if (base.recent[0]) void open(base.recent[0], base.offline, base);
    })();
    // Load settings once at start.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** The lock board for marks on changed files; quiet: errors (offline server) are ignored. */
  async function refreshLocksQuietly(branch: string, where = path) {
    try {
      const done = await invoke<Done<Lock[]>>('lock_board', { path: where.trim(), branch });
      setLocks(done.value);
    } catch {
      // no server: no marks
    }
  }

  function addToast(text: string, tone: Toast['tone']) {
    setToasts((list) => [...list, { id: Date.now() + Math.random(), text, tone }]);
  }

  // Progress and the end of jobs.
  useEffect(() => {
    const stop = listen<JobEvent>('lore-job', (event) => {
      const e = event.payload;
      if (e.progress) {
        setJob((j) => (j && j.id === e.id ? { ...j, progress: e.progress } : j));
      }
      if (e.done) {
        setJob((j) => (j && j.id === e.id ? null : j));
        const [status, message] = e.done;
        jobDone.current.get(e.id)?.(status === 0, message, e.cancelled);
        jobDone.current.delete(e.id);
      }
    });
    return () => {
      void stop.then((unlisten) => unlisten());
    };
  }, []);

  /** Starts a long operation; `then` runs when it ends (not when cancelled). */
  async function startJob(op: JobOp, label: string, then: () => void) {
    if (job) {
      setError(t("다른 작업이 진행 중입니다. 끝나거나 취소한 뒤에 하세요."));
      return;
    }
    setError('');
    try {
      const id = await invoke<number>('start_job', { op });
      setJob({ id, label, progress: null, cancelling: false });
      jobDone.current.set(id, (ok, message, cancelled) => {
        if (cancelled) addToast(t("{p0} 취소함{p1}", { p0: label, p1: op.op === 'clone' ? t(' (받던 폴더는 지웠습니다)') : t(' (다시 실행하면 이어서 진행합니다)') }), 'info');
        else if (ok) {
          addToast(t("{p0} 완료", { p0: label }), 'push');
          then();
        } else if (/diverged/i.test(message)) {
          setError(t("{p0} 실패: 서버에 새 리비전이 있습니다. Smartlog의 내 스택에서 restack(위로 옮기기)하거나 병합해서 받은 뒤 다시 push하세요.", { p0: label }));
          void loadStack();
        } else setError(t("{p0} 실패: {p1}", { p0: label, p1: message }));
      });
    } catch (e) {
      setError(String(e));
    }
  }

  function cancelJob() {
    if (!job) return;
    setJob({ ...job, cancelling: true });
    invoke('cancel_job', { id: job.id }).catch((e) => setError(String(e)));
  }

  function syncJob() {
    const where = path.trim();
    void startJob({ op: 'sync', path: where }, t("동기화"), () => {
      invoke<Done<Status>>('working_status', { path: where, offline: false }).then((d) => void refreshHistory(d.value), (e) => setError(String(e)));
    });
  }

  function pushJob(branch: string) {
    const where = path.trim();
    void startJob({ op: 'push', path: where, branch }, `${branch} push`, () => {
      invoke<Done<Status>>('working_status', { path: where, offline: false }).then((d) => void refreshHistory(d.value), () => {});
    });
  }

  async function commitThen(message: string, push: boolean) {
    // The commit is quick and local; a push after it runs as a job with progress.
    const ok = await run<Status>('commit', { message, push: false }, (s) => void refreshHistory(s));
    if (ok && push && overview) pushJob(overview.status.branch_name);
    return ok;
  }

  // Notifications from the server for the open working copy.
  const latest = useRef({ path, branchName: '', branches: [] as Branch[], graphLoaded: false });
  latest.current = { path, branchName: overview?.status.branch_name ?? '', branches: overview?.branches ?? [], graphLoaded: !!graph };
  useEffect(() => {
    const stop = listen<LoreNotification>('lore-notification', (event) => {
      const n = event.payload;
      const { path: where, branchName: current, branches: known } = latest.current;
      const paths = Array.isArray(n.data.paths) ? (n.data.paths as string[]) : [];
      const who = typeof n.data.userId === 'string' && n.data.userId !== '<unknown>' ? ` · ${n.data.userId}` : '';
      const branchName = known.find((b) => b.id === n.data.branch)?.name ?? '';
      if (n.kind === 'resourceLocked' || n.kind === 'resourceUnlocked') {
        addToast(t("{p0}: {p1}{p2}", { p0: n.kind === 'resourceLocked' ? t('잠금@@알림') : t('잠금 해제@@알림'), p1: paths.join(', '), p2: who }), n.kind === 'resourceLocked' ? 'lock' : 'unlock');
        if (current) void refreshLocksQuietly(current, where);
      } else if (n.kind === 'branchPushed') {
        addToast(t("{p0}에 새 리비전 r{p1} push{p2}", { p0: branchName || t('브랜치'), p1: String(n.data.revisionNumber ?? ''), p2: who }), 'push');
        invoke<Done<Status>>('working_status', { path: where.trim(), offline: false }).then((d) => setStatus(d.value), () => {});
        void loadStack(where);
        if (latest.current.graphLoaded) void loadGraph(where);
      } else {
        addToast(t("{p0}{p1}", { p0: n.kind === 'branchCreated' ? t('브랜치 생성') : n.kind === 'branchDeleted' ? t('브랜치 삭제') : n.kind, p1: branchName ? `: ${branchName}` : '' }), 'info');
      }
    });
    return () => {
      void stop.then((unlisten) => unlisten());
    };
    // Subscribed once; the handler reads the latest state through the ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function loadTools(where = path) {
    try {
      setToolSet(await invoke<ToolSet>('list_tools', { path: where.trim() }));
    } catch (e) {
      setError(String(e));
    }
  }

  /** Settings as the backend has them (trust is written there, never by the page). */
  async function reloadSettings() {
    const loaded = await invoke<Settings>('load_settings').catch(() => null);
    if (loaded) setSettings(loaded);
    return loaded;
  }

  function selectionFor(partial: Partial<ToolSelection>): ToolSelection {
    return { files: [], revision: '', revision_number: 0, branch: overview?.status.branch_name ?? '', answer: '', ...partial };
  }

  function openMenu(e: React.MouseEvent, context: ToolContext, title: string, selection: ToolSelection) {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, title, context, selection });
  }

  function startTool(entry: ToolEntry, selection: ToolSelection) {
    if (entry.tool.prompt || entry.tool.confirm) setPending({ entry, selection });
    else void runTool(entry, selection);
  }

  async function runTool(entry: ToolEntry, selection: ToolSelection) {
    const tool = entry.tool;
    setToolRun({ name: tool.name, running: true, output: null, error: '' });
    try {
      const output = await invoke<ToolOutput>('run_tool', { path: path.trim(), project: entry.project, id: tool.id, selection });
      setToolRun({ name: tool.name, running: false, output, error: '' });
      setCommands([output.command]);
      if (tool.refresh) void run<Status>('working_status', { offline: true }, setStatus);
    } catch (e) {
      setToolRun({ name: tool.name, running: false, output: null, error: String(e) });
    }
  }

  async function savePersonalTools(tools: Tool[]) {
    const current = (await reloadSettings()) ?? settings;
    if (!current) return;
    await saveSettings({ ...current, tools });
    await loadTools();
  }

  async function saveProjectTools(tools: Tool[]) {
    await invoke('save_project_tools', { path: path.trim(), tools });
    await reloadSettings();
    await loadTools();
  }

  async function trustProjectTools() {
    try {
      await invoke('trust_project_tools', { path: path.trim() });
      await reloadSettings();
      await loadTools();
    } catch (e) {
      setError(String(e));
    }
  }

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
  /** My stack as the server sees it; quiet: offline, the Smartlog falls back to revision numbers. */
  async function loadStack(where = path) {
    try {
      setStackInfo(await invoke<StackInfo>('stack_info', { path: where.trim() }));
    } catch {
      setStackInfo(null);
    }
  }

  async function refreshHistory(status: Status) {
    setStatus(status);
    void loadStack();
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
    if (next === 'assets') {
      // Fresh marks for the cards, quietly (no busy state, no error for an offline server).
      invoke<Done<Status>>('working_status', { path: path.trim(), offline: true }).then((d) => setStatus(d.value), () => {});
      void refreshLocksQuietly(overview.status.branch_name);
    }
  }

  function switchMode(mode: 'programmer' | 'artist') {
    if (settings) void saveSettings({ ...settings, mode });
    showTab(mode === 'artist' ? 'assets' : 'history');
  }

  async function openView() {
    try {
      const lines = await invoke<string[]>('read_view', { path: path.trim() });
      setViewLines(lines);
      setView({ lines, result: null });
    } catch (e) {
      setError(String(e));
    }
  }

  async function browse() {
    try {
      const picked = await browseFolder(t("작업본 폴더"), path);
      if (picked) void open(picked);
    } catch (e) {
      setError(String(e));
    }
  }

  const revision = useMemo(
    () => history.find((r) => r.id === selected) ?? graph?.rows.find((row) => row.revision.id === selected)?.revision ?? explorer?.nodes.find((n) => n.revision.id === selected)?.revision ?? null,
    [history, graph, explorer, selected],
  );
  const status = overview?.status;
  // Lore lists a branch once per location (local and remote): show each id once.
  const branches = (overview?.branches ?? []).filter((b, i, all) => !b.archived && all.findIndex((o) => o.id === b.id) === i);
  const branchList = useMemo(() => orderBranches(branches, status?.branch_name ?? ''), [branches, status?.branch_name]);
  // 보관 브랜치는 브랜치 탐색기가 "보관 브랜치 표시"로 읽어 온 것만 보인다(마지막 리비전도 거기서 찾는다).
  const archivedBranches = useMemo(() => archivedOf(explorer?.lanes ?? []), [explorer]);
  const collapsed = new Set(settings?.collapsed_branch_groups ?? []);
  function toggleGroup(group: string) {
    if (!settings) return;
    const next = new Set(collapsed);
    if (next.has(group)) next.delete(group);
    else next.add(group);
    void saveSettings({ ...settings, collapsed_branch_groups: [...next].sort() });
  }
  const changedCount = status ? status.files.filter((f) => !f.directory).length : 0;
  const artist = settings?.mode === 'artist';
  const marks = useMemo(() => assetMarks(status?.files ?? [], locks), [status, locks]);
  const viewPill = viewLines && viewSummary(viewLines, PRESETS);

  // 변경 목록의 경로 중 중첩 저장소 안에 있는 것(탐지는 백엔드에서 .lore 폴더로 한다).
  const statusPaths = useMemo(() => (status?.files ?? []).map((f) => f.path), [status]);
  useEffect(() => {
    const where = path.trim();
    if (!where || !statusPaths.length) {
      setNested([]);
      return;
    }
    let cancelled = false;
    invoke<string[]>('nested_repositories', { path: where, paths: statusPaths }).then((roots) => { if (!cancelled) setNested(roots); }, () => { if (!cancelled) setNested([]); });
    return () => { cancelled = true; };
    // 경로 입력란을 고칠 때마다가 아니라 작업본 상태가 바뀔 때 다시 확인한다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [statusPaths]);

  // The selected revision's changed files and diffs, loaded when it is selected.
  useEffect(() => {
    if (!revision || !overview) return;
    if (changes?.id === revision.id) return;
    setChanges({ id: revision.id, data: null, error: '' });
    invoke<RevisionChanges>('revision_changes', { path: path.trim(), revision: revision.id, parent: revision.parents[0] ?? '', offline })
      .then((data) => setChanges((c) => (c?.id === revision.id ? { id: revision.id, data, error: '' } : c)))
      .catch((e) => setChanges((c) => (c?.id === revision.id ? { id: revision.id, data: null, error: String(e) } : c)));
    // Reload only when the selection changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision?.id, overview]);

  /** After a branch is created or switched: everything shown depends on the branch. */
  async function afterBranchChange() {
    await open(path, offline);
  }

  async function createBranch(name: string, switchTo: boolean) {
    setNewBranch(false);
    await run<BranchState>('create_branch', { name, switch: switchTo }, () => void afterBranchChange());
  }

  async function switchTo(name: string) {
    await run<BranchState>('switch_branch', { name }, () => void afterBranchChange());
  }

  /** 브랜치 탐색기에서 더블클릭한 브랜치로 전환한다(브랜치 목록의 "전환"과 같은 경로). 보관 브랜치는 복원한 뒤 전환한다. */
  function switchFromExplorer(branch: Branch) {
    const action = switchAction(branch, status?.branch_name ?? '', !!status?.merging, busy);
    if (action === 'restore') void restoreBranch(branch, true);
    else if (action === 'merging') addToast(t('병합 중에는 브랜치를 전환할 수 없습니다'), 'info');
    else if (action === 'switch') void switchTo(branch.name);
  }

  /** 보관 브랜치를 원래 id·이름·기록으로 되살린다(서버에도 push). `switchAfter`면 이어서 그 브랜치로 전환한다. */
  async function restoreBranch(branch: Branch, switchAfter: boolean) {
    if (busy) return;
    const revision = lastRevisionOf(branch.id, explorer?.nodes ?? []);
    if (!revision) {
      addToast(t('보관 브랜치의 마지막 리비전을 찾지 못했습니다. 브랜치 탐색기에서 "전체"를 눌러 기록을 더 읽은 뒤 다시 하세요.'), 'info');
      return;
    }
    setRestoring(branch.id);
    addToast(t('{name} 브랜치를 복원하는 중…', { name: branch.name }), 'info');
    const ok = await run<Restored>('restore_branch', { id: branch.id, name: branch.name, revision, switch: switchAfter }, (done) => {
      if (done.switch_error) addToast(t('{name} 브랜치를 복원했지만 전환하지 못했습니다: {reason}', { name: branch.name, reason: explainError(done.switch_error) }), 'info');
      else addToast(switchAfter ? t('{name} 브랜치를 복원하고 전환했습니다', { name: branch.name }) : t('{name} 브랜치를 복원했습니다', { name: branch.name }), 'push');
      void afterBranchChange();
    });
    setRestoring(null);
    if (!ok) addToast(t('{name} 브랜치를 복원하지 못했습니다', { name: branch.name }), 'info');
  }

  /** 작업본 안의 중첩 저장소를 TOME에서 저장소로 연다(경로 입력 후 "열기"와 같은 경로). */
  function openNested(root: string) {
    void open(joinUnder(path.trim(), root));
  }

  async function mergeBranch(from: string, message: string) {
    setMergeFrom(null);
    const into = overview?.status.branch_name ?? '';
    await run<Status>('merge_branch', { from, message }, (status) => {
      if (status.merging) {
        // Conflicts: settle them in the changes tab, then commit.
        setMergeLabel(`${from} → ${into}`);
        setStatus(status);
        setTab('changes');
      } else {
        setMergeLabel('');
        void refreshHistory(status);
      }
    });
  }

  function resolve(paths: string[], how: Resolution | null) {
    void run<Status>('resolve_conflicts', { paths, how }, setStatus);
  }

  function abortMerge() {
    void run<Status>('abort_merge', {}, (status) => {
      setMergeLabel('');
      void refreshHistory(status);
    });
  }

  /** After a restack step: done refreshes everything; a conflict whose files were all settled
   * up front is settled and continued, otherwise it waits in the panel. */
  async function restackStepped(outcome: RestackOutcome, choices: Record<string, Keep>) {
    setStatus(outcome.status);
    const step = outcome.step;
    if (step.state === 'done') {
      setPendingRestack(null);
      setRestackChoices({});
      const skipped = step.skipped.length ? t(" · {p0}개는 새 베이스에 이미 있어 뺐습니다", { p0: step.skipped.length }) : '';
      addToast(t("Restack 완료{p0} · 이제 push할 수 있습니다", { p0: skipped }), 'push');
      void refreshHistory(outcome.status);
      return;
    }
    const pending = await invoke<PendingRestack | null>('restack_pending', { path: path.trim() }).catch(() => null);
    setPendingRestack(pending);
    if (step.files.length > 0 && step.files.every((f) => choices[f] && choices[f] !== 'edited')) {
      for (const keep of ['mine', 'base'] as Keep[]) {
        const files = step.files.filter((f) => choices[f] === keep);
        if (files.length && !(await run<Status>('restack_resolve', { paths: files, keep }, setStatus))) return;
      }
      await restackCall('restack_continue', {}, choices);
    }
  }

  /** A restack call that failed: the backend put the branch back when it could, so show
   * that plainly and read the working copy again. */
  async function restackCall(command: string, args: Record<string, unknown>, choices: Record<string, Keep>) {
    const ok = await run<RestackOutcome>(command, args, (o) => void restackStepped(o, choices));
    if (ok) return;
    setError((e) => (/put back as they were/.test(e) ? t("restack 중 오류가 나서 시작 전 상태로 되돌렸습니다. ({p0})", { p0: e.split(';')[0] }) : e));
    setPendingRestack(await invoke<PendingRestack | null>('restack_pending', { path: path.trim() }).catch(() => null));
    invoke<Done<Status>>('working_status', { path: path.trim(), offline: true }).then((d) => void refreshHistory(d.value), () => {});
  }

  function startRestack(plan: RestackPlan, choices: Record<string, Keep>) {
    setRestackReq(null);
    setRestackChoices(choices);
    void restackCall('restack_start', { plan }, choices);
  }

  async function showWorkingDiff(file: string) {
    try {
      const patches = await invoke<FilePatch[]>('working_patches', { path: path.trim(), files: [file] });
      setDiff({ title: t("미커밋 변경"), files: [{ path: file, action: 'modify', directory: false }], patches, initial: file });
    } catch (e) {
      setError(String(e));
    }
  }

  /** 파일 하나를 우클릭했을 때 사용자 도구 위에 붙는 동작. */
  function fileActions(file: string) {
    const actions = [{ label: t("파일 기록"), run: () => setHistoryOf(file) }];
    const root = nestedRootOf(file, nested);
    if (root) actions.unshift({ label: t('저장소로 열기'), run: () => openNested(root) });
    return actions;
  }

  function renderBranch(b: Branch, label: string) {
    const current = b.name === status?.branch_name;
    return (
      <li key={b.id} className="branch-li">
        <button className={b.name === branchName ? 'item active' : 'item'} onClick={() => showBranch(b)} title={b.name}>
          <span className="lane-dot" style={{ background: branchColor(b.id, status?.branch_id ?? '') }} />
          <span className="name">{label}</span>
          {current && <span className="badge">{t("현재")}</span>}
        </button>
        {!current && (
          <span className="branch-actions">
            <button className="ghost small-btn" onClick={() => void switchTo(b.name)} disabled={busy || !!status?.merging} title={t("{p0}로 전환", { p0: b.name })}>
              {t("전환")}
            </button>
            <button className="ghost small-btn" onClick={() => setMergeFrom(b.name)} disabled={busy || !!status?.merging} title={t("{p0}를 현재 브랜치에 병합", { p0: b.name })}>
              {t("병합")}
            </button>
          </span>
        )}
      </li>
    );
  }

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
          <label className="visually-hidden" htmlFor="repo-path">{t("작업본 경로")}</label>
          <input id="repo-path" list="recent-paths" value={path} onChange={(e) => setPath(e.target.value)} placeholder={t("Lore 작업본 경로 (예: C:\\Project\\MyGame)")} spellCheck={false} />
          <datalist id="recent-paths">
            {settings?.recent.map((p) => <option key={p} value={p} />)}
          </datalist>
          <button type="button" className="ghost" onClick={() => void browse()} disabled={busy}>
            {t("찾아보기")}
          </button>
          <label className="check">
            <input type="checkbox" checked={offline} onChange={(e) => setOffline(e.target.checked)} />
            {t("오프라인")}
          </label>
          <button type="submit" disabled={busy}>{busy ? t("작업 중…") : t("열기")}</button>
        </form>
        <div className="mode-switch" role="radiogroup" aria-label={t("화면 모드")}>
          {(
            [
              ['programmer', t("프로그래머")],
              ['artist', t("아티스트")],
            ] as const
          ).map(([id, label]) => (
            <button key={id} role="radio" aria-checked={(id === 'artist') === artist} className={(id === 'artist') === artist ? 'active' : ''} onClick={() => switchMode(id)} disabled={!settings}>
              {label}
            </button>
          ))}
        </div>
        <ToolMenu set={toolSet} disabled={!settings} onRun={(entry) => startTool(entry, selectionFor({}))} onManage={() => setManaging(true)} />
        <button className={viewPill && viewPill.kind !== 'all' ? 'view-pill narrowed' : 'view-pill'} onClick={() => void openView()} disabled={!overview || busy} title={t('View 규칙 편집 (.lore/view)')}>
          {!viewPill ? 'View' : viewPill.kind === 'all' ? t('View: 전체') : viewPill.kind === 'preset' ? t('View: {name}', { name: t(viewPill.name) }) : t('View: 규칙 {count}개', { count: viewPill.count })}
        </button>
        <button className="ghost" onClick={() => setInstance(true)} disabled={!overview || busy || !!job} title={t('작업본 이동·삭제')}>{t('작업본')}</button>
        <button className="ghost icon" onClick={() => setSetup(true)} disabled={!settings || busy} aria-label={t("설정")} title={t("설정")}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
          </svg>
        </button>
      </header>

      {error && <div className="error" role="alert">{explainError(error)}{' '}
        {needsLogin(error) && <button className="link" onClick={() => setLogin(true)}>{t('로그인')}</button>}
      </div>}
      {status?.merging && (
        <div className="merge-bar" role="status">
          {t("병합 중")} <span className="mono">{mergeLabel || `r${status.merging.slice(0, 8)}`}</span> {t("· 미해결 충돌 {p0}개 ·", { p0: status.files.filter((f) => f.unresolved).length })}{' '}
          <button className="link" onClick={() => showTab('changes')}>
            {t("변경 탭에서 정하기")}
          </button>
        </div>
      )}
      {settings?.setup_done && !settings.identity && auth?.server_requires_login !== true && (
        <div className="warn-bar" role="status">
          {t("신원(내 이름)이 비어 있어 TOME으로 한 커밋의 작성자가 비게 됩니다.")}{' '}
          <button className="link" onClick={() => setSetup(true)}>
            {t("설정하기")}
          </button>
        </div>
      )}

      <main className="panes">
        <nav className="pane branches" aria-label={t("브랜치")}>
          <h2 className="branches-head">
            {t("브랜치")}
            <button className="link" onClick={() => setNewBranch(true)} disabled={!overview || busy || !!status?.merging}>
              {t("+ 새 브랜치")}
            </button>
          </h2>
          <ul>
            {branchList.top.map((b) => renderBranch(b, b.name))}
          </ul>
          {branchList.groups.map(([group, list]) => {
            const open = !collapsed.has(group) || list.some((b) => b.name === status?.branch_name);
            return (
              <div key={group} className="branch-group">
                <button className="group-head" onClick={() => toggleGroup(group)} aria-expanded={open} title={open ? t("{p0}/ 접기", { p0: group }) : t("{p0}/ 펼치기", { p0: group })}>
                  <span className="tree-caret" aria-hidden="true">{open ? '▾' : '▸'}</span>
                  {group}/ <span className="muted">{list.length}</span>
                </button>
                {open && <ul>{list.map((b) => renderBranch(b, b.name.slice(group.length + 1)))}</ul>}
              </div>
            );
          })}
          {archivedBranches.length > 0 && (
            <div className="branch-group">
              <button className="group-head" onClick={() => toggleGroup(ARCHIVED_GROUP)} aria-expanded={!collapsed.has(ARCHIVED_GROUP)} title={t('브랜치 탐색기에서 "보관 브랜치 표시"로 읽은 보관 브랜치')}>
                <span className="tree-caret" aria-hidden="true">{collapsed.has(ARCHIVED_GROUP) ? '▸' : '▾'}</span>
                {t('보관 브랜치')} <span className="muted">{archivedBranches.length}</span>
              </button>
              {!collapsed.has(ARCHIVED_GROUP) && (
                <ul>
                  {archivedBranches.map((b) => (
                    <li key={b.id} className="branch-li">
                      <span className="item muted" title={b.name}>
                        <span className="name">{b.name}</span>
                      </span>
                      <span className="branch-actions">
                        <button className="ghost small-btn" onClick={() => void restoreBranch(b, false)} disabled={busy || offline || !!status?.merging} title={t('{name} 브랜치를 되살립니다(서버에도 반영, 전환하지 않음)', { name: b.name })}>
                          {restoring === b.id ? t('복원 중…') : t('복원')}
                        </button>
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
          {status && (
            <section className="working">
              <h2>{t("작업본")}</h2>
              <p>
                {status.branch_name} · r{status.revision_number}
                {status.local_ahead && <span className="badge ahead">{t("push 대기")}</span>}
                {status.remote_ahead && <span className="badge behind">{t("새 버전")}</span>}
              </p>
              <button className="link" onClick={() => showTab('changes')}>
                {changedCount ? t("변경 파일 {p0}개", { p0: changedCount }) : t("변경 확인")}
              </button>
            </section>
          )}
          {settings && (
            <section className="working identity">
              <h2>{t("신원")}</h2>
              <p>
                {t("나:")} <strong>{auth?.user?.id || settings.identity || t("(없음)")}</strong>
              </p>
              <AuthPanel path={path.trim()} remoteUrl={path.trim() ? '' : settings.server}
                onState={setAuth} onChanged={() => { setError(''); if (path.trim()) void open(); }} />
              <button className="link" onClick={() => setSetup(true)}>
                {t("바꾸기")}
              </button>
              {watch && (
                <p className="muted small" title={watch.reason}>
                  <span className={watch.on ? 'live-dot on' : 'live-dot'} /> {watch.on ? t("실시간 알림 켜짐") : t("실시간 알림 꺼짐(서버 연결 안 됨)")}
                </p>
              )}
            </section>
          )}
        </nav>

        <section className="pane smartlog">
          <div className="tabs" role="tablist">
            {(
              [
                artist ? ['assets', t("에셋")] : ['history', 'Smartlog'],
                ...(!artist ? [['branches', t("브랜치")]] : []),
                ['changes', t("변경")],
                ['locks', t("잠금")],
              ] as [Tab, string][]
            ).map(([id, label]) => (
              <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? 'tab active' : 'tab'} onClick={() => showTab(id)} disabled={!overview}>
                {label}
              </button>
            ))}
          </div>
          {!overview && (
            <div className="welcome">
              <p>{t("작업본을 열면 내 스택과 히스토리가 여기에 나옵니다.")}</p>
              <button className="primary" onClick={() => setSetup(true)} disabled={!settings}>
                {t("작업본 열기 · 서버에서 받기")}
              </button>
            </div>
          )}
          {pendingRestack && status && (
            <RestackPanel
              pending={pendingRestack}
              status={status}
              busy={busy}
              onResolve={(paths, keep) => void run<Status>('restack_resolve', { paths, keep }, setStatus)}
              onContinue={() => void restackCall('restack_continue', {}, restackChoices)}
              onAbort={() =>
                void run<Status>('restack_abort', {}, (s) => {
                  setPendingRestack(null);
                  setRestackChoices({});
                  addToast(t("Restack을 중단하고 원래대로 되돌렸습니다"), 'info');
                  void refreshHistory(s);
                })
              }
            />
          )}
          {tab === 'history' && status && (
            <Smartlog
              status={status}
              history={history}
              stack={stackInfo}
              graph={graph}
              branches={branches}
              selected={selected}
              mode={logMode}
              busy={busy}
              onMode={showMode}
              onSelect={setSelected}
              onCommit={() => showTab('changes')}
              onSync={syncJob}
              me={auth?.user?.id ?? settings?.identity ?? ''}
              onContext={(e, r) => openMenu(e, 'revision', `r${r.number} ${r.message.split('\n')[0]}`, selectionFor({ revision: r.id, revision_number: r.number }))}
              onRestack={setRestackReq}
              restacking={!!pendingRestack}
              onFold={setFoldReq}
            />
          )}
          {tab === 'branches' && status && !artist && <BranchExplorer path={path.trim()} offline={offline} status={status} stack={stackInfo} selected={selected} busy={busy} onSelect={setSelected} onData={setExplorer} onNewBranch={() => setNewBranch(true)} onSwitch={switchFromExplorer} restoring={restoring} onRestore={(b) => void restoreBranch(b, false)} />}
          {tab === 'assets' && status && (
            <Assets
              path={path.trim()}
              marks={marks}
              me={auth?.user?.id ?? settings?.identity ?? ''}
              selected={asset}
              previews={previews}
              onPreviews={(list) => setPreviews((p) => ({ ...p, ...Object.fromEntries(list.map((x) => [x.path, x])) }))}
              onSelect={setAsset}
              onContext={(e, files) => openMenu(e, 'file', files.join(', '), selectionFor({ files }))}
              onError={setError}
            />
          )}
          {tab === 'changes' && status && (
            <Changes
              status={status}
              busy={busy}
              onRefresh={() => void run<Status>('working_status', { offline: true }, setStatus)}
              onStage={(paths, stage) => void run<Status>('stage_files', { paths, stage }, setStatus)}
              onCommit={commitThen}
              onPush={() => pushJob(status.branch_name)}
              onContext={(e, files) => openMenu(e, 'file', files.join(', '), selectionFor({ files }))}
              onDiff={(file) => void showWorkingDiff(file)}
              mergeLabel={mergeLabel || `r${status.merging.slice(0, 8)}`}
              locked={new Set(locks.map((l) => l.path))}
              onHistory={setHistoryOf}
              nested={nested}
              onOpenNested={openNested}
              blocked={pendingRestack ? t("restack이 충돌에서 멈춰 있습니다. 위 패널에서 정하고 “계속”을 누르세요.") : ''}
              onResolve={resolve}
              onAbortMerge={abortMerge}
            />
          )}
          {tab === 'locks' && status && (
            <LockBoard
              branch={status.branch_name}
              me={auth?.user?.id ?? settings?.identity ?? ''}
              locks={locks}
              busy={busy}
              onRefresh={() => void run<Lock[]>('lock_board', { branch: status.branch_name }, setLocks)}
              onLock={(paths, lock) => void run<Lock[]>('lock_files', { branch: status.branch_name, paths, lock }, setLocks)}
              onContext={(e, files) => openMenu(e, 'file', files.join(', '), selectionFor({ files }))}
            />
          )}
        </section>

        <aside className="pane details" aria-label={tab === 'assets' ? t("에셋 상세") : t("리비전 상세")}>
          {tab === 'assets' ? (
            asset && status ? (
              <AssetDetails
                asset={asset}
                preview={previews[asset.path]}
                marks={marks}
                me={auth?.user?.id ?? settings?.identity ?? ''}
                busy={busy}
                online={watch?.on !== false}
                onLock={(lock) => void run<Lock[]>('lock_files', { branch: status.branch_name, paths: [asset.path], lock }, setLocks)}
                onHistory={() => setHistoryOf(asset.path)}
                onChanges={() => showTab('changes')}
              />
            ) : (
              <p className="muted">{t("에셋을 고르세요")}</p>
            )
          ) : revision ? (
            <>
              <p className={isDraft(revision, status, stackInfo) ? 'state draft' : 'state'}>{isDraft(revision, status, stackInfo) ? t("draft · 미푸시") : 'public'}</p>
              <h2>r{revision.number}</h2>
              <p className="message-full">{revision.message}</p>
              <dl>
                <dt>{t("작성")}</dt>
                <dd>{revision.author}</dd>
                <dt>{t("시각")}</dt>
                <dd>{formatTime(revision.timestamp)}</dd>
                <dt>{t("리비전")}</dt>
                <dd className="mono">{shortHash(revision.id)}</dd>
                <dt>{t("부모")}</dt>
                <dd className="mono">{revision.parents.map(shortHash).join(' · ') || t("없음")}</dd>
              </dl>
              <section className="rev-files">
                <h3>
                  {t("변경 파일")} <span className="muted">{changes?.data ? changes.data.files.length : ''}</span>
                </h3>
                {changes?.error && <p className="error-line">{explainError(changes.error)}</p>}
                {!changes?.data && !changes?.error && <p className="muted">{t("불러오는 중…")}</p>}
                {changes?.data && (
                  <ul>
                    {changes.data.files.map((f) => {
                      const p = changes.data!.patches.find((x) => x.path === f.path);
                      const n = p && !p.binary ? countLines(p.patch) : null;
                      return (
                        <li key={f.path}>
                          <button
                            className="rf"
                            title={f.path}
                            onContextMenu={(e) => openMenu(e, 'file', f.path, selectionFor({ files: [f.path] }))}
                            onClick={() =>
                              setDiff({
                                title: `r${revision.number} ${revision.message.split('\n')[0]}`,
                                files: changes.data!.files,
                                patches: changes.data!.patches,
                                initial: f.path,
                                note: changes.data!.first_revision ? t("첫 리비전은 비교할 이전 리비전이 없어 내용 diff를 보여 주지 않습니다.") : undefined,
                              })
                            }
                          >
                            <span className={`act ${f.action}`}>{ACTION_MARK[f.action] ?? '?'}</span>
                            <span className="df-path">{f.path}</span>
                            {n && (
                              <span className="df-n">
                                <span className="plus">+{n.add}</span> <span className="minus">−{n.del}</span>
                              </span>
                            )}
                            {p?.binary && <span className="df-n muted">bin</span>}
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </section>
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
            <p className="muted">{t("리비전을 고르세요")}</p>
          )}
        </aside>
      </main>

      {instance && overview && <InstanceDialog
        path={path.trim()}
        changes={overview.status.files.length}
        drafts={overview.status.local_ahead}
        onMoved={(target) => {
          const from = path.trim();
          setInstance(false);
          const next = settings && { ...settings, recent: [target, ...settings.recent.filter((p) => p !== from && p !== target)].slice(0, RECENT_MAX) };
          if (next) void saveSettings(next);
          addToast(t('작업본을 옮겼습니다: {target}', { target }), 'info');
          setOverview(null);
          void open(target, offline, next);
        }}
        onDeleted={() => {
          const from = path.trim();
          setInstance(false);
          if (settings) void saveSettings({ ...settings, recent: settings.recent.filter((p) => p !== from) });
          addToast(t('작업본을 삭제했습니다: {path}', { path: from }), 'info');
          setOverview(null);
          setPath('');
        }}
        onClose={() => setInstance(false)}
      />}
      {view && (
        <ViewDialog
          initial={view.lines}
          busy={busy}
          result={view.result}
          onApply={(lines) =>
            void run<ViewChange>('apply_view', { lines }, (result) => {
              setView({ lines, result });
              setViewLines(lines);
              // Refresh quietly, so the status bar keeps the view commands.
              invoke<Done<Status>>('working_status', { path: path.trim(), offline: true }).then((d) => setStatus(d.value), () => {});
            })
          }
          onClose={() => setView(null)}
        />
      )}

      {login && <AuthDialog path={path.trim()} remoteUrl={path.trim() ? '' : settings?.server ?? ''}
        onClose={() => setLogin(false)} onChanged={() => { setError(''); if (path.trim()) void open(); }} />}
      {setup && settings && (
        <SetupDialog
          settings={settings}
          firstRun={!settings.setup_done}
          onUpdates={() => { setSetup(false); setUpdateSettings(true); }}
          onDone={(next, target) => {
            setSetup(false);
            setOffline(next.offline);
            void saveSettings(next);
            if (target) void open(target, next.offline, next);
          }}
          onClone={(next, target, url, view, offlineReady) => {
            setSetup(false);
            setOffline(next.offline);
            void saveSettings(next);
            void startJob({ op: 'clone', path: target, url, view, offline_ready: offlineReady }, t("받기: {p0}", { p0: url.split('/').pop() }), () => void open(target, next.offline, next));
          }}
          onCancel={() => {
            setSetup(false);
            // "나중에" on first run still counts as done, so the dialog does not return every start.
            if (!settings.setup_done) void saveSettings({ ...settings, setup_done: true });
          }}
        />
      )}

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          title={menu.title}
          entries={toolsFor(toolSet, menu.context)}
          onRun={(entry) => startTool(entry, menu.selection)}
          actions={menu.context === 'file' && menu.selection.files.length === 1 ? fileActions(menu.selection.files[0]) : []}
          onClose={() => setMenu(null)}
        />
      )}

      {pending && (
        <RunDialog
          path={path.trim()}
          entry={pending.entry}
          selection={pending.selection}
          onRun={(answer) => {
            const { entry, selection } = pending;
            setPending(null);
            void runTool(entry, { ...selection, answer });
          }}
          onCancel={() => setPending(null)}
        />
      )}

      {managing && toolSet && (
        <ToolManager
          set={toolSet}
          path={overview ? path.trim() : ''}
          onSavePersonal={savePersonalTools}
          onSaveProject={saveProjectTools}
          onTrust={trustProjectTools}
          onClose={() => setManaging(false)}
        />
      )}

      {newBranch && status && (
        <NewBranchDialog from={status.branch_name} taken={branches.map((b) => b.name)} busy={busy} onCreate={(name, sw) => void createBranch(name, sw)} onClose={() => setNewBranch(false)} />
      )}

      {mergeFrom && status && (
        <MergeDialog path={path.trim()} from={mergeFrom} into={status.branch_name} busy={busy} onMerge={(message) => void mergeBranch(mergeFrom, message)} onClose={() => setMergeFrom(null)} />
      )}

      {historyOf && (
        <FileHistory path={path.trim()} file={historyOf} offline={offline} me={auth?.user?.id ?? settings?.identity ?? ''} onClose={() => setHistoryOf(null)} onCommands={setCommands} />
      )}

      {diff && <DiffDialog {...diff} onClose={() => setDiff(null)} />}

      {foldReq && status && (
        <FoldDialog
          request={foldReq}
          busy={busy}
          onFold={(message) => {
            const { base, group, rest } = foldReq;
            setFoldReq(null);
            setRestackChoices({});
            void restackCall('fold_drafts', { fold: { base, group, message, rest, original_head: status.revision } }, {});
          }}
          onClose={() => setFoldReq(null)}
        />
      )}

      {restackReq && stackInfo && status && (
        <RestackDialog
          path={path.trim()}
          request={restackReq}
          oldBase={stackInfo.fork?.id ?? ''}
          oldOrder={[...stackInfo.drafts].reverse().map((d) => d.id)}
          originalHead={status.revision}
          busy={busy}
          onRun={startRestack}
          onClose={() => setRestackReq(null)}
        />
      )}

      {settings && <Updater settings={settings} saveSettings={saveSettings} runningJob={!!job} settingsOpen={updateSettings} onCloseSettings={() => setUpdateSettings(false)} />}
      <Toasts toasts={toasts} onDone={(id) => setToasts((list) => list.filter((t) => t.id !== id))} />

      {job && <JobPanel label={job.label} progress={job.progress} cancelling={job.cancelling} onCancel={cancelJob} />}

      {toolRun && <OutputPanel {...toolRun} onClose={() => setToolRun(null)} />}

      <footer className="statusbar" aria-label={t("실행한 Lore 명령")}>
        {commands.map((c) => (
          <code key={c}>{c}</code>
        ))}
      </footer>
    </div>
  );
}
