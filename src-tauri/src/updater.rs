//! 공식 updater의 확인 결과를 보관하고 설치 시 같은 서명·버전을 사용한다.
use serde::Serialize;
use std::{path::Path, sync::{Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
use tauri::{Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    installing: AtomicBool,
}

#[derive(Serialize, Debug)]
pub struct UpdateInfo {
    available: bool,
    current: String,
    version: Option<String>,
    notes: Option<String>,
    date: Option<String>,
    installable: bool,
    check_failed: bool,
}

fn installable(windows: bool, development: bool, executable: &Path) -> bool {
    windows && !development && !executable.parent().is_some_and(|p| p.join("portable.txt").exists())
}

fn can_install() -> bool {
    std::env::current_exe().ok().is_some_and(|p| installable(cfg!(target_os = "windows"), cfg!(debug_assertions), &p))
}

fn describe(current: String, result: tauri_plugin_updater::Result<Option<Update>>) -> (UpdateInfo, Option<Update>) {
    let check_failed = result.is_err();
    let update = result.ok().flatten();
    let info = UpdateInfo {
        available: update.is_some(), current,
        version: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()),
        date: update.as_ref().and_then(|u| u.raw_json.get("pub_date").and_then(|d| d.as_str()).map(str::to_owned)),
        installable: can_install(), check_failed,
    };
    (info, update)
}

#[tauri::command]
pub async fn update_check(app: tauri::AppHandle) -> UpdateInfo {
    check_at(&app, std::env::var("TOME_UPDATER_ENDPOINT").ok()).await
}

async fn check_at<R: tauri::Runtime>(app: &tauri::AppHandle<R>, endpoint: Option<String>) -> UpdateInfo {
    let state = app.state::<UpdateState>();
    let result = async {
        let mut builder = app.updater_builder().timeout(Duration::from_secs(15));
        // 맥·개발판에서도 Windows 릴리스의 존재를 알려 주되 설치는 금지한다.
        if !can_install() { builder = builder.target("windows-x86_64"); }
        if let Some(endpoint) = endpoint {
            builder = builder.endpoints(vec![endpoint.parse().map_err(tauri_plugin_updater::Error::UrlParse)?])?;
        }
        builder.build()?.check().await
    }.await;
    let (info, update) = describe(app.package_info().version.to_string(), result);
    *state.pending.lock().unwrap() = update;
    info
}

#[derive(Clone, Serialize)]
struct Progress { received: u64, total: Option<u64>, phase: &'static str }

struct Installing<'a>(&'a AtomicBool);
impl Drop for Installing<'_> {
    fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
}

#[tauri::command]
pub async fn update_install(app: tauri::AppHandle, version: String, allow_running_jobs: bool) -> Result<(), String> {
    if !can_install() { return Err("release_only".into()); }
    let state = app.state::<UpdateState>();
    if state.installing.swap(true, Ordering::SeqCst) { return Err("already_installing".into()); }
    let _guard = Installing(&state.installing);
    if !allow_running_jobs && !app.state::<super::Jobs>().running.lock().unwrap().is_empty() {
        return Err("jobs_running".into());
    }
    let update = state.pending.lock().unwrap().clone().filter(|u| u.version == version).ok_or("check_again")?;
    let mut received = 0;
    update.download_and_install(|chunk, total| {
        received += chunk as u64;
        let _ = app.emit("tome-update-progress", Progress { received, total, phase: "download" });
    }, || {
        let _ = app.emit("tome-update-progress", Progress { received: 0, total: None, phase: "install" });
    }).await.map_err(|_| "install_failed".to_string())?;
    app.restart();
}

#[tauri::command]
pub fn update_open_release() -> Result<(), String> {
    super::auth_open_url("https://github.com/ldnjin81/TOME/releases/latest".into()).map_err(|_| "open_failed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_and_development_never_install() {
        let dir = std::env::temp_dir().join(format!("tome-portable-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("tome.exe");
        assert!(installable(true, false, &exe));
        assert!(!installable(false, false, &exe));
        assert!(!installable(true, true, &exe));
        std::fs::write(dir.join("portable.txt"), "portable").unwrap();
        assert!(!installable(true, false, &exe));
        std::fs::remove_file(dir.join("portable.txt")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn failed_checks_are_quiet_and_not_available() {
        let (info, pending) = describe("0.3.0".into(), Err(tauri_plugin_updater::Error::ReleaseNotFound));
        assert!(!info.available);
        assert!(info.check_failed);
        assert!(pending.is_none());
        let (info, _) = describe("0.3.0".into(), Ok(None));
        assert!(!info.available);
        assert!(!info.check_failed);
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;
    use std::io::{Read, Write};

    fn serve(status: &str, body: String) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/latest.json", listener.local_addr().unwrap());
        let status = status.to_owned();
        listener.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && started.elapsed() < Duration::from_secs(5) => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => panic!("시험 서버 요청 대기 실패: {e}"),
                }
            };
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request);
            write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        (url, worker)
    }

    #[test]
    fn local_manifest_versions_404_and_offline() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert("updater".into(), config["plugins"]["updater"].clone());
        context.package_info_mut().version = "0.3.0".parse().unwrap();
        let app = tauri::test::mock_builder()
            .manage(UpdateState::default())
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context).unwrap();
        tauri::async_runtime::block_on(async {
            for (version, available) in [("0.4.0", true), ("0.3.0", false), ("0.2.0", false), ("0.4.0-beta.1", true)] {
                let body = serde_json::json!({"version":version,"notes":"시험 변경 내용", "pub_date":"2026-10-24T00:00:00Z",
                    "platforms":{"windows-x86_64":{"url":"https://example.com/setup.exe", "signature":"test"}}}).to_string();
                let (url, worker) = serve("200 OK", body);
                let info = check_at(app.handle(), Some(url)).await;
                worker.join().unwrap();
                assert!(!info.check_failed, "{info:?}");
                assert_eq!(info.available, available, "{version}");
                assert!(!info.installable);
                if available { assert_eq!(info.version.as_deref(), Some(version)); }
            }
            let (url, worker) = serve("404 Not Found", "{}".into());
            let info = check_at(app.handle(), Some(url)).await;
            worker.join().unwrap();
            assert!(!info.available && info.check_failed);
            assert!(app.state::<UpdateState>().pending.lock().unwrap().is_none());
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/latest.json", listener.local_addr().unwrap());
            drop(listener);
            let info = check_at(app.handle(), Some(url)).await;
            assert!(!info.available && info.check_failed);
        });
    }
}
