#!/usr/bin/env python3
"""표준 라이브러리와 openssl로 격리 Lore/OIDC 서버를 실행해 통합 테스트한다.

사용: python3 scripts/test-auth.py [--only-oidc | --only-noauth]
LORESERVER_BIN으로 이미 빌드한 서버 실행 파일을 지정할 수 있다.
운영 서버/사용자 토큰 저장소는 사용하지 않으며 종료 시 시험 프로세스를 정리한다.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
SERVER = Path(os.environ.get("LORESERVER_BIN", ROOT / "third_party/lore/target/debug/loreserver"))


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def ready(port, process):
    for _ in range(300):
        if process.poll() is not None:
            raise RuntimeError("시험 서비스 시작 실패: 임시 로그 확인 필요")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=.1):
                return
        except OSError:
            time.sleep(.1)
    raise RuntimeError("시험 서비스 시작 시간 초과")


def run(root, oidc):
    port, http_port, internal = free_port(), free_port(), free_port()
    work = root / ("oidc" if oidc else "noauth")
    config = work / "lore-server/config/local.toml"
    config.parent.mkdir(parents=True)
    text = f'''[server.grpc]
port = {port}
[server.quic]
port = {port}
[server.http]
port = {http_port}
[server.grpc_internal]
port = {internal}
enabled = false
[server.quic_internal]
port = {internal}
enabled = false
[immutable_store.local]
path = {json.dumps(str(work / "data/imm"))}
[mutable_store.local]
path = {json.dumps(str(work / "data/mut"))}
'''
    # 시험 암호화 키도 임시 인증 경로에 두어 사용자 키체인과 완전히 격리한다.
    env = dict(os.environ, LORE_AUTH_PATH=str(work / "auth"), LORE_AUTH_STORE="fallback", LORE_MAX_THREADS="4")
    processes = []
    logs = []
    try:
        if oidc:
            provider_port = free_port()
            issuer = f"http://127.0.0.1:{provider_port}"
            log = open(work / "provider.log", "w")
            logs.append(log)
            provider = subprocess.Popen([sys.executable, str(ROOT / "crates/tome-core/tests/mock_oidc_provider.py")],
                                        env=dict(env, POC_PORT=str(provider_port), POC_HOST="127.0.0.1", POC_PENDING="1"), stdout=log, stderr=subprocess.STDOUT)
            processes.append(provider)
            ready(provider_port, provider)
            with urllib.request.urlopen(issuer + "/.well-known/openid-configuration", timeout=5) as response:
                assert json.load(response)["issuer"] == issuer
            text += f'''[server.auth]
jwt_issuer = "{issuer}"
jwt_audience = ["lore-poc"]
permission_claim = "groups"
[server.auth.oidc]
client_id = "lore-cli"
scopes = ["openid", "offline_access"]
'''
        config.write_text(text)
        log = open(work / "server.log", "w")
        logs.append(log)
        server = subprocess.Popen([str(SERVER)], cwd=work, env=env, stdout=log, stderr=subprocess.STDOUT)
        processes.append(server)
        ready(port, server)
        url = f"lore://127.0.0.1:{port}"
        if oidc:
            env.pop("TOME_TEST_SERVER", None)
            env["TOME_OIDC_TEST_SERVER"] = url
            command = ["cargo", "test", "-p", "tome-core", "--test", "oidc_server", "--", "--nocapture"]
        else:
            env.pop("TOME_OIDC_TEST_SERVER", None)
            env["TOME_TEST_SERVER"] = url
            command = ["cargo", "test"]
        subprocess.run(command, cwd=ROOT, env=env, check=True)
    except Exception:
        # 토큰이 없는 서버 시작 오류만 확인하도록 경로를 남긴다. 인증 응답은 출력하지 않는다.
        print(f"시험 실패. 격리 로그 경로: {work}", file=sys.stderr)
        raise
    finally:
        for process in reversed(processes):
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for log in logs:
            log.close()


if __name__ == "__main__":
    if not SERVER.is_file():
        sys.exit("LORESERVER_BIN에 빌드된 시험 서버 경로를 지정하세요.")
    subprocess.run(["cargo", "build", "--examples"], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="tome-auth-tests-") as folder:
        root = Path(folder)
        if "--only-oidc" not in sys.argv:
            run(root, False)
        if "--only-noauth" not in sys.argv:
            run(root, True)
    print("격리 서버 통합 검증 완료")
