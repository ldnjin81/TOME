//! 인증 메타데이터와 로그인. 토큰은 Lore 내부에만 두고 UI에는 허용된 이벤트만 보낸다.
use crate::{CallResult, call, call_watching, interface};
use lore::interface::{LoreGlobalArgs, LoreString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    // 연결 실패는 인증 없는 서버와 구분한다.
    pub server_requires_login: Option<bool>,
    pub status: String,
    pub user: Option<User>,
    pub expires: u64,
    pub detail: String,
}

#[derive(Default)]
struct Cache {
    endpoints: BTreeMap<String, String>,
    selected: BTreeMap<String, String>,
}
fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}
fn token_status(expires: u64, time: u64) -> &'static str {
    if expires != 0 && expires <= time {
        "expired"
    } else {
        "authenticated"
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn remote(path: &str, url: &str) -> Result<String, String> {
    if !url.trim().is_empty() {
        return Ok(url.trim().to_string());
    }
    lore_revision::repository::load_repository_config(path)
        .map_err(|_| "Repository not found".to_string())?
        .remote_url
        .filter(|u| !u.is_empty())
        .ok_or_else(|| "No remote configured".to_string())
}

fn endpoint(url: &str, fresh: bool) -> Result<String, String> {
    if !fresh && let Some(value) = cache().lock().unwrap().endpoints.get(url).cloned() {
        return Ok(value);
    }
    let execution = std::sync::Arc::new(lore_revision::interface::ExecutionContext::new_client(
        LoreGlobalArgs::default(),
        lore_revision::relay::EventDispatcher::new(None),
    ));
    let result = lore::runtime().block_on(lore_revision::lore_base::runtime::LORE_CONTEXT.scope(
        execution,
        async {
            // 환경 조회는 인증 전에도 가능하다. connect는 토큰부터 요구한다.
            let (remote, protocol) = lore_transport::parse(url).map_err(|e| e.to_string())?;
            let environment = protocol
                .environment(Default::default(), remote.as_str())
                .await
                .map_err(|e| e.to_string())?;
            let environment = environment.get().await.map_err(|e| e.to_string())?;
            lore_transport::auth::authentication::select(
                lore_transport::auth::AuthMode::Auto,
                &environment,
            )
            .map(|selected| selected.map(|s| s.endpoint.url).unwrap_or_default())
            .map_err(|e| e.to_string())
        },
    ));
    if let Ok(value) = &result {
        cache()
            .lock()
            .unwrap()
            .endpoints
            .insert(url.to_string(), value.clone());
    }
    result
}

fn identities() -> CallResult {
    call(
        interface::lore_auth_list_async,
        &LoreGlobalArgs::default(),
        &lore::auth::LoreAuthListArgs { with_token: 0 },
    )
}

// endpoint가 같은 인증용 항목만 고른다. 리소스 토큰이나 다른 서버의 로그인은 섞지 않는다.
fn select(list: &CallResult, endpoint: &str, selected: &str, time: u64) -> Option<(String, u64)> {
    let mut entries: Vec<_> = list
        .data("authIdentity")
        .filter(|e| e["authUrl"] == endpoint && e["resource"] == "")
        .filter_map(|e| {
            Some((
                e["userId"].as_str()?.to_string(),
                e["expires"].as_u64().unwrap_or(0),
            ))
        })
        .filter(|(id, _)| !id.is_empty())
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
        .iter()
        .find(|(id, _)| id == selected)
        .cloned()
        .or_else(|| {
            entries
                .iter()
                .find(|(_, expiry)| *expiry == 0 || *expiry > time)
                .cloned()
        })
        .or_else(|| entries.into_iter().next())
}

pub fn state(path: &str, url: &str) -> AuthState {
    let mut state = AuthState {
        server_requires_login: None,
        status: "error".into(),
        user: None,
        expires: 0,
        detail: String::new(),
    };
    let result = (|| {
        let url = remote(path, url)?;
        let endpoint = endpoint(&url, true)?;
        state.server_requires_login = Some(!endpoint.is_empty());
        if endpoint.is_empty() {
            state.status = "disabled".into();
            return Ok(());
        }
        let list = identities();
        if !list.ok() {
            return Err(list.error);
        }
        let selected = cache()
            .lock()
            .unwrap()
            .selected
            .get(&endpoint)
            .cloned()
            .unwrap_or_default();
        if let Some((id, expires)) = select(&list, &endpoint, &selected, now()) {
            let globals = LoreGlobalArgs {
                identity: LoreString::from_bytes(id.as_bytes()),
                ..Default::default()
            };
            let info = call(
                interface::lore_auth_local_user_info_async,
                &globals,
                &lore::auth::LoreAuthLocalUserInfoArgs {
                    auth_endpoint: LoreString::from_bytes(endpoint.as_bytes()),
                    user_ids: Default::default(),
                    with_identity_token: 0,
                    with_access_token: 0,
                },
            );
            if !info.ok() {
                return Err(info.error);
            }
            let name = info
                .data("authUserInfo")
                .find(|u| u["id"] == id)
                .and_then(|u| u["name"].as_str())
                .unwrap_or(&id)
                .to_string();
            state.user = Some(User { id, name });
            state.expires = expires;
            state.status = token_status(expires, now()).into();
        } else {
            state.status = "required".into();
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        state.detail = error;
    }
    state
}

/// 인증 서버에서는 설정 이름이나 기존 작업본 신원 대신 로그인 ID를 명시한다.
pub fn identity_for_remote(url: &str, fallback: &str, online: bool) -> String {
    let known = cache().lock().unwrap().endpoints.get(url).cloned();
    let endpoint = known.or_else(|| online.then(|| endpoint(url, false).ok()).flatten());
    let Some(endpoint) = endpoint.filter(|e| !e.is_empty()) else {
        return fallback.to_string();
    };
    let selected = cache()
        .lock()
        .unwrap()
        .selected
        .get(&endpoint)
        .cloned()
        .unwrap_or_default();
    select(&identities(), &endpoint, &selected, now())
        .map(|(id, _)| id)
        .unwrap_or_default()
}
pub fn identity_for_path(path: &str, fallback: &str, online: bool) -> String {
    remote(path, "")
        .map(|url| identity_for_remote(&url, fallback, online))
        .unwrap_or_else(|_| fallback.into())
}

pub fn login(path: &str, url: &str, watch: &mut dyn FnMut(&Value)) -> CallResult {
    let url = match remote(path, url) {
        Ok(url) => url,
        Err(error) => {
            return CallResult {
                events: vec![],
                status: -1,
                error,
            };
        }
    };
    let result = call_watching(
        interface::lore_auth_login_interactive_async,
        &LoreGlobalArgs::default(),
        &lore::auth::LoreAuthLoginInteractiveArgs {
            remote_url: LoreString::from_bytes(url.as_bytes()),
            no_browser: 1,
        },
        &mut |event| {
            if matches!(
                event["tagName"].as_str(),
                Some("authUrl" | "authPending" | "authUserInfo")
            ) {
                watch(event);
            }
        },
    );
    if result.ok()
        && let Some(id) = result
            .data("authUserInfo")
            .next()
            .and_then(|u| u["id"].as_str())
        && let Ok(endpoint) = endpoint(&url, true)
    {
        cache()
            .lock()
            .unwrap()
            .selected
            .insert(endpoint, id.to_string());
    }
    result
}

pub fn logout(path: &str, url: &str) -> Result<(), String> {
    let endpoint = endpoint(&remote(path, url)?, true)?;
    if endpoint.is_empty() {
        return Ok(());
    }
    let state = state(path, url);
    // 빈 ID는 모든 사용자를 삭제하므로 대상 사용자가 없으면 호출하지 않는다.
    let Some(user) = state.user else {
        return if state.status == "error" {
            Err(state.detail)
        } else {
            Ok(())
        };
    };
    let result = call(
        interface::lore_auth_logout_async,
        &LoreGlobalArgs::default(),
        &lore::auth::LoreAuthLogoutArgs {
            auth_url: LoreString::from_bytes(endpoint.as_bytes()),
            resource: LoreString::default(),
            user_id: LoreString::from_bytes(user.id.as_bytes()),
        },
    );
    if result.ok() {
        cache().lock().unwrap().selected.remove(&endpoint);
        // 재사용 연결에 남은 인증 정보도 제거해야 로그아웃이 즉시 적용된다.
        lore_revision::interface::drop_connections();
        Ok(())
    } else {
        Err(result.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn zero_expiry_is_unknown_and_deadline_is_expired() {
        assert_eq!(token_status(0, 100), "authenticated");
        assert_eq!(token_status(101, 100), "authenticated");
        assert_eq!(token_status(100, 100), "expired");
        assert_eq!(token_status(99, 100), "expired");
    }
    #[test]
    fn selection_filters_endpoint_resource_and_expiry() {
        let list = CallResult {
            status: 0,
            error: String::new(),
            events: vec![
                json!({"tagName":"authIdentity","data":{"authUrl":"other","resource":"","userId":"other","expires":0}}),
                json!({"tagName":"authIdentity","data":{"authUrl":"issuer","resource":"repo","userId":"resource","expires":0}}),
                json!({"tagName":"authIdentity","data":{"authUrl":"issuer","resource":"","userId":"expired","expires":1}}),
                json!({"tagName":"authIdentity","data":{"authUrl":"issuer","resource":"","userId":"current","expires":0}}),
            ],
        };
        assert_eq!(select(&list, "issuer", "", 10), Some(("current".into(), 0)));
        assert_eq!(
            select(&list, "issuer", "expired", 10),
            Some(("expired".into(), 1))
        );
        assert_eq!(select(&list, "missing", "", 10), None);
        assert_eq!(
            select(&list, "issuer", "current", 0),
            Some(("current".into(), 0))
        );
    }
}
