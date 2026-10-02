import type { Branch, Graph, GraphRow, Revision, Segment, Status } from './types';

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
  /** Every branch laid out in lanes; null until loaded. */
  graph: Graph | null;
  branches: Branch[];
  selected: string | null;
  mode: 'stack' | 'all';
  busy: boolean;
  onMode: (mode: 'stack' | 'all') => void;
  onSelect: (id: string) => void;
  onCommit: () => void;
  onSync: () => void;
}

export default function Smartlog({ status, history, graph, branches, selected, mode, busy, onMode, onSelect, onCommit, onSync }: Props) {
  // The stack is always the working copy's branch; the graph shows every branch.
  const shown = mode;

  return (
    <div className="smartlog-body">
      <div className="smartlog-head">
        <div className="chips" role="group" aria-label="보기">
          <button className={shown === 'stack' ? 'chip active' : 'chip'} onClick={() => onMode('stack')}>
            내 스택
          </button>
          <button className={shown === 'all' ? 'chip active' : 'chip'} onClick={() => onMode('all')}>
            전체 그래프
          </button>
        </div>
        <span className="muted">{shown === 'stack' ? status.branch_name : '모든 브랜치'}</span>
      </div>
      {shown === 'stack' ? (
        <Stack status={status} history={history} selected={selected} busy={busy} onSelect={onSelect} onCommit={onCommit} onSync={onSync} onAll={() => onMode('all')} />
      ) : (
        <GraphView status={status} graph={graph} branches={branches} selected={selected} onSelect={onSelect} />
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
            전체 그래프 보기
          </button>
        </li>
      )}
    </ol>
  );
}

const LANE = 18;
const ROW = 48;
const MID = 24;

const laneX = (lane: number) => lane * LANE + LANE / 2;
const endY = (end: Segment['from'] | Segment['to']) => (end === 'top' ? 0 : end === 'mid' ? MID : ROW);

/** Lane color from the branch id, so a renamed branch keeps its color; the current branch uses the accent. */
export function branchColor(branchId: string, current: string) {
  if (!branchId) return 'var(--public-line)';
  if (branchId === current) return 'var(--accent)';
  let hash = 0;
  for (const ch of branchId) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return `hsl(${hash % 360} 55% 52%)`;
}

function segmentPath(s: Segment) {
  const [x1, y1, x2, y2] = [laneX(s.from_lane), endY(s.from), laneX(s.to_lane), endY(s.to)];
  if (x1 === x2) return `M${x1} ${y1}V${y2}`;
  const my = (y1 + y2) / 2;
  return `M${x1} ${y1}C${x1} ${my} ${x2} ${my} ${x2} ${y2}`;
}

/** One row's lanes: lines through the row, the node, and stubs for parents that were not loaded. */
function RowGraph({ row, width, color, draft }: { row: GraphRow; width: number; color: (id: string) => string; draft: boolean }) {
  const x = laneX(row.lane);
  const own = color(row.revision.id);
  return (
    <svg className="row-graph" width={width * LANE} height={ROW} viewBox={`0 0 ${width * LANE} ${ROW}`} aria-hidden="true">
      {row.segments.map((s, i) => (
        <path key={i} d={segmentPath(s)} fill="none" stroke={color(s.target)} strokeWidth="2" />
      ))}
      {row.missing.map((p, i) =>
        p === row.revision.parents[0] ? (
          <path key={p} d={`M${x} ${MID}V${ROW}`} fill="none" stroke={own} strokeWidth="2" strokeDasharray="2 3">
            <title>더 이전 리비전은 불러오지 않았습니다</title>
          </path>
        ) : (
          <g key={p}>
            <title>병합된 쪽 리비전을 불러오지 못했습니다</title>
            <path d={`M${x} ${MID}Q${x + LANE * 0.8} ${MID} ${x + LANE * 0.8} ${ROW - 8 - i * 2}`} fill="none" stroke="var(--public-line)" strokeWidth="2" strokeDasharray="2 3" />
            <circle cx={x + LANE * 0.8} cy={ROW - 6 - i * 2} r="2.5" fill="var(--surface)" stroke="var(--public-line)" strokeWidth="1.5" />
          </g>
        ),
      )}
      {draft ? <circle cx={x} cy={MID} r="5" fill="var(--surface)" stroke={own} strokeWidth="2.5" /> : <circle cx={x} cy={MID} r="5" fill={own} />}
    </svg>
  );
}

/** Every branch, laid out in lanes by tome-core (graph.rs). */
function GraphView({ status, graph, branches, selected, onSelect }: { status: Status; graph: Graph | null; branches: Branch[]; selected: string | null; onSelect: (id: string) => void }) {
  if (!graph) return <p className="muted empty">그래프를 불러오는 중…</p>;
  // A revision without a branch id (some merges) takes its child's, so a lane keeps one color.
  const branchOf = new Map<string, string>();
  for (const { revision: r } of graph.rows) {
    if (r.branch_id) branchOf.set(r.id, r.branch_id);
    const own = branchOf.get(r.id) ?? '';
    const first = r.parents[0];
    if (first && own && !branchOf.has(first)) branchOf.set(first, own);
  }
  const color = (id: string) => branchColor(branchOf.get(id) ?? '', status.branch_id);
  const heads = new Map<string, string[]>();
  for (const b of branches) heads.set(b.latest, [...(heads.get(b.latest) ?? []), b.name]);
  const width = Math.max(1, ...graph.rows.map((r) => r.width));
  return (
    <>
      {graph.incomplete.length > 0 && <p className="muted note-line">일부 브랜치는 끝까지 읽지 못했습니다: {graph.incomplete.join(' · ')}</p>}
      <ol className="log graph">
        {graph.rows.map((row) => {
          const r = row.revision;
          const draft = isDraft(r, status);
          const tint = branchColor(branchOf.get(r.id) ?? '', status.branch_id);
          return (
            <li key={r.id}>
              <button className={r.id === selected ? 'grow active' : 'grow'} onClick={() => onSelect(r.id)}>
                <RowGraph row={row} width={width} color={color} draft={draft} />
                <span className="rev-title">
                  <span className={draft ? 'rev-no draft' : 'rev-no'}>r{r.number}</span>
                  {(heads.get(r.id) ?? []).map((name) => (
                    <span key={name} className="tag head" style={{ borderColor: tint, color: tint }}>
                      {name}
                    </span>
                  ))}
                  <span className="rev-message">{r.message || '(메시지 없음)'}</span>
                  <span className="rev-meta inline">
                    {r.author} · {relativeTime(r.timestamp)}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ol>
    </>
  );
}
