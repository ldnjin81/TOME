import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Asset, AssetListing, AssetPreview, ChangedFile, Lock } from './types';

/** Previews are asked for in batches, so the first cards fill in before the whole folder is read. */
const BATCH = 24;

const ACTION_LABEL: Record<string, string> = { add: '추가', modify: '수정', delete: '삭제', move: '이동' };

/** A colour family per kind of asset, for cards without a saved image. */
function classTone(cls: string, kind: string) {
  if (kind === 'umap' || cls === 'World') return 'map';
  if (/Blueprint|BlueprintGeneratedClass/.test(cls)) return 'blueprint';
  if (/Sound|MetaSound|Submix/.test(cls)) return 'sound';
  if (/Anim|Pose|Blend|Montage|Chooser|IKR/.test(cls)) return 'anim';
  if (/Material|Texture/.test(cls)) return 'material';
  if (/Enum|Struct|DataTable|DataAsset|Curve|Input/.test(cls)) return 'data';
  return 'other';
}

export function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export interface AssetMarks {
  changed: Map<string, ChangedFile>;
  locks: Map<string, Lock>;
}

/** The status and lock marks for asset cards, by path. */
export function assetMarks(files: ChangedFile[], locks: Lock[]): AssetMarks {
  return { changed: new Map(files.filter((f) => !f.directory).map((f) => [f.path, f])), locks: new Map(locks.map((l) => [l.path, l])) };
}

function Badges({ path, marks, me }: { path: string; marks: AssetMarks; me: string }) {
  const change = marks.changed.get(path);
  const lock = marks.locks.get(path);
  return (
    <>
      {change?.unresolved && <span className="ab conflict">충돌</span>}
      {change && !change.unresolved && <span className={`ab change ${change.action}`}>{ACTION_LABEL[change.action] ?? change.action}</span>}
      {lock && (
        <span className={lock.owner === me ? 'ab lock mine' : 'ab lock'} title={`잠금: ${lock.owner}`}>
          🔒 {lock.owner === me ? '내 잠금' : lock.owner}
        </span>
      )}
    </>
  );
}

function Thumb({ asset, preview }: { asset: Asset; preview: AssetPreview | undefined }) {
  if (preview?.image) return <img src={preview.image} alt="" draggable={false} />;
  const cls = preview?.class || (asset.kind === 'umap' ? 'World' : '');
  return (
    <span className={`thumb-ph ${classTone(cls, asset.kind)}`}>
      {preview ? cls || (asset.kind === 'umap' ? 'Map' : '미리보기 없음') : ''}
    </span>
  );
}

interface TreeProps {
  path: string;
  folder: string;
  onOpen: (folder: string) => void;
}

/** Folders, opened on demand. */
function FolderTree({ path, folder, onOpen }: TreeProps) {
  const [children, setChildren] = useState<Record<string, { name: string; path: string }[]>>({});
  const [open, setOpen] = useState<Set<string>>(new Set(['']));

  async function load(at: string) {
    if (children[at]) return;
    try {
      const listing = await invoke<AssetListing>('list_assets', { path, folder: at });
      setChildren((c) => ({ ...c, [at]: listing.folders }));
    } catch {
      setChildren((c) => ({ ...c, [at]: [] }));
    }
  }

  useEffect(() => {
    setChildren({});
    setOpen(new Set(['']));
  }, [path]);

  // Open the way down to the current folder (and the folder itself, to show what is in it).
  useEffect(() => {
    const parts = folder.split('/').filter(Boolean);
    setOpen((o) => new Set([...o, ...parts.map((_, i) => parts.slice(0, i + 1).join('/'))]));
  }, [folder]);

  useEffect(() => {
    for (const at of open) void load(at);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, children]);

  function toggle(at: string) {
    setOpen((o) => {
      const next = new Set(o);
      if (next.has(at)) next.delete(at);
      else next.add(at);
      return next;
    });
  }

  function render(at: string, depth: number): React.ReactNode {
    const list = children[at];
    if (!list) return null;
    return list.map((f) => (
      <li key={f.path}>
        <div className={f.path === folder ? 'tree-row active' : 'tree-row'} style={{ paddingLeft: 4 + depth * 14 }}>
          <button className="tree-twisty" onClick={() => toggle(f.path)} aria-label={open.has(f.path) ? '접기' : '펼치기'} aria-expanded={open.has(f.path)}>
            {children[f.path]?.length === 0 ? '' : open.has(f.path) ? '▾' : '▸'}
          </button>
          <button className="tree-name" onClick={() => onOpen(f.path)} title={f.path}>
            {f.name}
          </button>
        </div>
        {open.has(f.path) && <ul>{render(f.path, depth + 1)}</ul>}
      </li>
    ));
  }

  return (
    <nav className="folder-tree" aria-label="폴더">
      <div className={folder === '' ? 'tree-row active' : 'tree-row'}>
        <button className="tree-name root" onClick={() => onOpen('')}>
          작업본 루트
        </button>
      </div>
      <ul>{render('', 0)}</ul>
    </nav>
  );
}

