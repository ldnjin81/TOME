import type { Revision, Status } from './types';

/** How a lane segment is drawn: public history, folded history, my stack, uncommitted work. */
type Line = 'public' | 'dotted' | 'stack' | 'wip' | null;
type Node = 'public' | 'draft' | 'head' | 'wip' | null;
interface Cell {
  top?: Line;
  node?: Node;
  bottom?: Line;
}

/** Two lane columns: 0 = the branch's public history, 1 = my stack on top of it. `join` curves
 * lane 1 into this row's node: from above (`stack`, the base of my stack) or from below (`merge`,
 * the second parent of a merge). */
function Lanes({ cells, join }: { cells: [Cell, Cell]; join?: 'stack' | 'merge' }) {
  return (
    <span className="lanes" aria-hidden="true">
      {join && <span className={`join ${join}`} />}
      {cells.map((cell, i) =>
        cell.node ? (
          <span key={i} className="lane">
            <span className={`seg top ${cell.top ?? 'none'}`} />
            <span className={`dot ${cell.node}`} />
            <span className={`seg ${cell.bottom ?? 'none'}`} />
          </span>
        ) : (
          <span key={i} className="lane">
            <span className={`seg ${cell.top ?? cell.bottom ?? 'none'}`} />
          </span>
        ),
      )}
    </span>
  );
}

function relativeTime(ms: number) {
  if (!ms) return '';
  const minutes = Math.round((Date.now() - ms) / 60000);
  if (minutes < 1) return '방금';
  if (minutes < 60) return `${minutes}분 전`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}시간 전`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days}일 전`;
  return new Date(ms).toLocaleDateString();
}

/** A revision is a draft when it is on this branch locally but not yet pushed. */
export function isDraft(revision: Revision, status: Status | undefined) {
  return !!status && status.local_ahead && revision.branch_id === status.branch_id && revision.number > status.remote_number;
}

interface Props {
  status: Status;
  history: Revision[];
  branchName: string;
  selected: string | null;
  mode: 'stack' | 'all';
  busy: boolean;
  onMode: (mode: 'stack' | 'all') => void;
  onSelect: (id: string) => void;
  onCommit: () => void;
  onSync: () => void;
}

export default function Smartlog({ status, history, branchName, selected, mode, busy, onMode, onSelect, onCommit, onSync }: Props) {
  const onCurrentBranch = branchName === status.branch_name;
  const shown = onCurrentBranch ? mode : 'all';

  return (
    <div className="smartlog-body">
      <div className="smartlog-head">
        <div className="chips" role="group" aria-label="보기">
          <button className={shown === 'stack' ? 'chip active' : 'chip'} onClick={() => onMode('stack')} disabled={!onCurrentBranch}>
            내 스택
          </button>
          <button className={shown === 'all' ? 'chip active' : 'chip'} onClick={() => onMode('all')}>
            전체 히스토리
          </button>
        </div>
        <span className="muted">
          {branchName}
          {!onCurrentBranch && ' · 현재 브랜치가 아니라 히스토리만 봅니다'}
        </span>
      </div>
      {shown === 'stack' ? (
        <Stack status={status} history={history} selected={selected} busy={busy} onSelect={onSelect} onCommit={onCommit} onSync={onSync} onAll={() => onMode('all')} />
      ) : (
        <All status={status} history={history} selected={selected} onSelect={onSelect} />
      )}
    </div>
  );
}

function RevisionText({ revision, draft, tags }: { revision: Revision; draft: boolean; tags?: React.ReactNode }) {
  return (
    <span className="rev-text">
      <span className="rev-title">
        <span className={draft ? 'rev-no draft' : 'rev-no'}>{draft ? 'draft' : `r${revision.number}`}</span>
        <span className={draft ? 'rev-message strong' : 'rev-message'}>{revision.message || '(메시지 없음)'}</span>
        {tags}
      </span>
      <span className="rev-meta">
        {revision.author} · {relativeTime(revision.timestamp)} · {draft ? `r${revision.number} · 미푸시` : 'public'}
      </span>
    </span>
  );
}

