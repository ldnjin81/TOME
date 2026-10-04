import { useState } from 'react';
import type { Branch, Graph, GraphRow, Pick, Revision, Segment, StackInfo, Status } from './types';
import { reorderPicks } from './stackLogic';

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

/** A revision is a draft when it is on this branch locally but not yet pushed. With the stack
 * read from the server that is exact; without it, revision numbers above the server's tell it
 * (numbers collide once the branch has diverged). */
export function isDraft(revision: Revision, status: Status | undefined, stack?: StackInfo | null) {
  if (stack?.remote_head) return stack.drafts.some((d) => d.id === revision.id);
  return !!status && status.local_ahead && revision.branch_id === status.branch_id && revision.number > status.remote_number;
}

/** A restack the user asked for by dragging: the picks (oldest first) onto a base. */
export interface RestackRequest {
  onto: string;
  ontoLabel: string;
  picks: Pick[];
}

interface Props {
  status: Status;
  history: Revision[];
  /** Every branch laid out in lanes; null until loaded. */
  graph: Graph | null;
  branches: Branch[];
  /** My stack against the server; null until read (or offline). */
  stack: StackInfo | null;
  selected: string | null;
  mode: 'stack' | 'all';
  busy: boolean;
  onMode: (mode: 'stack' | 'all') => void;
  onSelect: (id: string) => void;
  onCommit: () => void;
  onSync: () => void;
  /** Right-click on a revision: the custom tools menu. */
  onContext: (e: React.MouseEvent, revision: Revision) => void;
  /** My identity: my revisions show 나. */
  me: string;
  /** Opens the restack preview (drag and drop, or the button). */
  onRestack: (request: RestackRequest) => void;
  /** A restack is stopped on a conflict: no new commits or restacks until it is settled. */
  restacking: boolean;
}

/** The author as shown: 나 for my own revisions. */
const who = (author: string, me: string) => (author && author === me ? '나' : author);

export default function Smartlog({ status, history, stack, graph, branches, selected, mode, busy, onMode, onSelect, onCommit, onSync, onContext, me, onRestack, restacking }: Props) {
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
        <Stack me={me} status={status} history={history} stack={stack} selected={selected} busy={busy} onSelect={onSelect} onCommit={onCommit} onSync={onSync} onAll={() => onMode('all')} onContext={onContext} onRestack={onRestack} restacking={restacking} />
      ) : (
        <GraphView me={me} status={status} stack={stack} graph={graph} branches={branches} selected={selected} onSelect={onSelect} onContext={onContext} />
      )}
    </div>
  );
}

function RevisionText({ revision, draft, tags, me }: { revision: Revision; draft: boolean; tags?: React.ReactNode; me: string }) {
  return (
    <span className="rev-text">
      <span className="rev-title">
        <span className={draft ? 'rev-no draft' : 'rev-no'}>{draft ? 'draft' : `r${revision.number}`}</span>
        <span className={draft ? 'rev-message strong' : 'rev-message'}>{revision.message || '(메시지 없음)'}</span>
        {tags}
      </span>
      <span className="rev-meta">
        {who(revision.author, me)} · {relativeTime(revision.timestamp)} · {draft ? `r${revision.number} · 미푸시` : 'public'}
      </span>
    </span>
  );
}

/** Where a dragged draft would land: before (newer than) or after (older than) a draft. */
type DropAt = { id: string; side: 'before' | 'after' } | 'server' | null;

const toPick = (r: Revision): Pick => ({ id: r.id, message: r.message });

/** My work against the remote: server head, uncommitted changes, drafts, and the base they sit on.
 * Drafts can be dragged: onto the server's head to move the whole stack there (restack), or
 * between each other to reorder. */
