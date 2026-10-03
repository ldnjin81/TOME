<img src="docs/logo/tome-icon.png" width="96" alt="">

# TOME

**Track, Own, Merge, Explore — Lore용 데스크톱 클라이언트.**

[English](README.md)

TOME은 Epic Games의 버전 관리 시스템 [Lore](https://github.com/EpicGames/lore)를 위한 커뮤니티 데스크톱 클라이언트입니다. Lore의 C API로 같은 프로세스 안에서 Lore를 직접 부르므로, 명령줄 출력을 해석하지 않습니다.

> **상태:** 초기(0.1). Lore 0.10 서버와 일상적으로 쓸 수 있으며, 주 플랫폼은 Windows입니다.

## 하는 일

- **Track** — 내 스택의 Smartlog. 서버 대비 push하지 않은 리비전, 모든 브랜치의 전체 그래프, 리비전·파일 diff, 파일 하나의 기록을 봅니다.
- **Own** — 팀 전체 잠금을 한 보드에서 보고, 파일·에셋 어디서든 잠그고 풉니다. 누가 잠그거나 풀거나 push하면 바로 알려 줍니다.
- **Merge** — 브랜치를 만들고 전환하고 병합하며, 충돌은 파일마다 정합니다. 끌어서 놓는 restack으로 내 스택을 서버의 새 리비전 위로 옮기거나 순서를 바꿉니다. 미리보기에서 충돌할 수 있는 파일과 다른 사람의 잠금을 보여 주고, 바이너리 파일은 남길 쪽을 미리 고를 수 있습니다.
- **Explore** — View를 고치면 바로 적용합니다(빠지는 파일과 다시 들어오는 파일까지). **아티스트 모드**는 작업본의 언리얼 에셋을 각 `.uasset`에 저장된 썸네일로 둘러보고, 변경·충돌·잠금을 표시합니다.

그 밖에: P4V식 커스텀 도구(파일·리비전 우클릭), 진행률과 취소가 있는 긴 작업(받기·동기화·push), 상태 표시줄에 동작마다 해당 Lore 명령줄.

## 설치 (Windows)

- **설치 파일** — `TOME_<버전>_x64-setup.exe`는 현재 사용자에게 설치합니다(관리자 권한 불필요). WebView2 런타임이 없으면 함께 설치합니다.
- **포터블** — `TOME_<버전>_x64_portable.zip`을 풀고 `tome.exe`를 실행합니다. WebView2 런타임이 필요하며 Windows 11에는 들어 있습니다.

설정은 `%APPDATA%\dev.ldnjin.tome`에 저장됩니다.

## 소스에서 빌드

```sh
git clone --recursive <이 저장소>
cd TOME/ui && npm ci && cd ..
cargo test -p tome-core            # 서버 테스트는 선택: TOME_TEST_SERVER=lore://host:port
ui/node_modules/.bin/tauri build   # 설치 파일과 zip은 scripts\package-windows.ps1
```

Lore는 `third_party/lore` 서브모듈에서 빌드해 함께 링크합니다.

## 공식 제품이 아닙니다

TOME은 독립 프로젝트이며 Epic Games가 만들거나 보증하거나 지원하지 않습니다. "Lore"는 Epic Games, Inc.의 상표이며, 여기서는 TOME이 무엇과 함께 동작하는지 밝히는 데만 씁니다. Lore의 MIT 라이선스는 코드에 대한 것이고 이름에 대한 권리를 주지 않습니다.

## 문서

- [구조와 기술 스택 결정](docs/architecture.md)

## 라이선스

TOME은 MIT 라이선스입니다([LICENSE](LICENSE)). Lore(MIT, Epic Games)와 여러 오픈 소스 패키지를 포함하며, 각 라이선스는 `scripts/third_party_notices.py`로 만든 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)에 있습니다.