/** My work against the remote: server head, uncommitted changes, drafts, and the base they sit on. */
function Stack({
  status,
  history,
  selected,
  busy,
  onSelect,
  onCommit,
  onSync,
  onAll,
}: {
  status: Status;
  history: Revision[];
  selected: string | null;
  busy: boolean;
  onSelect: (id: string) => void;
  onCommit: () => void;
  onSync: () => void;
  onAll: () => void;
}) {
  const drafts = history.filter((r) => isDraft(r, status));
  const base = history.find((r) => !isDraft(r, status));
  const changed = status.files.filter((f) => !f.directory);
  const wip = changed.length > 0;
  const remote = status.remote_ahead;
  const newOnServer = base ? Math.max(status.remote_number - base.number, 0) : 0;
  const stack = wip || drafts.length > 0;
  const older = base ? Math.max(base.number - 1, 0) : 0;

  return (
    <ol className="log stack">
      {remote && (
        <li className="lrow">
          <Lanes cells={[{ node: 'public', bottom: 'dotted' }, {}]} />
          <span className="rev-text">
            <span className="rev-title">
              <span className="rev-no">r{status.remote_number}</span>
              <span className="rev-message">서버의 {status.branch_name}</span>
              <span className="tag behind">↓ {newOnServer || '새'} 리비전</span>
            </span>
            <span className="rev-meta">
              아직 받지 않은 리비전이 있습니다 ·{' '}
              <button className="link" onClick={onSync} disabled={busy}>
                Sync
              </button>
            </span>
          </span>
        </li>
      )}

      {wip && (
        <li className="lrow">
          <Lanes cells={[{ top: remote ? 'dotted' : null }, { node: 'wip', bottom: 'wip' }]} />
          <span className="wip-card">
            <span>미커밋 변경 {changed.length}개</span>
            <span className="muted">스테이징 {changed.filter((f) => f.staged).length}</span>
            <button className="primary small" onClick={onCommit}>
              새 커밋
            </button>
          </span>
        </li>
      )}

      {drafts.map((r, i) => (
        <li key={r.id}>
          <button className={r.id === selected ? 'lrow draft active' : 'lrow draft'} onClick={() => onSelect(r.id)}>
            <Lanes cells={[{ top: remote ? 'dotted' : null }, { top: i === 0 && !wip ? null : 'stack', node: i === 0 ? 'head' : 'draft', bottom: 'stack' }]} />
            <RevisionText revision={r} draft tags={i === 0 && <span className="tag">작업 중</span>} />
          </button>
        </li>
      ))}

      {base && (
        <li>
          <button className={base.id === selected ? 'lrow active' : 'lrow'} onClick={() => onSelect(base.id)}>
            <Lanes cells={[{ top: remote ? 'dotted' : null, node: 'public', bottom: older ? 'dotted' : null }, {}]} join={stack ? 'stack' : undefined} />
            <RevisionText
              revision={base}
              draft={false}
              tags={<span className="tag">{stack ? '내 스택의 베이스' : base.id === status.revision ? '작업본' : '최신'}</span>}
            />
          </button>
        </li>
      )}

      {!base && !stack && <li className="muted empty">리비전이 없습니다</li>}

      {older > 0 && (
        <li className="fold">
          ⋮ 이전 public 리비전 {older.toLocaleString()}개 ·{' '}
          <button className="link" onClick={onAll}>
            전체 히스토리 보기
          </button>
        </li>
      )}
    </ol>
  );
}

/** The branch's first-parent chain, newest first, as one lane. */
function All({ status, history, selected, onSelect }: { status: Status; history: Revision[]; selected: string | null; onSelect: (id: string) => void }) {
  return (
    <ol className="log all">
      {history.map((r, i) => {
        const draft = isDraft(r, status);
        const line: Line = draft ? 'stack' : 'public';
        const below = history[i + 1];
        const belowLine: Line = below ? (isDraft(below, status) ? 'stack' : 'public') : null;
        const merge = r.parents.length > 1;
        return (
          <li key={r.id}>
            <button className={r.id === selected ? 'lrow compact active' : 'lrow compact'} onClick={() => onSelect(r.id)}>
              <Lanes cells={[{ top: i > 0 ? line : null, node: draft ? 'draft' : 'public', bottom: belowLine }, {}]} join={merge ? 'merge' : undefined} />
              <RevisionText revision={r} draft={draft} tags={merge && <span className="tag">병합</span>} />
            </button>
          </li>
        );
      })}
    </ol>
  );
}
