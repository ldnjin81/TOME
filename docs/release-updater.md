# Windows 업데이트 릴리스 준비 (맥)

공식 [Tauri updater](https://v2.tauri.app/plugin/updater/)를 사용한다. 앱의 공개키로 설치 파일 서명을 검증한다. `createUpdaterArtifacts`를 켜지 않으므로 일반 빌드는 비밀키 없이 가능하다. 버전 변경·공개 릴리스·업로드는 사용자 승인 후 Claude가 별도로 수행한다.

```bash
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s tome-updater-key -a signing -w)" \
  scripts/release-updater.sh 0.4.0 /path/to/TOME_0.4.0_x64-setup.exe /path/to/notes.txt
```

키 경로 기본값은 `~/.config/tome-updater/tome-updater.key`이며 `TOME_UPDATER_KEY`로 바꿀 수 있다. 비밀번호는 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 환경변수로만 받는다. 스크립트는 키체인을 직접 조회하지 않으며 키·비밀번호·서명 내용을 출력하지 않는다. 출력은 설치 파일, `.sig`, `latest.json` 경로와 승인 후 실행할 `gh release upload` 예시다. 입력 파일 이름이 다르면 배포 이름의 복사본을 같은 폴더에 만든다. 기존 배포 파일을 덮어쓰지 않는다.

매니페스트는 `windows-x86_64` 설치형 앱용이다. 앱과 설치 파일의 버전은 매니페스트 버전과 일치시킨다. 서명은 `--app-version`으로 버전에 연결한다. `.sig`에는 파일 내용 전체가 아닌 서명 텍스트가 들어간다. 공개키는 `src-tauri/tauri.conf.json`에 있으며 비밀키는 저장소·아티팩트에 넣지 않는다.

시험에서는 임시 키만 사용한다. `TOME_UPDATER_ENDPOINT=http://127.0.0.1:<port>/latest.json`을 지정한 개발 빌드로 로컬 매니페스트를 확인할 수 있다. 릴리스 빌드는 HTTPS가 필요하다. 포터블·맥·개발 빌드는 확인 후 릴리스 페이지를 열며 자동 설치하지 않는다. 설치·재시작 검증은 Windows 설치형 앱에서 수행한다.
