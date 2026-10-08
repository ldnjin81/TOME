/** Lore and TOME error text → what it means and what to do, in Korean. */
const RULES: [RegExp, string][] = [
  [/Branch has diverged|Branch history is divergent/i, '서버에 새 리비전이 있어 push할 수 없습니다. Smartlog의 내 스택에서 restack(위로 옮기기)하거나 병합해서 받은 뒤 다시 push하세요.'],
  [/advanced by another instance/i, '다른 곳(다른 TOME 창이나 명령줄)에서 이 브랜치를 먼저 바꿨습니다. 동기화한 뒤 다시 스테이징하고 커밋하세요.'],
  [/Local modifications prevent synchronization/i, '커밋하지 않은 변경이 있어 동기화하거나 브랜치를 바꿀 수 없습니다. 커밋하거나 변경을 되돌린 뒤 다시 하세요.'],
  [/resource locked by somebody else/i, '다른 사람이 잠근 파일입니다. 잠금 탭에서 누가 잠갔는지 확인하고, 풀릴 때까지 기다리거나 그 사람에게 연락하세요.'],
  [/lock does not exist/i, '이미 풀린 잠금입니다. 잠금 목록을 새로 고치세요.'],
  [/Not authorized/i, '이 저장소에 접근할 권한이 없습니다. 서버 로그인 상태와 설정의 신원을 확인하세요.'],
  [/Disconnected from server|Connection terminated|Reconnect to server failed|Not connected to remote|service unavailable|connection refused|ECONNREFUSED|timed out|No route to host|transport error|failed to connect/i, 'Lore 서버에 연결할 수 없습니다. 서버 주소와 네트워크를 확인하세요. 서버 없이 할 수 있는 작업은 "오프라인"을 켜고 계속할 수 있습니다.'],
  [/Repository not found/i, '서버에 그 저장소가 없습니다. 주소 끝의 저장소 이름을 확인하세요.'],
  [/Repository already exist/i, '그 폴더에는 이미 Lore 작업본이 있습니다. 다른 빈 폴더를 고르거나, 그 작업본을 여세요.'],
  [/Branch .+ already exists/i, '같은 이름의 브랜치가 이미 있습니다. 다른 이름을 쓰거나 그 브랜치로 전환하세요.'],
  [/Unable to commit when .+ is still in conflict|conflicts not settled yet/i, '아직 정하지 않은 충돌 파일이 있습니다. 파일마다 남길 쪽을 정한 뒤 다시 하세요.'],
  [/protected branch|Unable to delete default branch|Cannot delete the current branch/i, '이 브랜치는 지울 수 없습니다(보호된 브랜치, 기본 브랜치, 또는 지금 쓰는 브랜치).'],
  [/revision not found|branch not found/i, '리비전이나 브랜치를 찾을 수 없습니다. 오프라인이면 아직 서버에서 받지 않은 것일 수 있으니 동기화해 보세요.'],
  [/not a Lore working copy/i, 'Lore 작업본 폴더가 아닙니다. 올바른 폴더를 고르거나, 설정에서 서버에서 받기를 쓰세요.'],
  [/uncommitted changes: commit or revert them first/i, '커밋하지 않은 변경이 있어 시작할 수 없습니다. 커밋하거나 변경을 되돌린 뒤 다시 하세요.'],
  [/a merge is in progress/i, '병합이 진행 중입니다. 먼저 병합을 끝내거나 중단하세요.'],
  [/fold needs at least two revisions/i, '합치려면 커밋이 두 개 이상 필요합니다.'],
];

/** Raw text longer than this is cut in the shown original. */
const RAW_MAX = 160;

/**
 * The error as the user should read it: a Korean explanation with the original kept short
 * in parentheses, or the text itself when no rule knows it (TOME's own messages are Korean).
 * A prefix the app added (such as "push 실패: ") is kept in front.
 */
export function explainError(raw: string): string {
  const text = raw.trim();
  if (!text) return '';
  const rule = RULES.find(([pattern]) => pattern.test(text));
  if (!rule) return text;
  const prefix = /^([^:]{1,40} 실패): /.exec(text)?.[1];
  const original = text.length > RAW_MAX ? `${text.slice(0, RAW_MAX)}…` : text;
  return `${prefix ? `${prefix}: ` : ''}${rule[1]} (원문: ${original})`;
}
