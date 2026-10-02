import { useEffect, useRef, useState } from 'react';

/** A Lore branch name: no spaces, not empty (slashes group branches, e.g. auto/task-12). */
function nameProblem(name: string, taken: string[]) {
  if (!name) return '이름을 적으세요';
  if (/\s/.test(name)) return '공백은 쓸 수 없습니다';
  if (taken.includes(name)) return '같은 이름의 브랜치가 있습니다';
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
        <h2 id="nb-title">새 브랜치</h2>
        <p className="muted">{from}의 현재 리비전에서 시작합니다.</p>
        <label className="field">
          <span>이름</span>
          <input autoFocus className="mono" value={name} onChange={(e) => setName(e.target.value)} placeholder="feature/inventory-drag" spellCheck={false} />
        </label>
        {name && problem && <p className="error-line">{problem}</p>}
        <label className="check">
          <input type="checkbox" checked={switchTo} onChange={(e) => setSwitchTo(e.target.checked)} />
          만든 뒤 바로 전환
        </label>
        <div className="dialog-buttons">
          <button type="button" className="ghost" onClick={() => dialog.current?.close()}>
            취소
          </button>
          <button type="submit" className="primary" disabled={!!problem || busy}>
            만들기
          </button>
        </div>
      </form>
    </dialog>
  );
}

/** Merge another branch into the current one. */
export function MergeDialog({ from, into, busy, onMerge, onClose }: { from: string; into: string; busy: boolean; onMerge: (message: string) => void; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [message, setMessage] = useState(`Merge ${from} into ${into}`);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
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
          <span className="mono">{from}</span> → <span className="mono">{into}</span> 병합
        </h2>
        <p className="muted">충돌이 없으면 바로 병합 커밋을 만듭니다. 충돌이 있으면 변경 탭에서 파일마다 정한 뒤 커밋합니다.</p>
        <label className="field">
          <span>커밋 메시지</span>
          <input autoFocus value={message} onChange={(e) => setMessage(e.target.value)} />
        </label>
        <div className="dialog-buttons">
          <button type="button" className="ghost" onClick={() => dialog.current?.close()}>
            취소
          </button>
          <button type="submit" className="primary" disabled={!message.trim() || busy}>
            병합
          </button>
        </div>
      </form>
    </dialog>
  );
}
