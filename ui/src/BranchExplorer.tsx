import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { t, getLocale } from './i18n';
import { branchColor, isDraft } from './Smartlog';
import { Author } from './author';
import { adjacentNode, clampZoom, recentPosition, visibleLanes, type ExplorerLayout, type ExplorerNode } from './branchExplorerLogic';
import type { StackInfo, Status } from './types';

const ROW = 58;
const HEADER = 40;
const date = (start: number, end: number) => new Intl.DateTimeFormat(getLocale(), { year: 'numeric', month: 'short', day: 'numeric' }).formatRange(new Date(start), new Date(end));

export default function BranchExplorer({ path, offline, status, stack, selected, busy, onSelect, onData, onNewBranch }: {
  path: string; offline: boolean; status: Status; stack: StackInfo | null; selected: string | null; busy: boolean;
  onSelect: (id: string) => void; onData: (layout: ExplorerLayout | null) => void; onNewBranch: () => void;
}) {
  const [data, setData] = useState<ExplorerLayout | null>(null);
  const [archived, setArchived] = useState(false);
  const [query, setQuery] = useState('');
  const [collapsed, setCollapsed] = useState(new Set<string>());
  const [highlight, setHighlight] = useState('');
  const [zoom, setZoom] = useState(1);
  const [refresh, setRefresh] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const viewport = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const [view, setView] = useState({ left: 0, top: 0, width: 1000, height: 800 });
  const [tip, setTip] = useState<{ node: ExplorerNode; x: number; y: number } | null>(null);

  useEffect(() => {
    let cancelled = false;
    // 외부 요청의 대기 상태를 렌더 후 반영한다.
    queueMicrotask(() => { if (!cancelled) { setLoading(true); setError(''); } });
    invoke<ExplorerLayout>('branch_explorer', { path, offline, archived, timezoneOffsetMinutes: -new Date().getTimezoneOffset() }).then((layout) => {
      if (cancelled) return;
      setData(layout); onData(layout);
      requestAnimationFrame(() => {
        const el = viewport.current;
        if (el) { const position = recentPosition(layout.nodes, el.clientWidth, el.clientHeight, zoomRef.current); el.scrollLeft = position.left; el.scrollTop = position.top; }
      });
    }, (e) => { if (!cancelled) setError(String(e)); }).finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [path, offline, archived, refresh, onData, status.revision]);

  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const update = () => setView({ left: el.scrollLeft, top: el.scrollTop, width: el.clientWidth, height: el.clientHeight });
    const observer = new ResizeObserver(update); observer.observe(el); update();
    return () => observer.disconnect();
  }, []);

  const visible = useMemo(() => visibleLanes(data?.lanes ?? [], collapsed, query), [data, collapsed, query]);
  const positions = new Map(visible.map((lane, i) => [lane, HEADER + i*ROW + ROW/2]));
  const color = (lane: number) => branchColor(data?.lanes[lane].branch.id ?? '', status.branch_id);
  const y = (node: ExplorerNode) => positions.get(node.lane) ?? -100;
  const childIds = new Set(data?.lanes.map((l) => l.parent));

  function changeZoom(next: number, anchor?: number) {
    const el = viewport.current;
    const value = clampZoom(next);
    if (el) {
      const point = anchor ?? el.clientWidth/2;
      const left = (el.scrollLeft+point)/zoom*value-point;
      requestAnimationFrame(() => { el.scrollLeft = left; });
    }
    setZoom(value);
  }
  // 비수동 휠 핸들러여야 브라우저 페이지 확대를 막을 수 있다.
  const zoomRef = useRef(zoom);
  useEffect(() => { zoomRef.current = zoom; }, [zoom]);
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const old = zoomRef.current;
      const next = clampZoom(old * Math.exp(-e.deltaY*0.002));
      const anchor = e.clientX-el.getBoundingClientRect().left;
      const left = (el.scrollLeft+anchor)/old*next-anchor;
      zoomRef.current = next; setZoom(next);
      requestAnimationFrame(() => { el.scrollLeft = left; });
    };
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  }, []);

  function reveal(node: ExplorerNode, focus = false) {
    onSelect(node.revision.id);
    setHighlight(data?.lanes[node.lane].branch.id ?? '');
    const el = viewport.current;
    if (el) {
      el.scrollLeft = node.x*zoom-el.clientWidth/2;
      el.scrollTop = y(node)-el.clientHeight/2;
    }
    if (focus) requestAnimationFrame(() => viewport.current?.querySelector<SVGGElement>(`[data-revision="${node.revision.id}"]`)?.focus());
  }

  return <div className="branch-explorer">
    <div className="explorer-toolbar">
      <strong>{t('브랜치 탐색기')}</strong>
      <button onClick={() => setRefresh((n) => n+1)} disabled={loading}>{t('새로고침')}</button>
      <button onClick={onNewBranch} disabled={busy}>{t('새 브랜치')}</button>
      <span className="explorer-spacer" />
      <button aria-label={t('축소')} onClick={() => changeZoom(zoom/1.25)}>−</button>
      <output>{Math.round(zoom*100)}%</output>
      <button aria-label={t('확대')} onClick={() => changeZoom(zoom*1.25)}>+</button>
      <button onClick={() => { changeZoom((viewport.current?.clientWidth ?? 600)/(data?.width ?? 600)); requestAnimationFrame(() => { if (viewport.current) { viewport.current.scrollTop=0; viewport.current.scrollLeft=0; } }); }}>{t('화면 맞춤')}</button>
    </div>
    <div className="explorer-filters">
      <input aria-label={t('브랜치 검색')} placeholder={t('브랜치 검색')} value={query} onChange={(e) => setQuery(e.target.value)} />
      <label className="check"><input type="checkbox" checked={archived} onChange={(e) => setArchived(e.target.checked)} />{t('보관 브랜치 표시')}</label>
      <span className="muted small">{t('날짜: 로컬 시간 · ⌘/Ctrl+휠 확대 · 빈 곳을 끌어 이동')}</span>
    </div>
    {loading && <p role="status" className="muted">{t('브랜치 불러오는 중…')}</p>}
    {error && <p role="alert" className="explorer-error">{error}</p>}
    {!!data?.incomplete.length && <details className="explorer-warning"><summary>{t('일부 브랜치의 기록을 다 읽지 못했습니다 ({count})', { count: data.incomplete.length })}</summary>{data.incomplete.map((s) => <div key={s}>{s}</div>)}</details>}
    <div className="explorer-body">
      <div className="explorer-tree" onWheel={(e) => { if (viewport.current) viewport.current.scrollTop+=e.deltaY; }} aria-label={t('브랜치 트리')}>
        <div className="explorer-tree-head">{t('브랜치')} <span className="muted">{visible.length}</span></div>
        <div style={{ transform: `translateY(${-view.top}px)` }}>
          {visible.map((lane) => {
            const l = data!.lanes[lane]; const id = l.branch.id;
            return <div className={`explorer-tree-row${highlight===id ? ' active' : ''}`} key={id} style={{ paddingLeft: 6+Math.min(l.depth, 6)*12 }}>
              {childIds.has(id) ? <button className="explorer-fold" aria-label={`${t('접기/펼치기')}: ${l.branch.name}`} aria-expanded={query.trim() ? true : !collapsed.has(id)} onClick={() => setCollapsed((old) => { const next=new Set(old); if (next.has(id)) next.delete(id); else next.add(id); return next; })}>{collapsed.has(id) && !query.trim() ? '▸' : '▾'}</button> : <span className="explorer-fold" />}
              <button className="explorer-branch" title={l.branch.name} onClick={() => {
                setHighlight(id);
                const el=viewport.current; if (el) el.scrollTop=(positions.get(lane) ?? 0)-el.clientHeight/2;
                const last=data!.nodes.filter((n) => n.lane===lane).at(-1); if (el && last) el.scrollLeft=last.x*zoom-el.clientWidth/2;
              }}><span className="lane-dot" style={{ background: color(lane) }} /><span className="name">{l.branch.name}</span>{l.branch.current && <span aria-label={t('현재 브랜치')}>✓</span>}{id===data!.root && <span className="badge">{t('기본')}</span>}{l.branch.archived && <span className="muted">{t('보관')}</span>}</button>
            </div>;
          })}
        </div>
      </div>
      <div className="explorer-viewport" ref={viewport} tabIndex={0} aria-label={t('브랜치 타임라인')} onScroll={(e) => { const el=e.currentTarget; setView({ left: el.scrollLeft, top: el.scrollTop, width: el.clientWidth, height: el.clientHeight }); setTip(null); }}
        onKeyDown={(e) => {
          if (e.target!==e.currentTarget || !data || !['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Enter'].includes(e.key)) return;
          e.preventDefault();
          const target=data.nodes.find((n) => n.revision.id===selected && positions.has(n.lane)) ?? data.nodes.filter((n) => positions.has(n.lane)).at(-1);
          if (target) reveal(target,true);
        }}
        onPointerDown={(e) => { if (e.button!==0 || (e.target as Element).closest('[data-revision]')) return; const el=e.currentTarget; drag.current={x:e.clientX,y:e.clientY,left:el.scrollLeft,top:el.scrollTop}; el.setPointerCapture(e.pointerId); }}
        onPointerMove={(e) => { const d=drag.current; if (!d) return; e.currentTarget.scrollLeft=d.left-e.clientX+d.x; e.currentTarget.scrollTop=d.top-e.clientY+d.y; }}
        onPointerUp={() => { drag.current=null; }} onPointerCancel={() => { drag.current=null; }}>
        <div className="explorer-canvas" style={{ width: (data?.width ?? 320)*zoom, height: Math.max(visible.length*ROW+HEADER, view.height) }}>
          <div className="explorer-lane-labels" aria-hidden="true">
            {visible.map((lane) => <span key={lane} className="explorer-lane-name" style={{ top: positions.get(lane)!-27, color: color(lane), maxWidth: Math.max(0, view.width-24) }}>{data!.lanes[lane].branch.name}</span>)}
          </div>
          <div className="explorer-date-head" aria-hidden="true" style={{ height: HEADER, width: (data?.width ?? 320)*zoom }}>
            {data?.columns.filter((c) => (c.x+c.width)*zoom>=view.left && c.x*zoom<=view.left+view.width).map((c) => <div key={c.start} className="explorer-date-cell" style={{ left: c.x*zoom, width: c.width*zoom }}><span>{date(c.start, c.end)}</span></div>)}
          </div>
        <svg width={(data?.width ?? 320)*zoom} height={Math.max(visible.length*ROW+HEADER, view.height)} role="group" aria-label={t('브랜치 타임라인')}>
          {data?.columns.map((c) => <g key={c.start} aria-hidden="true"><rect x={c.x*zoom} y={0} width={c.width*zoom} height="100%" className={c.gap ? 'explorer-gap' : 'explorer-column'} /><line x1={c.x*zoom} x2={c.x*zoom} y1={0} y2="100%" className="explorer-grid" /></g>)}
          {visible.map((lane) => <g key={lane} aria-hidden="true"><rect x={0} y={positions.get(lane)!-ROW/2} width="100%" height={ROW} className={highlight===data!.lanes[lane].branch.id ? 'explorer-lane selected' : 'explorer-lane'} /><line x1={0} x2="100%" y1={positions.get(lane)} y2={positions.get(lane)} stroke={color(lane)} opacity={0.24} /></g>)}
          {data?.edges.map((edge, i) => {
            const a=data.nodes[edge.from], b=data.nodes[edge.to];
            if (!positions.has(a.lane) || !positions.has(b.lane)) return null;
            if (Math.max(a.x,b.x)*zoom<view.left-40 || Math.min(a.x,b.x)*zoom>view.left+view.width+40) return null;
            const ax=a.x*zoom,bx=b.x*zoom,ay=y(a),by=y(b),mid=(ax+bx)/2;
            return <path key={i} d={`M${ax} ${ay}H${mid}Q${mid+8} ${ay} ${mid+8} ${(ay+by)/2}T${bx} ${by}`} fill="none" stroke={color(edge.kind==='merge' ? a.lane : b.lane)} strokeWidth={edge.kind==='line' ? 2 : 2.5} strokeDasharray={isDraft(b.revision,status,stack) ? '5 4' : undefined} className={`explorer-edge ${edge.kind}`} />;
          })}
          {data?.nodes.filter((n) => positions.has(n.lane) && (n.revision.id===selected || (n.x*zoom>=view.left-40 && n.x*zoom<=view.left+view.width+40 && y(n)>=view.top-ROW && y(n)<=view.top+view.height+ROW))).map((n) => {
            const r=n.revision, draft=isDraft(r,status,stack), label=`r${r.number} · ${r.message.split('\n')[0] || t('(메시지 없음)')} · ${r.author} · ${new Date(r.timestamp).toLocaleString(getLocale())}${draft ? ` · ${t('미푸시')}` : ''}`;
            return <g key={r.id} data-revision={r.id} role="button" tabIndex={selected===r.id || (!data.nodes.some((n) => n.revision.id===selected && positions.has(n.lane)) && n===data.nodes.find((n) => positions.has(n.lane))) ? 0 : -1} aria-label={label} aria-pressed={selected===r.id} className={`explorer-node${draft ? ' draft' : ''}${selected===r.id ? ' active' : ''}`} transform={`translate(${n.x*zoom} ${y(n)})`} style={{ color: color(n.lane) }} onClick={() => reveal(n)} onFocus={() => onSelect(r.id)} onMouseEnter={(e) => setTip({node:n,x:e.clientX,y:e.clientY})} onMouseLeave={() => setTip(null)} onKeyDown={(e) => {
              if (e.key==='Enter' || e.key===' ') { e.preventDefault(); reveal(n); }
              if (['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key)) { e.preventDefault(); const next=adjacentNode(data.nodes,visible,r.id,e.key); if (next) reveal(next,true); }
            }}><title>{label}</title><circle r={13} fill="transparent" stroke="none" /><circle r={selected===r.id ? 9 : 6} fill={draft ? 'var(--surface)' : 'currentColor'} stroke="currentColor" strokeWidth={2} strokeDasharray={draft ? '3 2' : undefined} />{r.parents.length>1 && <circle r={3} fill="var(--surface)" />}</g>;
          })}

        </svg>
        </div>
        {data && !visible.length && <p className="explorer-empty">{offline && !data.lanes.length ? t('오프라인에 저장된 브랜치가 없습니다. 연결 후 새로고침하세요.') : t('일치하는 브랜치가 없습니다')}</p>}
        {data && visible.length>0 && !data.nodes.length && <p className="explorer-empty">{t('리비전이 없습니다')}</p>}
      </div>
    </div>
    {tip && <div role="tooltip" className="explorer-tooltip" style={{ left: Math.max(8,Math.min(tip.x+12,window.innerWidth-300)), top: Math.max(8,Math.min(tip.y+16,window.innerHeight-140)) }}><strong>r{tip.node.revision.number} · {tip.node.revision.message.split('\n')[0] || t('(메시지 없음)')}</strong><div><Author name={tip.node.revision.author} /></div><div>{new Date(tip.node.revision.timestamp).toLocaleString(getLocale())}</div>{isDraft(tip.node.revision,status,stack) && <div>{t('미푸시')}</div>}</div>}
  </div>;
}
