# TOME 구조와 기술 스택

상태: 결정 초안(2026-10-02). 근거는 Lore 저장소 `v0.10.0`의 `lore-capi/lore.h`(12,833행)를 직접 읽어 확인했다.

## 1. 결정: Tauri 2 + Rust 코어 + TypeScript(React) UI

```
┌──────────────────────── TOME (하나의 실행 파일) ────────────────────────┐
│  UI (WebView: Windows WebView2 / macOS WKWebView)                        │
│    React + TypeScript · 가상화 리스트/트리 · Canvas 그래프 레인          │
│                 ▲ 화면 단위 조회(창 크기만큼)   │ 명령                     │
│                 │ 이벤트 스트림(Tauri Channel)   ▼                        │
│  tome-core (Rust)                                                        │
│    명령 큐(워커) · 상태 캐시(저장소별) · 그래프 레인 배치 · 진행/취소     │
│                 ▲ lore_event_t 콜백               │ lore_*_async 호출     │
│  lore (Epic, MIT) — C API, 같은 프로세스에 정적 링크                      │
└──────────────────────────────────────────────────────────────────────────┘
```

### 기준별 비교

| 기준 | Tauri 2 + Rust | Qt 6 + C++ | egui (Rust) | UE 에디터 플러그인 |
|---|---|---|---|---|
| Lore in-process 바인딩 | **최상**: Lore가 Rust라 같은 툴체인에서 C API(`lore-capi`)를 정적 링크 | 좋음: `lore.dll` + `lore.h` | 최상(같음) | 좋음(C API) |
| 대용량 트리·리스트 가상화 | 좋음: 검증된 가상 리스트, 데이터는 Rust에 두고 보이는 만큼만 전달 | 최상(`QAbstractItemModel`) | 좋음(`show_rows`) | 보통 |
| 커스텀 그래프 레인 | 좋음: Canvas 2D, 배치는 Rust | 최상(`QPainter`) | 좋음 | 보통(Slate) |
| 아티스트 모드(썸네일 그리드, 드래그) | **최상**: 웹 레이아웃·이미지 | 좋음 | 보통 | 좋음 |
| 와이어프레임 → 실제 화면 | **최상**: 와이어프레임이 HTML이라 구조를 그대로 옮김 | 다시 그려야 함 | 다시 그려야 함 | 다시 그려야 함 |
| Windows 우선 + macOS | 좋음(시스템 WebView, 설치 파일 ~10MB) | 좋음(Qt 런타임 동봉) | 좋음 | UE 안에서만 |
| 한국어 입력(IME)·접근성 | 최상(브라우저 엔진) | 최상 | 약함 | 좋음 |
| 빌드·배포 부담 | 보통(Rust + Node) | 높음(Qt 빌드, LGPL 동적 링크) | 낮음 | UE 버전마다 |

**고른 이유**
1. Lore가 Rust로 쓰여 있어 Rust 코어는 C API를 **같은 툴체인에서 정적 링크**한다. 별도 DLL 배포·ABI 문제가 없다.
2. 데스크톱 클라이언트에서 가장 손이 많이 가는 것은 화면(스택, 그리드, 다이얼로그, 미리보기)이다. 웹 UI가 이 반복을 가장 빠르게 한다. 와이어프레임 4장이 이미 HTML이다.
3. 대용량 데이터는 Rust 쪽 상태 캐시에 두고, UI는 "보이는 범위"만 요청한다. 그래서 WebView 경계가 병목이 되지 않는다.

**탈락 이유**
- Qt 6: 그래프·트리에는 가장 강하지만 아티스트 모드와 UI 반복이 느리고, C++ 빌드와 Qt 라이선스(LGPL 동적 링크) 관리가 개인 프로젝트에 무겁다.
- egui: DrTableSystem GUI로 경험은 있지만 썸네일 그리드·드래그 restack·IME 품질이 데스크톱 VCS 클라이언트에 부족하다.
- UE 에디터 플러그인: 2차 셸로 남긴다. `tome-core`의 명령·상태 모델을 그대로 쓰는 `ISourceControlProvider` 구현을 나중에 붙일 수 있다.

참고로 LoreGUI도 Tauri + Rust다. 차별화는 기술 스택이 아니라 UX(Smartlog 스택, 잠금 보드, View 관리자, uasset 비교)에서 낸다.

## 2. Lore 연동 방식

- **C API가 계약이다.** `lore-capi`는 Rust 크레이트 `lore`를 감싼 공개 C 인터페이스이고, 모든 함수가 `lore_*_async(globals, args, callback)` 형태다. 결과는 `lore_event_t` 스트림으로 온다(진행, 파일 하나씩, `COMPLETE`, `END`).
- Rust 크레이트 `lore`를 직접 쓰지 않는 이유: `publish = false`인 내부 크레이트라 API가 예고 없이 바뀔 수 있다. C API는 cbindgen으로 생성되는 문서화된 경계다.
- 의존 방식: Lore 저장소를 git 태그(`v0.10.x`)로 고정하고 `lore-capi`를 staticlib로 빌드해 링크한다. 바인딩은 `bindgen`으로 `lore.h`에서 생성한다. Lore 버전을 올리는 일은 태그 하나를 바꾸고 바인딩을 다시 만드는 일이다.

