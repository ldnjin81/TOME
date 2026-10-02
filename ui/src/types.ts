// Mirrors tome-core's models (crates/tome-core/src/model.rs).

export interface ChangedFile {
  path: string;
  /** A directory entry (status lists directories too). */
  directory: boolean;
  action: string;
  staged: boolean;
  /** Part of a merge conflict (true until the merge is committed). */
  conflict: boolean;
  /** The conflict is not settled yet. */
  unresolved: boolean;
  /** How it was settled: mine, theirs, auto, edited; empty otherwise. */
  resolution: string;
}

export interface Status {
  branch_id: string;
  branch_name: string;
  revision: string;
  revision_number: number;
  remote_number: number;
  local_ahead: boolean;
  remote_ahead: boolean;
  /** The revision being merged in while a merge is in progress; empty otherwise. */
  merging: string;
  files: ChangedFile[];
}

export interface Branch {
  id: string;
  name: string;
  latest: string;
  current: boolean;
  archived: boolean;
  creator: string;
  created: number;
}

export interface Revision {
  id: string;
  number: number;
  parents: string[];
  message: string;
  author: string;
  timestamp: number;
  branch_id: string;
  metadata: [string, unknown][];
}

export interface Overview {
  status: Status;
  branches: Branch[];
  history: Revision[];
  commands: string[];
}

export interface Lock {
  path: string;
  owner: string;
  locked_at: number;
}

export interface ViewChange {
  removed: string[];
  kept: string[];
  restored: string[];
}

/** A command's result with the Lore command lines that do the same. */
export interface Done<T> {
  value: T;
  commands: string[];
}

export interface RemoteRepository {
  id: string;
  name: string;
}

/** Kept between runs in the app config folder (settings.json). */
export interface Settings {
  setup_done: boolean;
  server: string;
  recent: string[];
  offline: boolean;
  /** Who I am to Lore: recorded as the author on a server without authentication. */
  identity: string;
  /** My own custom tools. */
  tools: Tool[];
  /** Working copy root -> fingerprint of the trusted .tome/tools.json (written by the backend). */
  trusted_tools: Record<string, string>;
}

export interface AuthState {
  server_requires_login: boolean;
  logged_in: string[];
  detail: string;
}

/** Custom tools (crates/tome-core/src/tools.rs). */
export type ToolContext = 'repository' | 'file' | 'revision';
export type RunMode = 'capture' | 'terminal' | 'detached';

export interface Tool {
  id: string;
  name: string;
  program: string;
  args: string;
  cwd: string;
  contexts: ToolContext[];
  run: RunMode;
  prompt: string;
  confirm: boolean;
  refresh: boolean;
}

export interface ToolSet {
  personal: Tool[];
  project: Tool[];
  project_error: string;
  project_trusted: boolean;
}

export interface ToolSelection {
  files: string[];
  revision: string;
  revision_number: number;
  branch: string;
  answer: string;
}

export interface ToolOutput {
  command: string;
  exit_code: number | null;
  stdout: string;
  stderr: string;
}

/** A line inside one graph row (crates/tome-core/src/graph.rs). */
export interface Segment {
  from_lane: number;
  from: 'top' | 'mid';
  to_lane: number;
  to: 'mid' | 'bottom';
  /** The revision the line leads down to. */
  target: string;
}

export interface GraphRow {
  revision: Revision;
  lane: number;
  width: number;
  segments: Segment[];
  /** Parents not loaded (the first one is the first parent). */
  missing: string[];
}

export interface Graph {
  rows: GraphRow[];
  /** Branches whose history could not be read in full. */
  incomplete: string[];
}

/** Diffs (crates/tome-core/src/model.rs). */
export interface DiffFile {
  path: string;
  action: string;
  directory: boolean;
}

export interface FilePatch {
  path: string;
  action: string;
  patch: string;
  binary: boolean;
}

export interface RevisionChanges {
  files: DiffFile[];
  patches: FilePatch[];
  first_revision: boolean;
}

export interface BranchState {
  status: Status;
  branches: Branch[];
}

export type Resolution = 'mine' | 'theirs' | 'edited';

/** A push notification from the server (crates/tome-core/src/notify.rs). */
export interface LoreNotification {
  kind: string;
  data: Record<string, unknown>;
}

/** One revision of a file's history. */
export interface FileRevision {
  revision: Revision;
  path: string;
  size: number;
}

/** Long operations run by a worker process (crates/tome-core/src/ops.rs). */
export type JobOp =
  | { op: 'clone'; path: string; url: string; view: string }
  | { op: 'sync'; path: string }
  | { op: 'push'; path: string; branch: string };

export interface JobProgress {
  phase: string;
  done: number;
  total: number;
  bytes: number;
  bytes_total: number;
}

export interface JobEvent {
  id: number;
  progress: JobProgress | null;
  done: [number, string] | null;
  cancelled: boolean;
}
