"""격리된 TOME 통합 테스트용 OIDC 제공자 (표준 라이브러리 + openssl CLI만 사용).

Lore 0.11 scripts/test/mock_oidc_provider.py와 같은 엔드포인트를 흉내 낸다:
discovery, device(자동 승인), token(device code / token-exchange api-key), jwks.

환경변수:
  POC_PORT            기본 41450
  POC_HOST            issuer 호스트, 기본 127.0.0.1
  POC_TOKEN_LIFETIME  액세스 토큰 수명(초), 기본 3600
  POC_GROUPS          groups 클레임(쉼표 구분), 기본 admin,owner,migrate,push-protected
  POC_USER            device 로그인 사용자 sub, 기본 ldnjin
  POC_PENDING         device 승인 전 authorization_pending 응답 횟수, 기본 1
"""

import base64, json, os, subprocess, tempfile, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qsl

KEY_DIR = tempfile.TemporaryDirectory(prefix="tome-oidc-key-")
KEY = os.path.join(KEY_DIR.name, "key.pem")
PORT = int(os.environ.get("POC_PORT", "41450"))
HOST = os.environ.get("POC_HOST", "127.0.0.1")
ISSUER = f"http://{HOST}:{PORT}"
LIFETIME = int(os.environ.get("POC_TOKEN_LIFETIME", "3600"))
GROUPS = [g for g in os.environ.get("POC_GROUPS", "admin,owner,migrate,push-protected").split(",") if g]
CLIENT_ID = "lore-cli"
AUDIENCE = ["lore-poc", "127.0.0.1", HOST]
KID = "poc-key-1"
DEVICE_CODE = "poc-device-code"
USER_CODE = "POC1-TEST"
USERS = {
    os.environ.get("POC_USER", "ldnjin"): ("대로 (PoC)", "ldnjin"),
    "agent": ("자동화 에이전트", "agent"),
}
API_KEYS = {"poc-agent-key": "agent"}
LOG = []
LOCK = threading.Lock()
PENDING = {"left": int(os.environ.get("POC_PENDING", "1"))}


def b64u(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def ensure_key():
    if not os.path.exists(KEY):
        subprocess.run(["openssl", "genrsa", "-out", KEY, "2048"], check=True, capture_output=True)


def jwks():
    mod = subprocess.run(["openssl", "rsa", "-in", KEY, "-noout", "-modulus"],
                         check=True, capture_output=True, text=True).stdout.strip().split("=", 1)[1]
    n = bytes.fromhex(mod)
    return {"keys": [{"kty": "RSA", "n": b64u(n), "e": b64u((65537).to_bytes(3, "big")),
                      "kid": KID, "alg": "RS256", "use": "sig"}]}


def sign(claims: dict) -> str:
    header = {"alg": "RS256", "typ": "JWT", "kid": KID}
    si = b64u(json.dumps(header, separators=(",", ":")).encode()) + "." + \
        b64u(json.dumps(claims, separators=(",", ":"), ensure_ascii=False).encode())
    sig = subprocess.run(["openssl", "dgst", "-sha256", "-sign", KEY], input=si.encode(),
                         check=True, capture_output=True).stdout
    return si + "." + b64u(sig)


def access_token(sub: str) -> str:
    now = int(time.time())
    return sign({"iss": ISSUER, "sub": sub, "aud": AUDIENCE, "iat": now, "exp": now + LIFETIME,
                 "client_id": CLIENT_ID, "scope": "openid offline_access", "groups": GROUPS})


def id_token(sub: str) -> str:
    now = int(time.time())
    name, pref = USERS[sub]
    return sign({"iss": ISSUER, "sub": sub, "aud": CLIENT_ID, "iat": now, "exp": now + 300,
                 "name": name, "preferred_username": pref})


def token(form):
    gt = form.get("grant_type", "")
    if form.get("client_id") != CLIENT_ID:
        return 400, {"error": "invalid_client"}
    if gt == "urn:ietf:params:oauth:grant-type:device_code":
        if form.get("device_code") != DEVICE_CODE:
            return 400, {"error": "invalid_grant"}
        with LOCK:
            if PENDING["left"] > 0:
                PENDING["left"] -= 1
                return 400, {"error": "authorization_pending"}
        sub = next(iter(USERS))
        return 200, {"access_token": access_token(sub), "token_type": "Bearer", "expires_in": LIFETIME,
                     "scope": "openid offline_access", "id_token": id_token(sub)}
    if gt == "urn:ietf:params:oauth:grant-type:token-exchange":
        if form.get("subject_token_type") in ("urn:lore:params:oauth:token-type:api-key", "api-key"):
            sub = API_KEYS.get(form.get("subject_token", ""))
            if not sub:
                return 400, {"error": "invalid_grant"}
            return 200, {"access_token": access_token(sub), "token_type": "Bearer", "expires_in": LIFETIME,
                         "issued_token_type": "urn:ietf:params:oauth:token-type:access_token"}
        return 400, {"error": "invalid_target"}
    if gt == "refresh_token":
        return 400, {"error": "unsupported_grant_type"}
    return 400, {"error": "unsupported_grant_type"}


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def out(self, code, obj, ctype="application/json"):
        body = obj if isinstance(obj, bytes) else json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        p = self.path.split("?")[0]
        if p == "/.well-known/openid-configuration":
            self.out(200, {"issuer": ISSUER, "token_endpoint": f"{ISSUER}/token", "jwks_uri": f"{ISSUER}/jwks",
                           "device_authorization_endpoint": f"{ISSUER}/device",
                           "scopes_supported": ["openid", "offline_access"]})
        elif p == "/jwks":
            self.out(200, jwks())
        elif p == "/activate":
            self.out(200, "<html><body>PoC: 자동 승인됨</body></html>".encode(), "text/html; charset=utf-8")
        else:
            self.out(404, {"error": "not found"})

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        form = dict(parse_qsl(self.rfile.read(n).decode()))
        p = self.path.split("?")[0]
        if p == "/device":
            if form.get("client_id") != CLIENT_ID:
                return self.out(400, {"error": "invalid_client"})
            self.out(200, {"device_code": DEVICE_CODE, "user_code": USER_CODE,
                           "verification_uri": f"{ISSUER}/activate",
                           "verification_uri_complete": f"{ISSUER}/activate?user_code={USER_CODE}",
                           "expires_in": 300, "interval": 1})
        elif p == "/token":
            self.out(*token(form))
        else:
            self.out(404, {"error": "not found"})


if __name__ == "__main__":
    ensure_key()
    print(f"PoC OIDC {ISSUER} lifetime={LIFETIME}s groups={GROUPS}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
