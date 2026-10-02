import { useEffect, useRef, useState } from 'react';
import type { ViewChange } from './types';

const PRESETS: { name: string; lines: string[] }[] = [
  { name: '전체', lines: [] },
  { name: '코드만', lines: ['/*', '!/Source/', '!/Config/', '!/Plugins/', '!/Docs/', '!/*.uproject'] },
  { name: '콘텐츠 제외', lines: ['/Content/'] },
];

interface Props {
  initial: string[];
  busy: boolean;
  result: ViewChange | null;
  onApply: (lines: string[]) => void;
  onClose: () => void;
}

/** Edits `.lore/view`: which paths are materialized in this working copy. */
export default function ViewDialog({ initial, busy, result, onApply, onClose }: Props) {
  const [text, setText] = useState(initial.join('\n'));
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean);

  return (
    <dialog ref={dialog} className="view-dialog" onClose={onClose} aria-labelledby="view-title">
      <h2 id="view-title">View — 받을 범위</h2>
      <p className="muted">
        한 줄에 글롭 하나. 그냥 쓰면 <b>제외</b>, <code>!</code>로 시작하면 다시 <b>포함</b>합니다. 비우면 전부 받습니다.
      </p>
      <div className="presets" role="group" aria-label="프리셋">
        {PRESETS.map((p) => (
          <button key={p.name} className="ghost" onClick={() => setText(p.lines.join('\n'))} disabled={busy}>{p.name}</button>
        ))}
      </div>
      <label className="visually-hidden" htmlFor="view-lines">.lore/view</label>
      <textarea id="view-lines" className="mono" rows={9} value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} placeholder={'/*\n!/Source/'} />
      <p className="muted note">View 밖으로 나간 파일 중 변경 없는 파일은 디스크에서 지우고, 변경했거나 새로 만든 파일은 남겨 둡니다. 다시 들어온 파일은 현재 리비전에서 받아 옵니다.</p>

      {result && (
        <div className="view-result" role="status">
          지움 {result.removed.length} · 받아 옴 {result.restored.length} · 남겨 둠 {result.kept.length}
          {result.kept.length > 0 && (
            <ul className="mono">
              {result.kept.slice(0, 20).map((p) => <li key={p}>{p}</li>)}
              {result.kept.length > 20 && <li className="muted">… 외 {result.kept.length - 20}개</li>}
            </ul>
          )}
        </div>
      )}

      <div className="dialog-buttons">
        <button className="ghost" onClick={() => dialog.current?.close()}>닫기</button>
        <button className="primary" onClick={() => onApply(lines)} disabled={busy}>{busy ? '적용 중…' : '적용'}</button>
      </div>
    </dialog>
  );
}
