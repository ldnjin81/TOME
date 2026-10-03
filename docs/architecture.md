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

- **C API가 계약이다.** 모든 함수가 `lore_*_async(globals, args, callback)` 형태이고, 결과는 `lore_event_t` 스트림으로 온다(진행, 파일 하나씩, `COMPLETE`, `END`). TOME은 Lore 내부 함수가 아니라 이 함수들만 부른다.
- **연결 방식(구현됨):** Lore를 git 서브모듈 `third_party/lore`로 태그 `v0.10.0`에 고정한다. 이 태그에서는 C API 함수가 `lore` 크레이트의 `lore::interface` 모듈에 `#[no_mangle] pub extern "C"`로 들어 있고 크레이트가 rlib로도 빌드되므로, `tome-core`가 Rust 의존성으로 **정적 링크**해 그 함수를 직접 부른다. bindgen·DLL이 필요 없고, `#[repr(C)]` 인자 구조체를 그대로 쓴다.
- Lore를 우리 워크스페이스에서 빌드하려면 Lore와 같은 설정이 필요하다: `[patch.crates-io]`의 `quinn-proto`·`glob-match`(Lore `vendor/`), rustflags `--cfg tokio_unstable --cfg uuid_unstable`, 그리고 Lore의 `Cargo.lock`을 출발점으로 쓴다.
- **주의:** Lore `main`에서는 C API가 별도 크레이트 `lore-capi`(cdylib·staticlib 전용)로 옮겨졌다. 다음 태그로 올릴 때는 그 크레이트를 정적 라이브러리로 빌드해 `lore.h` 바인딩으로 링크하는 방식으로 바꿔야 한다(계약은 같은 C API라 `tome-core`의 호출부는 그대로다).
- 콜백은 Lore 워커 스레드에서 오고 이벤트 데이터는 콜백이 끝나면 무효가 되므로, `tome-core::call`이 이벤트를 즉시 JSON으로 복사(Lore가 제공하는 serde 형식)해 채널로 넘긴다.

## 3. 코어 레이어(`tome-core`)

| 구성 | 역할 |
|---|---|
| `call` | Lore C API 호출 하나를 콜백 이벤트(JSON)로 모아 결과로 돌려준다. 안전하지 않은 호출을 한곳에 가둔다 |
| `Repository` | 작업본 하나. 전역 인자(경로, 오프라인 여부, 신원)를 들고 명령을 낸다 |
| `ops` | 오래 걸리는 받기·동기화·push를 별도 작업 프로세스로 돌려 진행률을 받고, 취소는 그 프로세스를 끝내는 것으로 한다(Lore에 호출 단위 취소가 없다) |
| `notify` | 서버 알림 구독(잠금·push·브랜치). 구독이 끝날 때까지 콜백 컨텍스트를 소유한다 |
| `graph` | 리비전 DAG의 레인 배치를 Rust에서 계산한다(부모 최대 2개라 단순). 레인 색은 브랜치 **ID** 기준 |
| `restack`·`view`·`uasset`·`assets`·`tools` | restack, View 적용, 썸네일 읽기, 에셋 목록, 커스텀 도구 |
| 명령 기록 | 각 동작에 대응하는 `lore` CLI 명령을 함께 만들어 상태바에 보여 준다(Sublime Merge 방식) |
| Undo | 브랜치 포인터 이동 기록(이전 리비전)을 남겨 `lore_branch_reset`으로 되돌린다. 리비전이 불변이라 안전하다 |

UI와는 두 경로로만 이야기한다: **명령**(Tauri command, 즉시 작업 ID를 돌려줌)과 **이벤트**(Tauri Channel로 진행·완료·상태 변경분).

## 4. 열린 질문에 대한 답(`lore.h` v0.10.0 기준)

