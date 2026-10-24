//! 임시 서버·제공자는 scripts/test-auth.py로 실행한다. 운영 서버는 사용하지 않는다.
use std::process::Command;
use tome_core::ops::{Job, Line, Op};
use tome_core::{CallResult, Repository, auth, model};

fn ok(result: CallResult) -> CallResult {
    assert!(result.ok(), "status {}: {}", result.status, result.error);
    result
}

#[test]
fn login_same_process_clone_push_and_logout() {
    let Ok(server) = std::env::var("TOME_OIDC_TEST_SERVER") else {
        eprintln!("skip: TOME_OIDC_TEST_SERVER 없음");
        return;
    };
    assert!(server.starts_with("lore://127.0.0.1:"));
    assert!(!server.contains(":41337"));
    assert!(
        std::env::var_os("LORE_AUTH_PATH").is_some(),
        "격리된 인증 저장소 필요"
    );
    eprintln!("OIDC 단계: 로그인 전 상태");
    let before = auth::state("", &server);
    assert_eq!(before.server_requires_login, Some(true), "{before:?}");
    assert_eq!(before.status, "required");
    eprintln!("OIDC 단계: 로그인 전 목록");
    assert!(!tome_core::list_repositories(&server, "settings-author").ok());

    eprintln!("OIDC 단계: 로그인 시작");
    let mut events = vec![];
    ok(auth::login("", &server, &mut |e| {
        eprintln!("OIDC 이벤트: {}", e["tagName"]);
        events.push(e.clone());
    }));
    assert!(
        events
            .iter()
            .any(|e| e["tagName"] == "authUrl" && e["data"]["userCode"] == "POC1-TEST")
    );
    assert!(events.iter().any(|e| e["tagName"] == "authPending"));
    assert!(events.iter().any(|e| e["tagName"] == "authUserInfo"));
    eprintln!("OIDC 단계: 로그인 후 상태");
    let after = auth::state("", &server);
    assert_eq!(after.status, "authenticated", "{after:?}");
    assert_eq!(after.user.as_ref().unwrap().id, "ldnjin");
    assert!(after.expires > 0);
    let shown = serde_json::to_string(&after).unwrap();
    assert!(!shown.contains("token"));
    // 목록 조회로 미리 로드한 토큰 캐시가 재시작 없이 갱신된다.
    ok(tome_core::list_repositories(&server, "settings-author"));

    eprintln!("OIDC 단계: 저장소 생성");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("tome-oidc-{stamp}"));
    let work = root.join("work");
    let copy = root.join("copy");
    let url = format!("{server}/oidc-{stamp}");
    let mut repo = Repository::open(work.to_string_lossy().to_string());
    repo.identity = "settings-author".into();
    ok(repo.create(&url));
    std::fs::write(work.join("hello.txt"), "authenticated").unwrap();
    ok(repo.scan_status());
    ok(repo.stage(&["hello.txt".into()]));
    ok(repo.commit("OIDC identity"));
    ok(repo.push("main"));
    assert_eq!(
        model::history(&ok(repo.history("main", 10)))[0].author,
        "ldnjin"
    );
    // 저장된 작업본 신원이 다른 사용자여도 인증 ID가 우선한다.
    let config = lore_revision::repository::get_dot_lore_path(&work)
        .unwrap()
        .join("config.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    let changed = text.replace("identity = \"ldnjin\"", "identity = \"old-author\"");
    assert_ne!(text, changed, "실제 작업본 신원 설정을 변경해야 한다");
    std::fs::write(&config, changed).unwrap();
    assert_eq!(
        lore_revision::repository::load_repository_config(&work)
            .unwrap()
            .identity
            .as_deref(),
        Some("old-author")
    );
    eprintln!("OIDC 단계: 기존 작업본 sync");
    ok(repo.sync());
    let mut cloned = Repository::open(copy.to_string_lossy().to_string());
    cloned.identity = "settings-author".into();
    ok(cloned.clone_from(&url, ""));
    assert_eq!(
        std::fs::read_to_string(copy.join("hello.txt")).unwrap(),
        "authenticated"
    );
    std::fs::write(copy.join("hello.txt"), "fresh worker").unwrap();
    ok(cloned.scan_status());
    ok(cloned.stage(&["hello.txt".into()]));
    ok(cloned.commit("Worker push"));
    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/tome-worker");
    let job = Job {
        op: Op::Push {
            path: copy.to_string_lossy().into(),
            branch: "main".into(),
        },
        identity: "settings-author".into(),
    };
    let output = Command::new(&exe)
        .arg(serde_json::to_string(&job).unwrap())
        .output()
        .unwrap();
    assert!(output.status.success(), "worker 상태 {:?}", output.status);
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| matches!(
                serde_json::from_str::<Line>(line),
                Ok(Line::Done { status: 0, .. })
            ))
    );
    eprintln!("OIDC 단계: 로그아웃");
    auth::logout("", &server).unwrap();
    assert_eq!(auth::state("", &server).status, "required");
    assert!(
        !tome_core::list_repositories(&server, "settings-author").ok(),
        "로그아웃 후 목록 접근 차단"
    );
    assert!(!repo.push("main").ok(), "로그아웃 후 작업본 접근 차단");
    // 재로그인도 같은 프로세스에서 복구된다.
    ok(auth::login("", &server, &mut |_| {}));
    ok(tome_core::list_repositories(&server, "settings-author"));
    eprintln!("OIDC 단계: 로그아웃");
    auth::logout("", &server).unwrap();
    std::fs::remove_dir_all(root).ok();
}
