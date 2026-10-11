import { t, tn } from './i18n';
import { useEffect, useRef, useState } from 'react';
import type { ViewChange } from './types';

export const PRESETS: { name: string; lines: string[] }[] = [
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
      <h2 id="view-title">{t("View — 받을 범위")}</h2>
      <p className="muted">
        {tn("한 줄에 글롭 하나. 그냥 쓰면 {p0}, {p1}로 시작하면 다시 {p2}합니다. 비우면 전부 받습니다.", { p0: <b>{t("제외")}</b>, p1: <code>!</code>, p2: <b>{t("포함")}</b> })}
      </p>
      <div className="presets" role="group" aria-label={t("프리셋")}>
        {PRESETS.map((p) => (
          <button key={p.name} className="ghost" onClick={() => setText(p.lines.join('\n'))} disabled={busy}>{t(p.name)}</button>
        ))}
      </div>
      <label className="visually-hidden" htmlFor="view-lines">.lore/view</label>
      <textarea id="view-lines" className="mono" rows={9} value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} placeholder={'/*\n!/Source/'} />
      <p className="muted note">{t("View 밖으로 나간 파일 중 변경 없는 파일은 디스크에서 지우고, 변경했거나 새로 만든 파일은 남겨 둡니다. 다시 들어온 파일은 현재 리비전에서 받아 옵니다.")}</p>

      {result && (
        <div className="view-result" role="status">
          {t("지움 {p0} · 받아 옴 {p1} · 남겨 둠 {p2}", { p0: result.removed.length, p1: result.restored.length, p2: result.kept.length })}
          {result.kept.length > 0 && (
            <ul className="mono">
              {result.kept.slice(0, 20).map((p) => <li key={p}>{p}</li>)}
              {result.kept.length > 20 && <li className="muted">{t("… 외 {p0}개", { p0: result.kept.length - 20 })}</li>}
            </ul>
          )}
        </div>
      )}

      <div className="dialog-buttons">
        <button className="ghost" onClick={() => dialog.current?.close()}>{t("닫기")}</button>
        <button className="primary" onClick={() => onApply(lines)} disabled={busy}>{busy ? t("적용 중…") : t("적용")}</button>
      </div>
    </dialog>
  );
}
