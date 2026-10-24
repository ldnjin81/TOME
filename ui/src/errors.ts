import { t } from './i18n';

const LOGIN_ERROR = /\bNotAuthenticated\b|\bTokenNotFound\b|authorization header required|Not authenticated|No token stored/i;
/** 로그인 버튼 표시에도 같은 판정을 사용한다. 권한 부족은 재로그인으로 처리하지 않는다. */
export function needsLogin(raw: string): boolean {
  return LOGIN_ERROR.test(raw);
}

/** Lore and TOME error text → what it means and what to do, in Korean. */
const RULES: [RegExp, () => string][] = [
  [/Login already in progress/i, () => t('이전 로그인 요청이 끝나기를 기다리는 중…')],
  [/expired_token|Login session expired|device.*expired|authorization.*expired/i, () => t('승인 시간이 지났습니다. 다시 로그인하세요.')],
  [/access_denied|authorization_declined/i, () => t('로그인 요청이 거부되었습니다. 다시 시도하세요.')],
  [LOGIN_ERROR, () => t("로그인이 필요하거나 만료되었습니다")],
  [/Branch has diverged|Branch history is divergent/i, () => t("서버에 새 리비전이 있어 push할 수 없습니다. Smartlog의 내 스택에서 restack(위로 옮기기)하거나 병합해서 받은 뒤 다시 push하세요.")],
  [/advanced by another instance/i, () => t("다른 곳(다른 TOME 창이나 명령줄)에서 이 브랜치를 먼저 바꿨습니다. 동기화한 뒤 다시 스테이징하고 커밋하세요.")],
  [/Local modifications prevent synchronization/i, () => t("커밋하지 않은 변경이 있어 동기화하거나 브랜치를 바꿀 수 없습니다. 커밋하거나 변경을 되돌린 뒤 다시 하세요.")],
  [/resource locked by somebody else/i, () => t("다른 사람이 잠근 파일입니다. 잠금 탭에서 누가 잠갔는지 확인하고, 풀릴 때까지 기다리거나 그 사람에게 연락하세요.")],
  [/lock does not exist/i, () => t("이미 풀린 잠금입니다. 잠금 목록을 새로 고치세요.")],
  [/\bNotAuthorized\b|Not authorized/i, () => t("권한 없음. 이 저장소에 접근할 권한이 없습니다. 서버 관리자에게 문의하세요.")],
  [/Disconnected from server|Connection terminated|Reconnect to server failed|Not connected to remote|service unavailable|connection refused|ECONNREFUSED|timed out|No route to host|transport error|failed to connect/i, () => t("Lore 서버에 연결할 수 없습니다. 서버 주소와 네트워크를 확인하세요. 서버 없이 할 수 있는 작업은 \"오프라인\"을 켜고 계속할 수 있습니다.")],
  [/Repository not found/i, () => t("서버에 그 저장소가 없습니다. 주소 끝의 저장소 이름을 확인하세요.")],
  [/Repository already exist/i, () => t("그 폴더에는 이미 Lore 작업본이 있습니다. 다른 빈 폴더를 고르거나, 그 작업본을 여세요.")],
  [/Branch .+ already exists/i, () => t("같은 이름의 브랜치가 이미 있습니다. 다른 이름을 쓰거나 그 브랜치로 전환하세요.")],
  [/Unable to commit when .+ is still in conflict|conflicts not settled yet/i, () => t("아직 정하지 않은 충돌 파일이 있습니다. 파일마다 남길 쪽을 정한 뒤 다시 하세요.")],
  [/protected branch|Unable to delete default branch|Cannot delete the current branch/i, () => t("이 브랜치는 지울 수 없습니다(보호된 브랜치, 기본 브랜치, 또는 지금 쓰는 브랜치).")],
  [/revision not found|branch not found/i, () => t("리비전이나 브랜치를 찾을 수 없습니다. 오프라인이면 아직 서버에서 받지 않은 것일 수 있으니 동기화해 보세요.")],
  [/not a Lore working copy/i, () => t("Lore 작업본 폴더가 아닙니다. 올바른 폴더를 고르거나, 설정에서 서버에서 받기를 쓰세요.")],
  [/uncommitted changes: commit or revert them first/i, () => t("커밋하지 않은 변경이 있어 시작할 수 없습니다. 커밋하거나 변경을 되돌린 뒤 다시 하세요.")],
  [/a merge is in progress/i, () => t("병합이 진행 중입니다. 먼저 병합을 끝내거나 중단하세요.")],
  [/fold needs at least two revisions/i, () => t("합치려면 커밋이 두 개 이상 필요합니다.")],
  [/^Folder is not empty: /i, () => t('폴더가 비어 있지 않습니다: {p0}', { p0: '' })],
  [/^Could not start job process: /i, () => t('작업 프로세스를 시작할 수 없습니다: {p0}', { p0: '' })],
  [/^Cancelled$/i, () => t('취소함')],
  [/^Job process ended without a result$/i, () => t('작업 프로세스가 결과 없이 끝났습니다')],
  [/^Job already finished$/i, () => t('이미 끝난 작업입니다')],
  [/^Local modifications prevent switching branches$/i, () => t('커밋하지 않은 변경이 있어 전환할 수 없습니다. 커밋하거나 변경을 되돌린 뒤 다시 하세요.')],
  [/^No restack in progress$/i, () => t('진행 중인 restack이 없습니다')],
  [/^\.tome\/tools\.json was not found$/i, () => t('.tome/tools.json이 없습니다')],
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
  const prefix = /^([^:]{1,40} (?:실패|failed)): /i.exec(text)?.[1];
  const original = text.length > RAW_MAX ? `${text.slice(0, RAW_MAX)}…` : text;
  return `${prefix ? `${prefix}: ` : ''}${rule[1]()} (${t('원문: {original}', { original })})`;
}
