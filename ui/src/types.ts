// Mirrors tome-core's models (crates/tome-core/src/model.rs).

export interface ChangedFile {
  path: string;
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