| 질문 | 답 | 근거 |
|---|---|---|
| draft(로컬 커밋)가 리비전 번호를 받나 | **받는다.** 커밋 결과 이벤트에 `revision_number`가 있다. 미푸시 표시는 "원격 latest보다 앞선 리비전"으로 계산한다(`lore_branch_latest_list` + 로컬 브랜치) | `lore_revision_commit_revision_event_data_t.revision_number` |
| rebase/cherry-pick/squash 지원 범위 | **cherry-pick·amend·revert·reset·bisect는 있다. rebase·squash·split·fold 함수는 없다.** Restack = 새 베이스에서 스택을 차례로 `cherry_pick` + 브랜치 `reset`으로 구현한다. Fold/Squash는 앞 리비전으로 `reset` 후 합친 트리를 다시 커밋(또는 `amend`)하는 방식으로 직접 구성한다 | `lore_revision_cherry_pick_async`, `lore_revision_amend_async`, `lore_branch_reset_async` |
| 팀 전체 잠금 목록·알림 | **목록: 된다.** `lore_lock_file_query`가 브랜치 단위로 `owner`·`path`를 비우면 전부 돌려준다(경로, 소유자, 잠근 시각). **실시간: 된다.** `lore_notification_subscribe`가 `RESOURCE_LOCKED/UNLOCKED`, `BRANCH_PUSHED/CREATED/DELETED`를 push로 준다. **"잠금 해제 요청": API가 없다** → TOME 자체 채널이 필요(예: 저장소 메타데이터에 요청 기록, 또는 외부 메신저 연동) | `lore_lock_file_query_args_t`, Notification Events |
| View/Hydration | `.lore/view` 파일을 쓰는 방식이다(그냥 쓴 줄은 **제외**, `!` 줄은 다시 **포함**). 복제와 새 리비전을 받는 sync 때만 실체화하고, **v0.10.0은 View를 바꿔도 이미 받은 파일을 지우거나 새로 받지 않는다**(변경 없는 sync는 아무것도 안 함). 그래서 TOME이 직접 적용한다(`tome-core::view`): Lore의 필터 코드(`lore_revision::filter`)로 판정해 View 밖의 변경 없는 파일은 지우고(Lore는 이를 삭제로 보지 않는다), 변경·신규 파일과 `.loreignore` 대상은 남긴다. 다시 들어온 파일은 status에 '삭제'로 나오므로 그 경로만 `lore_file_reset`으로 받아 온다(reset은 수정 내용도 덮어쓰므로 경로를 좁힌다) | `lore.h`의 `view` 인자, 서버 통합 테스트 |
| uasset 썸네일·구조 비교를 에디터 없이 | `.uasset` 패키지 헤더에 썸네일 테이블(`ThumbnailTableOffset`)이 있어 **에디터 없이 PNG/JPEG를 꺼낼 수 있다.** TOME은 이 썸네일 테이블만 직접 읽고(`tome-core::uasset`), 구조 비교 같은 깊은 분석은 커스텀 도구로 외부 도구를 부르게 한다 | UE 패키지 포맷(`FPackageFileSummary`) |

## 5. 지금 상태 (0.1)

- Lore v0.10.0을 `third_party/lore` 서브모듈에서 빌드하고, C API(`lore::interface`)를 bindgen 없이 같은 프로세스에서 직접 부른다. C API에 없는 cherry-pick은 Lore 라이브러리 함수를 같은 방식으로 부른다(`tome-core::restack`).
- 구현됨: Smartlog(스택·전체 그래프), 스테이징·커밋·push, 잠금 보드와 실시간 알림, View 적용, 브랜치·병합·충돌 해결, 파일 기록과 diff, 진행률·취소가 있는 긴 작업(별도 작업 프로세스), 커스텀 도구, 아티스트 모드(uasset 썸네일), 끌어서 놓는 restack.
- 서버 테스트는 `TOME_TEST_SERVER`로 버리는 loreserver를 가리킬 때만 돈다(Lore 기본 포트 41337은 거부).
