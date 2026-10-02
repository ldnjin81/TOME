// Mirrors tome-core's models (crates/tome-core/src/model.rs).

export interface ChangedFile {
  path: string;
  /** A directory entry (status lists directories too). */
  directory: boolean;
  action: string;
  staged: boolean;
  conflict: boolean;
}

export interface Status {
  branch_id: string;
  branch_name: string;
  revision: string;
  revision_number: number;
  remote_number: number;
  local_ahead: boolean;
  remote_ahead: boolean;
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
