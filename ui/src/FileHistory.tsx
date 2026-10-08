import { useEffect, useRef, useState } from 'react';
import { explainError } from './errors';
import { invoke } from '@tauri-apps/api/core';
import type { Done, FilePatch, FileRevision } from './types';
import { PatchView, countLines } from './DiffView';

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatTime(ms: number) {
  return ms ? new Date(ms).toLocaleString(undefined, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '';
}

/** The revisions that changed one file, and the selected revision's change to it. */
export default function FileHistory({ path, file, offline, me, onClose, onCommands }: { path: string; file: string; offline: boolean; me: string; onClose: () => void; onCommands: (c: string[]) => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [history, setHistory] = useState<FileRevision[] | null>(null);
  const [error, setError] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [patches, setPatches] = useState<Record<string, FilePatch | null | 'loading'>>({});

  useEffect(() => {
    dialog.current?.showModal();
    invoke<Done<FileRevision[]>>('file_history', { path, file, offline }).then(
      (done) => {
        setHistory(done.value);
        setSelected(done.value[0]?.revision.id ?? null);
        onCommands(done.commands);
      },
      (e) => setError(String(e)),
    );
    // Loaded once per dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const entry = history?.find((h) => h.revision.id === selected) ?? null;
  useEffect(() => {
    if (!entry || patches[entry.revision.id] !== undefined) return;
    const id = entry.revision.id;
    setPatches((p) => ({ ...p, [id]: 'loading' }));
    invoke<FilePatch | null>('file_patch', { path, file: entry.path, revision: id, parent: entry.revision.parents[0] ?? '', offline }).then(
      (patch) => setPatches((p) => ({ ...p, [id]: patch })),
      () => setPatches((p) => ({ ...p, [id]: null })),
    );
  }, [entry, patches, path, offline]);

  const patch = entry ? patches[entry.revision.id] : undefined;
  return (
    <dialog ref={dialog} className="diff-dialog" onClose={onClose} aria-labelledby="fh-title">
      <header className="diff-head">
        <h2 id="fh-title">
          파일 기록 <span className="mono muted">{file}</span>
        </h2>
        <button className="ghost" onClick={() => dialog.current?.close()}>
          닫기
        </button>
      </header>
      {error && <p className="error-line">{explainError(error)}</p>}
      {!history && !error && <p className="muted">불러오는 중…</p>}
      {history && history.length === 0 && <p className="muted">이 파일을 바꾼 리비전이 없습니다(아직 커밋되지 않은 새 파일).</p>}
      {history && history.length > 0 && (
        <div className="diff-body">
          <ul className="diff-files fh-list">
            {history.map((h) => {
              const r = h.revision;
              const p = patches[r.id];
              const n = p && p !== 'loading' && !p.binary ? countLines(p.patch) : null;
              return (
                <li key={r.id}>
                  <button className={r.id === selected ? 'df fh active' : 'df fh'} onClick={() => setSelected(r.id)}>
                    <span className="fh-top">
                      <span className="rev-no">r{r.number}</span>
                      <span className="fh-msg">{r.message || '(메시지 없음)'}</span>
                    </span>
                    <span className="fh-meta muted">
                      {r.author === me && me ? '나' : r.author} · {formatTime(r.timestamp)} · {formatSize(h.size)}
                      {h.path !== file && ` · 당시 경로 ${h.path}`}
                      {n && (
                        <>
                          {' '}
                          · <span className="plus">+{n.add}</span> <span className="minus">−{n.del}</span>
                        </>
                      )}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          <div className="diff-pane">
            {entry && (
              <p className="mono diff-path">
                r{entry.revision.number} · {entry.path}
              </p>
            )}
            {patch === 'loading' && <p className="muted diff-note">불러오는 중…</p>}
            {patch === null && entry && !entry.revision.parents[0] && <p className="muted diff-note">첫 리비전이라 비교할 이전 내용이 없습니다(이 리비전에서 추가됨).</p>}
            {patch === null && entry && entry.revision.parents[0] && <p className="muted diff-note">이 리비전의 변경 내용을 불러오지 못했습니다.</p>}
            {patch && patch !== 'loading' && <PatchView patch={patch} />}
          </div>
        </div>
      )}
    </dialog>
  );
}
