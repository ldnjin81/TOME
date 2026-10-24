#!/usr/bin/env bash
# 설치 파일을 서명하고 배포용 매니페스트만 만든다. 업로드는 별도 승인 후 수행한다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo '사용법: scripts/release-updater.sh <version> <setup.exe 경로> [notes 파일]' >&2; exit 2
fi
VERSION="$1"
SETUP="$2"
NOTES="${3:-}"
# 경로·URL에 들어갈 버전은 SemVer 형식만 허용한다.
python3 - "$VERSION" "$SETUP" "$NOTES" <<'PY'
import re, sys
from pathlib import Path
version, setup, notes = sys.argv[1:]
if not re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?', version):
    sys.exit('잘못된 버전 형식')
if not Path(setup).is_file() or (notes and not Path(notes).is_file()):
    sys.exit('설치 파일 또는 변경 내용 파일이 없습니다.')
PY
KEY="${TOME_UPDATER_KEY:-$HOME/.config/tome-updater/tome-updater.key}"
# CLI가 대화식으로 비밀번호를 묻거나 민감한 출력을 남기지 않게 한다.
if [ "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD+x}" != x ]; then
  echo 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD 환경변수를 설정하세요.' >&2; exit 2
fi
SETUP="$(cd "$(dirname "$SETUP")" && pwd)/$(basename "$SETUP")"
OUT="$(dirname "$SETUP")"
ARTIFACT="$OUT/TOME_${VERSION}_x64-setup.exe"
if [ "$SETUP" != "$ARTIFACT" ]; then
  if [ -e "$ARTIFACT" ]; then echo '같은 이름의 배포 파일이 이미 있습니다.' >&2; exit 2; fi
  cp "$SETUP" "$ARTIFACT"
fi
if ! "$ROOT/ui/node_modules/.bin/tauri" signer sign -f "$KEY" --app-version "$VERSION" "$ARTIFACT" >/dev/null 2>&1; then
  echo '서명 실패: 키 경로와 비밀번호 환경변수를 확인하세요.' >&2; exit 1
fi
python3 - "$VERSION" "$ARTIFACT" "$NOTES" <<'PY'
import json, sys
from datetime import datetime, timezone
from pathlib import Path
version, setup, notes = sys.argv[1:]
p = Path(setup)
data = {'version': version, 'notes': Path(notes).read_text() if notes else '',
        'pub_date': datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z'),
        'platforms': {'windows-x86_64': {'signature': Path(setup + '.sig').read_text().strip(),
        'url': f'https://github.com/ldnjin81/TOME/releases/download/v{version}/{p.name}'}}}
p.with_name('latest.json').write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')
PY
printf '생성 파일:\n  %s\n  %s\n  %s\n' "$ARTIFACT" "$ARTIFACT.sig" "$OUT/latest.json"
printf '사용자 승인 후 Claude가 실행할 예시:\n'
printf 'gh release upload %q %q %q %q --repo ldnjin81/TOME\n' "v$VERSION" "$ARTIFACT" "$ARTIFACT.sig" "$OUT/latest.json"