interface Props {
  path: string;
  marks: AssetMarks;
  me: string;
  selected: Asset | null;
  previews: Record<string, AssetPreview>;
  onPreviews: (list: AssetPreview[]) => void;
  onSelect: (asset: Asset | null) => void;
  onContext: (e: React.MouseEvent, files: string[]) => void;
  onError: (message: string) => void;
}

/** The asset browser: folder tree, and the folder's packages as thumbnails with status and lock marks. */
export default function Assets({ path, marks, me, selected, previews, onPreviews, onSelect, onContext, onError }: Props) {
  const [folder, setFolder] = useState<string | null>(null);
  const [listing, setListing] = useState<AssetListing | null>(null);
  const [filter, setFilter] = useState('');
  const [onlyMarked, setOnlyMarked] = useState(false);
  const [size, setSize] = useState(128);
  const asked = useRef(new Set<string>());

  // Start in Content when the working copy has one.
  useEffect(() => {
    asked.current.clear();
    setListing(null);
    invoke<AssetListing>('list_assets', { path, folder: '' }).then(
      (root) => setFolder(root.folders.some((f) => f.name === 'Content') ? 'Content' : ''),
      (e) => onError(String(e)),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path]);

  useEffect(() => {
    if (folder === null) return;
    let stale = false;
    invoke<AssetListing>('list_assets', { path, folder }).then(
      (l) => {
        if (!stale) setListing(l);
      },
      (e) => onError(String(e)),
    );
    return () => {
      stale = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, folder]);

  // Read previews of the folder in batches; changed files again when their time changes.
  useEffect(() => {
    if (!listing) return;
    const seen = asked.current;
    const want = listing.assets.filter((a) => !seen.has(`${a.path}@${a.modified}`));
    want.forEach((a) => seen.add(`${a.path}@${a.modified}`));
    let stopped = false;
    void (async () => {
      for (let i = 0; i < want.length && !stopped; i += BATCH) {
        try {
          onPreviews(await invoke<AssetPreview[]>('asset_previews', { path, files: want.slice(i, i + BATCH).map((a) => a.path) }));
        } catch (e) {
          onError(String(e));
          return;
        }
      }
    })();
    return () => {
      stopped = true;
      // Unfinished batches are asked again when the folder is shown again.
      want.forEach((a) => !previews[a.path] && seen.delete(`${a.path}@${a.modified}`));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [listing]);

  const shown = useMemo(() => {
    if (!listing) return [];
    const needle = filter.trim().toLowerCase();
    return listing.assets.filter((a) => {
      if (onlyMarked && !marks.changed.has(a.path) && !marks.locks.has(a.path)) return false;
      if (!needle) return true;
      return a.name.toLowerCase().includes(needle) || (previews[a.path]?.class ?? '').toLowerCase().includes(needle);
    });
  }, [listing, filter, onlyMarked, marks, previews]);

  const crumbs = (folder ?? '').split('/').filter(Boolean);

  return (
    <div className="assets">
      <FolderTree path={path} folder={folder ?? ''} onOpen={(f) => { setFolder(f); onSelect(null); }} />
      <section className="asset-main">
        <div className="asset-bar">
          <nav className="crumbs" aria-label="현재 폴더">
            <button className="link" onClick={() => setFolder('')}>
              루트
            </button>
            {crumbs.map((c, i) => (
              <span key={i}>
                {' / '}
                <button className="link" onClick={() => setFolder(crumbs.slice(0, i + 1).join('/'))}>
                  {c}
                </button>
              </span>
            ))}
          </nav>
          <input className="asset-filter" type="search" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="이름·클래스 거르기" aria-label="이름이나 클래스로 거르기" />
          <label className="check">
            <input type="checkbox" checked={onlyMarked} onChange={(e) => setOnlyMarked(e.target.checked)} />
            변경·잠금만
          </label>
          <label className="check size" title="썸네일 크기">
            <span aria-hidden="true">▫</span>
            <input type="range" min={88} max={224} step={8} value={size} onChange={(e) => setSize(Number(e.target.value))} aria-label="썸네일 크기" />
            <span aria-hidden="true">◻</span>
          </label>
        </div>
        {listing && (
          <p className="asset-count muted small">
            에셋 {shown.length}
            {shown.length !== listing.assets.length && ` / ${listing.assets.length}`}개 · 하위 폴더 {listing.folders.length}개
          </p>
        )}
        {listing && listing.folders.length > 0 && !filter && !onlyMarked && (
          <ul className="subfolders">
            {listing.folders.map((f) => (
              <li key={f.path}>
                <button className="subfolder" onDoubleClick={() => setFolder(f.path)} onClick={() => setFolder(f.path)}>
                  <span aria-hidden="true">📁</span> {f.name}
                </button>
              </li>
            ))}
          </ul>
        )}
        {listing && shown.length === 0 && (listing.assets.length > 0 || listing.folders.length === 0) && <p className="muted asset-empty">{listing.assets.length ? '거른 결과가 없습니다' : '이 폴더에는 에셋(.uasset·.umap)이 없습니다'}</p>}
        <ul className="asset-grid" style={{ '--thumb': `${size}px` } as React.CSSProperties}>
          {shown.map((a) => {
            const preview = previews[a.path];
            return (
              <li key={a.path}>
                <button
                  className={selected?.path === a.path ? 'asset-card active' : 'asset-card'}
                  onClick={() => onSelect(a)}
                  onContextMenu={(e) => {
                    onSelect(a);
                    onContext(e, [a.path]);
                  }}
                  title={`${a.path}${preview?.class ? `\n${preview.class}` : ''}`}
                >
                  <span className="thumb">
                    <Thumb asset={a} preview={preview} />
                    <span className="asset-badges">
                      <Badges path={a.path} marks={marks} me={me} />
                    </span>
                  </span>
                  <span className="asset-name">{a.name}</span>
                  <span className="asset-class muted">{preview?.class || (a.kind === 'umap' ? 'Map' : '')}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </section>
    </div>
  );
}

interface DetailsProps {
  asset: Asset;
  preview: AssetPreview | undefined;
  marks: AssetMarks;
  me: string;
  busy: boolean;
  online: boolean;
  onLock: (lock: boolean) => void;
  onHistory: () => void;
  onChanges: () => void;
}

/** The selected asset: its large thumbnail, facts, and what can be done with it. */
export function AssetDetails({ asset, preview, marks, me, busy, online, onLock, onHistory, onChanges }: DetailsProps) {
  const lock = marks.locks.get(asset.path);
  const change = marks.changed.get(asset.path);
  const lockedByOther = !!lock && lock.owner !== me;
  return (
    <div className="asset-details">
      <div className="asset-hero">
        <Thumb asset={asset} preview={preview} />
      </div>
      <h2 className="asset-title">{asset.name}</h2>
      <p className="asset-path mono">{asset.path}</p>
      <dl>
        <dt>클래스</dt>
        <dd>{preview?.class || (asset.kind === 'umap' ? 'World (맵)' : '알 수 없음')}</dd>
        <dt>크기</dt>
        <dd>{formatSize(asset.size)}</dd>
        <dt>수정</dt>
        <dd>{new Date(asset.modified).toLocaleString('ko-KR')}</dd>
        {preview?.image && (
          <>
            <dt>썸네일</dt>
            <dd>
              {preview.width}×{preview.height}
            </dd>
          </>
        )}
        <dt>상태</dt>
        <dd>
          {change ? (change.unresolved ? '병합 충돌 — 변경 탭에서 정하세요' : `${ACTION_LABEL[change.action] ?? change.action}됨${change.staged ? ' · 스테이지됨' : ''}`) : '변경 없음'}
        </dd>
        <dt>잠금</dt>
        <dd>{lock ? `${lock.owner === me ? '내가' : lock.owner + '님이'} 잠금` : '잠기지 않음'}</dd>
      </dl>
      <div className="asset-actions">
        {lock && lock.owner === me ? (
          <button className="primary" onClick={() => onLock(false)} disabled={busy || !online}>
            잠금 풀기
          </button>
        ) : (
          <button className="primary" onClick={() => onLock(true)} disabled={busy || !online || lockedByOther} title={lockedByOther ? `${lock!.owner}님이 잠그고 있습니다` : '다른 사람이 동시에 고치지 못하게 잠급니다'}>
            잠그기
          </button>
        )}
        <button className="ghost" onClick={onHistory}>
          파일 기록
        </button>
        {change && (
          <button className="ghost" onClick={onChanges}>
            변경 탭에서 보기
          </button>
        )}
      </div>
      {!online && <p className="muted small">오프라인이라 잠금을 바꿀 수 없습니다.</p>}
      {lockedByOther && <p className="warn-line">{lock!.owner}님이 작업 중입니다. 고치기 전에 풀릴 때까지 기다리거나 연락하세요.</p>}
    </div>
  );
}