function Stack({
  me,
  status,
  history,
  stack,
  selected,
  busy,
  onSelect,
  onCommit,
  onSync,
  onAll,
  onContext,
  onRestack,
  restacking,
}: {
  me: string;
  status: Status;
  history: Revision[];
  stack: StackInfo | null;
  selected: string | null;
  busy: boolean;
  onSelect: (id: string) => void;
  onCommit: () => void;
  onSync: () => void;
  onAll: () => void;
  onContext: (e: React.MouseEvent, revision: Revision) => void;
  onRestack: (request: RestackRequest) => void;
  restacking: boolean;
}) {
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<DropAt>(null);
  const exact = !!stack?.remote_head;
  const drafts = exact ? stack!.drafts : history.filter((r) => isDraft(r, status));
  const base = (exact ? stack!.fork : null) ?? history.find((r) => !isDraft(r, status, stack));
  const changed = status.files.filter((f) => !f.directory);
  const wip = changed.length > 0;
  const incoming = exact ? stack!.incoming : [];
  // Lore keeps saying the server is ahead after a restack until the push; the stack knows better.
  const remote = exact ? incoming.length > 0 : status.remote_ahead;
  const newOnServer = exact ? incoming.length : base ? Math.max(status.remote_number - base.number, 0) : 0;
  const diverged = remote && drafts.length > 0;
  const stackShown = wip || drafts.length > 0;
  const older = base ? Math.max(base.number - 1, 0) : 0;
  // Restack needs the stack as the server sees it, and no uncommitted changes.
  const canDrag = exact && !busy && !wip && !restacking && drafts.length > 0;
  const dragTitle = !exact ? '서버에 연결되어야 순서를 바꿀 수 있습니다' : wip ? '미커밋 변경을 먼저 커밋하거나 되돌리세요' : '끌어서 순서 바꾸기 · 서버 리비전 위로 끌면 restack';
  const head = incoming[0];

  function restackOntoServer() {
    if (!stack || !head) return;
    onRestack({ onto: stack.remote_head, ontoLabel: `r${head.number} ${head.message.split('\n')[0]}`, picks: [...drafts].reverse().map(toPick) });
  }

  function reorder(dragged: string, target: string, side: 'before' | 'after') {
    if (!base) return;
    const picks = reorderPicks(drafts, dragged, target, side);
    if (!picks) return;
    onRestack({ onto: base.id, ontoLabel: `r${base.number} ${base.message.split('\n')[0]}`, picks });
  }

  function endDrag() {
    setDragging(null);
    setDropAt(null);
  }

  return (
    <ol className={dragging ? 'log stack dragging' : 'log stack'}>
      {remote && (
        <li
          className={dropAt === 'server' ? 'lrow drop-target' : 'lrow'}
          onDragOver={(e) => {
            if (!dragging || !diverged) return;
            e.preventDefault();
            setDropAt('server');
          }}
          onDragLeave={() => setDropAt((d) => (d === 'server' ? null : d))}
          onDrop={(e) => {
            e.preventDefault();
            endDrag();
            restackOntoServer();
          }}
        >
          <Lanes cells={[{ node: 'public', bottom: 'dotted' }, {}]} />
          <span className="rev-text">
            <span className="rev-title">
              <span className="rev-no">r{head?.number ?? status.remote_number}</span>
              <span className="rev-message">{head ? head.message.split('\n')[0] || '(메시지 없음)' : `서버의 ${status.branch_name}`}</span>
              <span className="tag behind">↓ {newOnServer || '새'} 리비전</span>
            </span>
            {diverged ? (
              <span className="rev-meta diverged">
                서버에 새 리비전이 있어 지금은 push할 수 없습니다 ·{' '}
                <button className="link" onClick={restackOntoServer} disabled={busy || !canDrag} title={canDrag ? '내 스택을 서버 리비전 위로 옮겨 일직선으로 만듭니다' : dragTitle}>
                  내 스택을 위로 옮기기 (restack)
                </button>{' '}
                ·{' '}
                <button className="link" onClick={onSync} disabled={busy || restacking} title="서버 리비전을 받아 병합 리비전을 하나 만듭니다">
                  병합해서 받기 (sync)
                </button>
              </span>
            ) : (
              <span className="rev-meta">
                아직 받지 않은 리비전이 있습니다 ·{' '}
                <button className="link" onClick={onSync} disabled={busy}>
                  Sync
                </button>
              </span>
            )}
          </span>
        </li>
      )}

      {wip && (
        <li className="lrow">
          <Lanes cells={[{ top: remote ? 'dotted' : null }, { node: 'wip', bottom: 'wip' }]} />
          <span className="wip-card">
            <span>미커밋 변경 {changed.length}개</span>
            <span className="muted">스테이징 {changed.filter((f) => f.staged).length}</span>
            {restacking ? (
              <span className="muted small">restack 충돌을 정하는 중 · 위 패널에서 계속하세요</span>
            ) : (
              <button className="primary small" onClick={onCommit}>
                새 커밋
              </button>
            )}
          </span>
        </li>
      )}

      {drafts.map((r, i) => {
        const marker = dropAt && dropAt !== 'server' && dropAt.id === r.id ? ` drop-${dropAt.side}` : '';
        return (
          <li
            key={r.id}
            className={`draft-li${marker}`}
            onDragOver={(e) => {
              if (!dragging || dragging === r.id) return;
              e.preventDefault();
              const box = e.currentTarget.getBoundingClientRect();
              setDropAt({ id: r.id, side: e.clientY < box.top + box.height / 2 ? 'before' : 'after' });
            }}
            onDrop={(e) => {
              e.preventDefault();
              const at = dropAt;
              const dragged = dragging;
              endDrag();
              if (dragged && at && at !== 'server') reorder(dragged, at.id, at.side);
            }}
          >
            <button
              className={`${r.id === selected ? 'lrow draft active' : 'lrow draft'}${dragging === r.id ? ' lifted' : ''}`}
              draggable={canDrag}
              title={drafts.length > 1 || diverged ? dragTitle : undefined}
              onDragStart={(e) => {
                e.dataTransfer.effectAllowed = 'move';
                e.dataTransfer.setData('text/plain', r.id);
                setDragging(r.id);
              }}
              onDragEnd={endDrag}
              onClick={() => onSelect(r.id)}
              onContextMenu={(e) => onContext(e, r)}
            >
              <Lanes cells={[{ top: remote ? 'dotted' : null }, { top: i === 0 && !wip ? null : 'stack', node: i === 0 ? 'head' : 'draft', bottom: 'stack' }]} />
              <RevisionText me={me} revision={r} draft tags={i === 0 && <span className="tag">작업 중</span>} />
              {canDrag && (drafts.length > 1 || diverged) && (
                <span className="grip" aria-hidden="true">
                  ⠿
                </span>
              )}
            </button>
          </li>
        );
      })}

      {base && (
        <li>
          <button className={base.id === selected ? 'lrow active' : 'lrow'} onClick={() => onSelect(base.id)} onContextMenu={(e) => onContext(e, base)}>
            <Lanes cells={[{ top: remote ? 'dotted' : null, node: 'public', bottom: older ? 'dotted' : null }, {}]} join={stackShown ? 'stack' : undefined} />
            <RevisionText
              me={me}
              revision={base}
              draft={false}
              tags={<span className="tag">{stackShown ? '내 스택의 베이스' : base.id === status.revision ? '작업본' : '최신'}</span>}
            />
          </button>
        </li>
      )}

      {!base && !stackShown && <li className="muted empty">리비전이 없습니다</li>}

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
function GraphView({ me, status, stack, graph, branches, selected, onSelect, onContext }: { status: Status; stack: StackInfo | null; graph: Graph | null; branches: Branch[]; selected: string | null; onSelect: (id: string) => void; onContext: (e: React.MouseEvent, revision: Revision) => void; me: string }) {
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
          const draft = isDraft(r, status, stack);
          const tint = branchColor(branchOf.get(r.id) ?? '', status.branch_id);
          return (
            <li key={r.id}>
              <button className={r.id === selected ? 'grow active' : 'grow'} onClick={() => onSelect(r.id)} onContextMenu={(e) => onContext(e, r)}>
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
                    {who(r.author, me)} · {relativeTime(r.timestamp)}
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
