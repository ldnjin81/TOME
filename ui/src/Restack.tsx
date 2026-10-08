import { useEffect, useRef, useState } from 'react';
import { explainError } from './errors';
import { invoke } from '@tauri-apps/api/core';
import type { Keep, PendingRestack, Pick, RestackPlan, RestackPreview, Status } from './types';
import type { RestackRequest } from './Smartlog';
import type { FoldRequest } from './stackLogic';
import { movePick } from './restackLogic';

const KEEP_LABEL: Record<Keep, string> = { mine: '내 변경 유지', base: '베이스 버전 사용', edited: '직접 고침' };

interface DialogProps {
  path: string;
  request: RestackRequest;
  /** The base the stack sits on now, and its order (oldest first), for the preview. */
  oldBase: string;
  oldOrder: string[];
  originalHead: string;
  busy: boolean;
  /** Runs the plan; `choices` settle binary conflicts on these paths without asking again. */
  onRun: (plan: RestackPlan, choices: Record<string, Keep>) => void;
  onClose: () => void;
}

/** What a restack will do before anything changes: the order (editable), and for each revision
 * the files that may conflict and others' locks. Binary files can be settled up front. */
export function RestackDialog({ path, request, oldBase, oldOrder, originalHead, busy, onRun, onClose }: DialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [picks, setPicks] = useState<Pick[]>(request.picks);
  const [preview, setPreview] = useState<RestackPreview | null>(null);
  const [error, setError] = useState('');
  const [choices, setChoices] = useState<Record<string, Keep>>({});
  const plan: RestackPlan = { onto: request.onto, picks, original_head: originalHead };
  const reordering = request.onto === oldBase;

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  useEffect(() => {
    let stale = false;
    setPreview(null);
    setError('');
    invoke<RestackPreview>('restack_preview', { path, plan, oldBase, oldOrder }).then(
      (p) => !stale && setPreview(p),
      (e) => !stale && setError(String(e)),
    );
    return () => {
      stale = true;
    };
    // The plan changes only with the order.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [picks]);

  // Shown newest first, like the Smartlog; `picks` is oldest first.
  const shown = [...picks].reverse();
  function move(id: string, by: -1 | 1) {
    const next = movePick(picks, id, by);
    if (next !== picks) setPicks(next);
  }

  const byId = new Map(preview?.picks.map((p) => [p.id, p]) ?? []);
  const binary = preview?.picks.flatMap((p) => p.risks.filter((r) => r.binary)) ?? [];
  const textOnly = preview?.picks.filter((p) => !p.risks.some((r) => r.binary)).length ?? 0;
  const locked = preview?.picks.reduce((n, p) => n + p.locked.length, 0) ?? 0;
  const unchanged = picks.every((p, i) => p.id === oldOrder[i]) && reordering;

  return (
    <dialog ref={dialog} className="restack-dialog" onClose={onClose} aria-labelledby="rs-title">
      <h2 id="rs-title">{reordering ? `스택 순서 바꾸기 · ${picks.length}개` : `커밋 ${picks.length}개를 ${request.ontoLabel} 위로 옮깁니다`}</h2>
      <p className="muted small">미리보기입니다. 아직 아무것도 바뀌지 않았습니다. 각 커밋을 새 순서대로 다시 적용하고, push 전까지는 내 작업본에만 있습니다. 새 베이스에 이미 들어 있는 변경은 빈 커밋으로 남기지 않고 뺍니다. 중간에 오류가 나면 시작 전 상태로 되돌립니다.</p>

      <ol className="rs-list">
        {shown.map((p, i) => {
          const info = byId.get(p.id);
          return (
            <li key={p.id} className={info?.risks.some((r) => r.binary) ? 'rs-item warn' : 'rs-item'}>
              <span className="rs-order">
                <button className="ghost tiny" onClick={() => move(p.id, -1)} disabled={i === 0} aria-label={`${p.message} 위로 (나중에 적용)`}>
                  ▲
                </button>
                <button className="ghost tiny" onClick={() => move(p.id, 1)} disabled={i === shown.length - 1} aria-label={`${p.message} 아래로 (먼저 적용)`}>
                  ▼
                </button>
              </span>
              <span className="rs-body">
                <span className="rs-title">
                  <span className="rev-no draft">draft</span> {p.message.split('\n')[0] || '(메시지 없음)'}
                  <span className="muted small"> · 파일 {info ? info.files.length : '…'}개</span>
                </span>
                {info?.merge && <span className="rs-note bin">병합 리비전입니다. 옮기면 일반 리비전 하나가 되어 병합 기록(어느 브랜치를 합쳤는지)은 사라집니다.</span>}
                {info && !info.merge && info.risks.length === 0 && info.locked.length === 0 && <span className="rs-note ok">겹치는 변경 없음</span>}
                {info?.risks.map((r) =>
                  r.binary ? (
                    <span key={r.path} className="rs-note bin">
                      <span className="mono">{r.path}</span> · 바이너리라 자동 병합 불가 — 충돌하면:
                      <span className="rs-choice" role="radiogroup" aria-label={`${r.path} 충돌 시`}>
                        {(['mine', 'base'] as Keep[]).map((k) => (
                          <button key={k} role="radio" aria-checked={choices[r.path] === k} className={choices[r.path] === k ? 'chip active' : 'chip'} onClick={() => setChoices((c) => ({ ...c, [r.path]: k }))}>
                            {KEEP_LABEL[k]}
                          </button>
                        ))}
                        <button role="radio" aria-checked={!choices[r.path]} className={!choices[r.path] ? 'chip active' : 'chip'} onClick={() => setChoices((c) => Object.fromEntries(Object.entries(c).filter(([path]) => path !== r.path)))}>
                          그때 묻기
                        </button>
                      </span>
                    </span>
                  ) : (
                    <span key={r.path} className="rs-note text">
                      <span className="mono">{r.path}</span> · 텍스트 — 자동 병합을 시도합니다
                    </span>
                  ),
                )}
                {info?.locked.map(([file, owner]) => (
                  <span key={file} className="rs-note lock">
                    🔒 <span className="mono">{file}</span> · {owner}님이 잠금
                  </span>
                ))}
              </span>
            </li>
          );
        })}
      </ol>
      <p className="rs-base">
        <span className="rev-no">{reordering ? '베이스' : '새 베이스'}</span> {request.ontoLabel}
        {preview && !reordering && <span className="muted small"> · 베이스 사이에 바뀐 파일 {preview.base_changes.length}개</span>}
      </p>

      {error && <p className="error-line">{explainError(error)}</p>}
      <div className="rs-tiles" aria-live="polite">
        <span className="rs-tile ok">
          <strong>{preview ? textOnly : '…'}</strong> 자동 적용 예상
        </span>
        <span className="rs-tile bin">
          <strong>{preview ? binary.length : '…'}</strong> 바이너리 충돌 가능
        </span>
        <span className={locked ? 'rs-tile lock' : 'rs-tile'}>
          <strong>{preview ? locked : '…'}</strong> 다른 사람 잠금
        </span>
      </div>
      <div className="dialog-buttons">
        <button type="button" className="ghost" onClick={() => dialog.current?.close()}>
          취소
        </button>
        <button type="button" className="primary" disabled={busy || !preview || unchanged} onClick={() => onRun(plan, choices)} title={unchanged ? '순서가 그대로입니다' : undefined}>
          Restack 실행
        </button>
      </div>
    </dialog>
  );
}

interface PanelProps {
  pending: PendingRestack;
  status: Status;
  busy: boolean;
  onResolve: (paths: string[], keep: Keep) => void;
  onContinue: () => void;
  onAbort: () => void;
}

/** A restack stopped on a conflict: settle each file, then continue, or put everything back. */
export function RestackPanel({ pending, status, busy, onResolve, onContinue, onAbort }: PanelProps) {
  const pick = pending.plan.picks[pending.index];
  const files = pending.files.map((path) => ({ path, file: status.files.find((f) => f.path === path) }));
  const open = files.filter((f) => f.file?.unresolved ?? true).length;
  return (
    <section className="conflicts restack-panel" aria-label="restack 충돌">
      <h3>
        Restack 중 · {pending.index + 1}/{pending.plan.picks.length} <span className="rs-pick">“{pick?.message.split('\n')[0]}”</span> 적용에서 충돌
        <span className="tag conflict-tag">미해결 {open}</span>
      </h3>
      <p className="muted small">내 변경과 새 베이스가 같은 파일을 바꿨습니다. 파일마다 남길 쪽을 정한 뒤 계속하세요. 텍스트 파일은 작업본에서 직접 고친 뒤 “직접 고침”을 누를 수도 있습니다.</p>
      {files.map(({ path, file }) => (
        <div key={path} className="conflict-row">
          <span className="file-path mono" title={path}>
            {path}
          </span>
          {file && !file.unresolved ? (
            <span className="muted small">{file.resolution === 'theirs' ? KEEP_LABEL.mine : file.resolution === 'mine' ? KEEP_LABEL.base : '정함'}</span>
          ) : (
            <span className="conflict-actions">
              <button className="ghost small-btn" disabled={busy} onClick={() => onResolve([path], 'mine')}>
                {KEEP_LABEL.mine}
              </button>
              <button className="ghost small-btn" disabled={busy} onClick={() => onResolve([path], 'base')}>
                {KEEP_LABEL.base}
              </button>
              <button className="ghost small-btn" disabled={busy} onClick={() => onResolve([path], 'edited')} title="작업본에서 고친 내용을 그대로 씁니다">
                {KEEP_LABEL.edited}
              </button>
            </span>
          )}
        </div>
      ))}
      <div className="dialog-buttons">
        <button className="ghost" disabled={busy} onClick={onAbort} title="restack 전으로 브랜치와 파일을 되돌립니다">
          중단하고 되돌리기
        </button>
        <button className="primary" disabled={busy || open > 0} onClick={onContinue}>
          계속
        </button>
      </div>
    </section>
  );
}

interface FoldProps {
  request: FoldRequest;
  busy: boolean;
  onFold: (message: string) => void;
  onClose: () => void;
}

/** Fold two drafts into one: shows what is folded and lets the message be edited. */
export function FoldDialog({ request, busy, onFold, onClose }: FoldProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [message, setMessage] = useState(request.message);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog ref={dialog} className="small-dialog" onClose={onClose} aria-labelledby="fold-title">
      <form
        method="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          if (message.trim()) onFold(message.trim());
        }}
      >
        <h2 id="fold-title">커밋 {request.group.length}개를 하나로 합칩니다</h2>
        <ol className="fold-list">
          {request.messages.map((m, i) => (
            <li key={i}>{m.split('\n')[0] || '(메시지 없음)'}</li>
          ))}
        </ol>
        <p className="muted small">
          두 커밋의 변경을 합친 커밋 하나가 됩니다. push 전까지는 내 작업본에만 있습니다.
          {request.rest.length > 0 && ` 위에 있는 커밋 ${request.rest.length}개는 합친 커밋 위에 다시 얹습니다.`}
        </p>
        <label className="field">
          <span>합친 커밋 메시지</span>
          <textarea className="fold-msg" rows={4} value={message} onChange={(e) => setMessage(e.target.value)} />
        </label>
        <div className="dialog-buttons">
          <button type="button" className="ghost" onClick={() => dialog.current?.close()}>
            취소
          </button>
          <button type="submit" className="primary" disabled={busy || !message.trim()}>
            합치기
          </button>
        </div>
      </form>
    </dialog>
  );
}
