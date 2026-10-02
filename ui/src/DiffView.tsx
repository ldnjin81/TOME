import { useEffect, useRef, useState } from 'react';
import type { DiffFile, FilePatch } from './types';

export const ACTION_MARK: Record<string, string> = { add: 'A', modify: 'M', delete: 'D', move: 'R', copy: 'C' };

/** Added and removed line counts of a unified diff (headers left out). */
export function countLines(patch: string) {
  let add = 0;
  let del = 0;
  for (const line of patch.split('\n')) {
    if (line.startsWith('+++') || line.startsWith('---')) continue;
    if (line.startsWith('+')) add++;
    else if (line.startsWith('-')) del++;
  }
  return { add, del };
}

/** One file's unified diff, lines colored by kind. */
export function PatchView({ patch }: { patch: FilePatch | undefined }) {
  if (!patch) return <p className="muted diff-note">이 파일의 diff를 불러오지 않았습니다.</p>;
  if (patch.binary) return <p className="muted diff-note">바이너리 파일이라 내용 비교를 보여 주지 않습니다.</p>;
  if (!patch.patch.trim()) return <p className="muted diff-note">내용 차이가 없습니다(이동·속성 변경).</p>;
  return (
    <pre className="patch">
      {patch.patch.split('\n').map((line, i) => {
        const kind = line.startsWith('@@') ? 'hunk' : line.startsWith('+++') || line.startsWith('---') ? 'head' : line.startsWith('+') ? 'add' : line.startsWith('-') ? 'del' : line.startsWith('\\') ? 'meta' : '';
        return (
          <span key={i} className={`pl ${kind}`}>
            {line || ' '}
            {'\n'}
          </span>
        );
      })}
    </pre>
  );
}

/** A dialog with a file list and the selected file's diff. */
export function DiffDialog({ title, files, patches, initial, note, onClose }: { title: string; files: DiffFile[]; patches: FilePatch[]; initial: string; note?: string; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [path, setPath] = useState(initial);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  const byPath = new Map(patches.map((p) => [p.path, p]));
  return (
    <dialog ref={dialog} className="diff-dialog" onClose={onClose} aria-labelledby="diff-title">
      <header className="diff-head">
        <h2 id="diff-title">{title}</h2>
        <button className="ghost" onClick={() => dialog.current?.close()}>
          닫기
        </button>
      </header>
      <div className="diff-body">
        <ul className="diff-files">
          {files.map((f) => {
            const p = byPath.get(f.path);
            const n = p && !p.binary ? countLines(p.patch) : null;
            return (
              <li key={f.path}>
                <button className={f.path === path ? 'df active' : 'df'} onClick={() => setPath(f.path)} title={f.path}>
                  <span className={`act ${f.action}`}>{ACTION_MARK[f.action] ?? '?'}</span>
                  <span className="df-path">{f.path}</span>
                  {n && (
                    <span className="df-n">
                      <span className="plus">+{n.add}</span> <span className="minus">−{n.del}</span>
                    </span>
                  )}
                  {p?.binary && <span className="df-n muted">bin</span>}
                </button>
              </li>
            );
          })}
        </ul>
        <div className="diff-pane">
          <p className="mono diff-path">{path}</p>
          {note ? <p className="muted diff-note">{note}</p> : <PatchView patch={byPath.get(path)} />}
        </div>
      </div>
    </dialog>
  );
}
