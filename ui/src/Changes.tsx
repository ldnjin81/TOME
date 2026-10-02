import { useState } from 'react';
import type { ChangedFile, Status } from './types';

const ACTION_LABEL: Record<string, string> = { add: '추가', modify: '수정', delete: '삭제', move: '이동' };

interface Props {
  status: Status;
  busy: boolean;
  onRefresh: () => void;
  onStage: (paths: string[], stage: boolean) => void;
  onCommit: (message: string, push: boolean) => Promise<boolean>;
  onPush: () => void;
  /** Right-click on a file: the custom tools menu. */
  onContext: (e: React.MouseEvent, files: string[]) => void;
}

function FileRow({ file, onToggle, busy, onContext }: { file: ChangedFile; onToggle: () => void; busy: boolean; onContext: (e: React.MouseEvent, files: string[]) => void }) {
  return (
    <li>
      <button className="file-row" onClick={onToggle} onContextMenu={(e) => onContext(e, [file.path])} disabled={busy} title={file.staged ? '스테이징 해제' : '스테이징'}>
        <span className={`action ${file.action}`}>{ACTION_LABEL[file.action] ?? file.action}</span>
        <span className="file-path mono">{file.path}</span>
        {file.conflict && <span className="badge conflict">충돌</span>}
        <span className="move" aria-hidden="true">{file.staged ? '−' : '+'}</span>
      </button>
    </li>
  );
}

/** The working copy's changes: stage, commit, push. */
export default function Changes({ status, busy, onRefresh, onStage, onCommit, onPush, onContext }: Props) {
  const [message, setMessage] = useState('');
  const files = status.files.filter((f) => !f.directory);
  const staged = files.filter((f) => f.staged);
  const unstaged = files.filter((f) => !f.staged);
  const canCommit = !busy && staged.length > 0 && message.trim().length > 0;

  async function commit(push: boolean) {
    if (await onCommit(message.trim(), push)) setMessage('');
  }

  return (
    <div className="changes">
      <div className="pane-actions">
        <button className="ghost" onClick={onRefresh} disabled={busy}>다시 검사</button>
        {status.local_ahead && (
          <button className="ghost" onClick={onPush} disabled={busy}>push</button>
        )}
      </div>

      <section>
        <h3>
          스테이징됨 <span className="muted">{staged.length}</span>
          {staged.length > 0 && (
            <button className="link" onClick={() => onStage(staged.map((f) => f.path), false)} disabled={busy}>모두 해제</button>
          )}
        </h3>
        <ul className="file-list">
          {staged.map((f) => <FileRow key={f.path} file={f} busy={busy} onContext={onContext} onToggle={() => onStage([f.path], false)} />)}
        </ul>
      </section>

      <section>
        <h3>
          변경 <span className="muted">{unstaged.length}</span>
          {unstaged.length > 0 && (
            <button className="link" onClick={() => onStage(unstaged.map((f) => f.path), true)} disabled={busy}>모두 스테이징</button>
          )}
        </h3>
        {unstaged.length === 0 && staged.length === 0 ? (
          <p className="muted">변경 없음</p>
        ) : (
          <ul className="file-list">
            {unstaged.map((f) => <FileRow key={f.path} file={f} busy={busy} onContext={onContext} onToggle={() => onStage([f.path], true)} />)}
          </ul>
        )}
      </section>

      <form
        className="commit-box"
        onSubmit={(e) => {
          e.preventDefault();
          if (canCommit) void commit(false);
        }}
      >
        <label htmlFor="commit-message">커밋 메시지</label>
        <textarea id="commit-message" value={message} onChange={(e) => setMessage(e.target.value)} rows={3} placeholder="무엇을 바꿨나요" />
        <div className="commit-buttons">
          <button type="submit" className="primary" disabled={!canCommit}>커밋</button>
          <button type="button" className="primary outline" disabled={!canCommit} onClick={() => void commit(true)}>커밋 후 push</button>
        </div>
      </form>
    </div>
  );
}
