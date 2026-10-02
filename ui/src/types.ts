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