## 3. 코어 레이어(`tome-core`)

| 구성 | 역할 |
|---|---|
| `lore-sys` | `lore.h` 바인딩(bindgen), 안전하지 않은 호출을 한곳에 가둔다 |
| `session` | 저장소 하나의 핸들. 전역 인자(경로, 원격, 사용자)를 들고 명령을 낸다 |
| 명령 큐 | 저장소마다 직렬 워커 하나(쓰기 명령은 순서 보장), 읽기 명령은 병렬. 모든 명령은 취소 토큰과 진행 이벤트를 가진다 |
| 상태 캐시 | 브랜치·리비전 DAG·작업 트리 상태·잠금을 메모리에 둔다. 알림(`lore_notification_subscribe`)과 명령 결과로 갱신하고, UI에는 변경분만 보낸다 |
| 그래프 배치 | 리비전 DAG의 레인 배치를 Rust에서 계산한다(부모 최대 2개라 단순). 레인 색은 브랜치 **ID** 기준 |
| 명령 기록 | 각 동작에 대응하는 `lore` CLI 명령을 함께 만들어 상태바에 보여 준다(Sublime Merge 방식) |
| Undo | 브랜치 포인터 이동 기록(이전 리비전)을 남겨 `lore_branch_reset`으로 되돌린다. 리비전이 불변이라 안전하다 |

UI와는 두 경로로만 이야기한다: **명령**(Tauri command, 즉시 작업 ID를 돌려줌)과 **이벤트**(Tauri Channel로 진행·완료·상태 변경분).

## 4. 열린 질문에 대한 답(`lore.h` v0.10.0 기준)

| 질문 | 답 | 근거 |
|---|---|---|
| draft(로컬 커밋)가 리비전 번호를 받나 | **받는다.** 커밋 결과 이벤트에 `revision_number`가 있다. 미푸시 표시는 "원격 latest보다 앞선 리비전"으로 계산한다(`lore_branch_latest_list` + 로컬 브랜치) | `lore_revision_commit_revision_event_data_t.revision_number` |
| rebase/cherry-pick/squash 지원 범위 | **cherry-pick·amend·revert·reset·bisect는 있다. rebase·squash·split·fold 함수는 없다.** Restack = 새 베이스에서 스택을 차례로 `cherry_pick` + 브랜치 `reset`으로 구현한다. Fold/Squash는 앞 리비전으로 `reset` 후 합친 트리를 다시 커밋(또는 `amend`)하는 방식으로 직접 구성한다 | `lore_revision_cherry_pick_async`, `lore_revision_amend_async`, `lore_branch_reset_async` |
| 팀 전체 잠금 목록·알림 | **목록: 된다.** `lore_lock_file_query`가 브랜치 단위로 `owner`·`path`를 비우면 전부 돌려준다(경로, 소유자, 잠근 시각). **실시간: 된다.** `lore_notification_subscribe`가 `RESOURCE_LOCKED/UNLOCKED`, `BRANCH_PUSHED/CREATED/DELETED`를 push로 준다. **"잠금 해제 요청": API가 없다** → TOME 자체 채널이 필요(예: 저장소 메타데이터에 요청 기록, 또는 외부 메신저 연동) | `lore_lock_file_query_args_t`, Notification Events |
| View/Hydration | `.lore/view` 파일을 쓰는 방식이다. 동기화(`sync`)·복제 인자에 `view` 필드가 있어 그 파일을 기준으로 실체화한다. 디렉터리·파일 수 집계도 "view 기준"으로 준다 | `lore.h`의 `view` 인자, view-filtered 집계 |
| uasset 썸네일·구조 비교를 에디터 없이 | `.uasset` 패키지 헤더에 썸네일 테이블(`ThumbnailTableOffset`)이 있어 **에디터 없이 PNG/JPEG를 꺼낼 수 있다.** TOME은 이 썸네일 테이블만 직접 읽고(`tome-core::uasset`), 구조 비교 같은 깊은 분석은 커스텀 도구로 외부 도구를 부르게 한다 | UE 패키지 포맷(`FPackageFileSummary`) |

## 5. 다음 작업

1. 저장소 골격: Tauri 2 앱 + `tome-core` 크레이트 + `lore-sys`(bindgen), CI(Windows·macOS 빌드)
2. 첫 수직 슬라이스: 저장소 열기 → 상태 → 브랜치·리비전 히스토리를 Smartlog 목록으로 표시(읽기 전용)
3. 그 위에 커밋·push, 잠금 보드, View 다이얼로그 순서로 쓰기 기능
4. 로고·아이콘 방향
