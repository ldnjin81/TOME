import { t } from './i18n';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { explainError } from './errors';
import type { MergePreview } from './types';

/** How many preview rows to show before summarising the rest. */
export const PREVIEW_ROWS = 200;

const ACTIONS: Record<string, string> = { add: '추가', delete: '삭제', change: '변경@@상태' };

/** A Lore branch name: no spaces, not empty (slashes group branches, e.g. auto/task-12). */
function nameProblem(name: string, taken: string[]) {
  if (!name) return t("이름을 적으세요");
  if (/\s/.test(name)) return t("공백은 쓸 수 없습니다");
  if (taken.includes(name)) return t("같은 이름의 브랜치가 있습니다");
  return '';
}

/** New branch at the current revision. */
export function NewBranchDialog({ from, taken, busy, onCreate, onClose }: { from: string; taken: string[]; busy: boolean; onCreate: (name: string, switchTo: boolean) => void; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState('');
  const [switchTo, setSwitchTo] = useState(true);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  const problem = nameProblem(name.trim(), taken);
  return (
    <dialog ref={dialog} className="small-dialog" onClose={onClose} aria-labelledby="nb-title">
      <form
        method="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          if (!problem) onCreate(name.trim(), switchTo);
        }}
      >
        <h2 id="nb-title">{t("새 브랜치")}</h2>
        <p className="muted">{t("{p0}의 현재 리비전에서 시작합니다.", { p0: from })}</p>
        <label className="field">
          <span>{t("이름")}</span>
          <input autoFocus className="mono" value={name} onChange={(e) => setName(e.target.value)} placeholder="feature/inventory-drag" spellCheck={false} />
        </label>
        {name && problem && <p className="error-line">{problem}</p>}
        <label className="check">
          <input type="checkbox" checked={switchTo} onChange={(e) => setSwitchTo(e.target.checked)} />
          {t("만든 뒤 바로 전환")}
        </label>
        <div className="dialog-buttons">
          <button type="button" className="ghost" onClick={() => dialog.current?.close()}>
            {t("취소")}
          </button>
          <button type="submit" className="primary" disabled={!!problem || busy}>
            {t("만들기")}
          </button>
        </div>
      </form>
    </dialog>
  );
}

/** Merge another branch into the current one. */
export function MergeDialog({ path, from, into, busy, onMerge, onClose }: { path: string; from: string; into: string; busy: boolean; onMerge: (message: string) => void; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [message, setMessage] = useState(`Merge ${from} into ${into}`);
  const [preview, setPreview] = useState<MergePreview | null>(null);
  const [previewError, setPreviewError] = useState('');
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  // Lore 0.12의 dry run은 작업본·서버를 바꾸지 않으므로 대화상자를 열 때 바로 미리 본다.
  useEffect(() => {
    let live = true;
    invoke<MergePreview>('merge_preview', { path, from }).then(
      (value) => { if (live) setPreview(value); },
      (e) => { if (live) setPreviewError(String(e)); },
    );
    return () => { live = false; };
  }, [path, from]);
  return (
    <dialog ref={dialog} className="small-dialog" onClose={onClose} aria-labelledby="mg-title">
      <form
        method="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          if (message.trim()) onMerge(message.trim());
        }}
      >
        <h2 id="mg-title">
          <span className="mono">{from}</span> → <span className="mono">{into}</span> {t("병합")}
        </h2>
        <p className="muted">{t("충돌이 없으면 바로 병합 커밋을 만듭니다. 충돌이 있으면 변경 탭에서 파일마다 정한 뒤 커밋합니다.")}</p>
        <section className="merge-preview" aria-live="polite">
          {!preview && !previewError && <p className="muted small">{t("병합 결과를 미리 보는 중…")}</p>}
          {previewError && <p className="muted small">{t("미리보기를 만들지 못했습니다: {detail}", { detail: explainError(previewError) })}</p>}
          {preview && (preview.files.length === 0
            ? <p className="small">{t("들어올 변경이 없습니다.")}</p>
            : <>
              <p className={preview.conflicts ? 'warn-line' : 'small'}>
                {preview.conflicts
                  ? t("파일 {count}개가 바뀌고, 그중 {conflicts}개는 충돌합니다.", { count: preview.files.length, conflicts: preview.conflicts })
                  : t("파일 {count}개가 바뀝니다. 충돌은 없습니다.", { count: preview.files.length })}
              </p>
              <ul className="merge-files">
                {preview.files.slice(0, PREVIEW_ROWS).map((file) => (
                  <li key={file.path} className={file.conflict ? 'conflict' : ''}>
                    <span className="merge-action">{file.conflict ? t("충돌") : t(ACTIONS[file.action] ?? '변경@@상태')}</span>
                    <span className="mono">{file.path}</span>
                  </li>
                ))}
              </ul>
              {preview.files.length > PREVIEW_ROWS && <p className="muted small">{t("외 {count}개", { count: preview.files.length - PREVIEW_ROWS })}</p>}
            </>)}
        </section>
        <label className="field">
          <span>{t("커밋 메시지")}</span>
          <input autoFocus value={message} onChange={(e) => setMessage(e.target.value)} />
        </label>
        <div className="dialog-buttons">
          <button type="button" className="ghost" onClick={() => dialog.current?.close()}>
            {t("취소")}
          </button>
          <button type="submit" className="primary" disabled={!message.trim() || busy}>
            {t("병합")}
          </button>
        </div>
      </form>
    </dialog>
  );
}
