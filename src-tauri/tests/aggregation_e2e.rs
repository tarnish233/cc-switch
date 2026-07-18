//! 模型聚合功能端到端测试
//!
//! 启动一个**真实的本地代理实例**（`ProxyServer`）+ 一个模拟上游服务，
//! 通过真实 HTTP 请求验证「按请求模型名路由到对应供应商」的完整链路：
//! - 聚合关闭时走当前供应商
//! - 聚合开启且命中路由时走目标供应商
//! - 未命中路由时回退当前供应商
//! - 命中带 `upstream_model` 的路由时改写上游模型名
//!
//! 代理监听端口用 0（临时端口），避免与本机已运行的正式实例（15721）冲突。

#[path = "support.rs"]
mod support;
use support::{create_test_state, ensure_test_home, reset_test_fs, test_mutex};

use cc_switch_lib::{ModelRoute, Provider};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 模拟上游命中记录：命中的 URL 路径 + 请求体里的模型名
#[derive(Clone, Debug)]
struct UpstreamHit {
    path: String,
    model: String,
}

/// 启动一个模拟上游服务，记录每次请求并返回一个最小的 Anthropic Messages 响应。
/// 两个供应商用不同的路径前缀（/a、/b）区分，便于断言路由到了哪一家。
async fn start_mock_upstream(hits: Arc<Mutex<Vec<UpstreamHit>>>) -> u16 {
    use axum::{body::Bytes, http::Uri, response::IntoResponse, Router};

    async fn handler(
        axum::extract::State(hits): axum::extract::State<Arc<Mutex<Vec<UpstreamHit>>>>,
        uri: Uri,
        body: Bytes,
    ) -> impl IntoResponse {
        let model = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("model")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        hits.lock().unwrap().push(UpstreamHit {
            path: uri.path().to_string(),
            model,
        });
        axum::Json(json!({
            "id": "msg_mock",
            "type": "message",
            "role": "assistant",
            "model": "mock-model",
            "content": [{"type": "text", "text": "ok"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }))
    }

    let app = Router::new().fallback(handler).with_state(hits);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve mock upstream");
    });
    port
}

fn claude_provider(id: &str, name: &str, base_url: String, token: &str) -> Provider {
    Provider::with_id(
        id.to_string(),
        name.to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": base_url,
                "ANTHROPIC_AUTH_TOKEN": token
            }
        }),
        None,
    )
}

fn route(id: &str, pattern: &str, provider_id: &str, upstream: Option<&str>) -> ModelRoute {
    ModelRoute {
        id: id.to_string(),
        app_type: "claude".to_string(),
        model_pattern: pattern.to_string(),
        provider_id: provider_id.to_string(),
        provider_name: None,
        upstream_model: upstream.map(|s| s.to_string()),
        sort_index: Some(0),
        enabled: true,
        created_at: Some(0),
    }
}

async fn send_model(client: &reqwest::Client, proxy: &str, model: &str) -> reqwest::StatusCode {
    let body = json!({
        "model": model,
        "max_tokens": 16,
        "messages": [{"role": "user", "content": "hi"}]
    });
    client
        .post(proxy)
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", "client-key")
        .json(&body)
        .send()
        .await
        .expect("send request to proxy")
        .status()
}

// 测试使用 Mutex 进行串行化，跨 await 持锁是预期行为
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn aggregation_routes_requests_by_model_end_to_end() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let hits = Arc::new(Mutex::new(Vec::<UpstreamHit>::new()));
    let mock_port = start_mock_upstream(hits.clone()).await;

    let state = create_test_state().expect("create test state");

    // 两个供应商都指向模拟上游，用 /a、/b 区分
    let a = claude_provider(
        "A",
        "Provider A",
        format!("http://127.0.0.1:{mock_port}/a"),
        "key-a",
    );
    let b = claude_provider(
        "B",
        "Provider B",
        format!("http://127.0.0.1:{mock_port}/b"),
        "key-b",
    );
    state.db.save_provider("claude", &a).expect("save A");
    state.db.save_provider("claude", &b).expect("save B");
    state
        .db
        .set_current_provider("claude", "A")
        .expect("set current A");

    // 启用 claude 代理接管
    let mut cfg = state
        .db
        .get_proxy_config_for_app("claude")
        .await
        .expect("get app config");
    cfg.enabled = true;
    state
        .db
        .update_proxy_config_for_app(cfg)
        .await
        .expect("enable takeover");

    // 代理用临时端口，避免与本机正式实例（15721）冲突
    let mut gc = state
        .db
        .get_global_proxy_config()
        .await
        .expect("get global config");
    gc.listen_port = 0;
    state
        .db
        .update_global_proxy_config(gc)
        .await
        .expect("set ephemeral port");

    let info = state.proxy_service.start().await.expect("start proxy");
    let proxy = format!("http://127.0.0.1:{}/v1/messages", info.port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let last_hit = || hits.lock().unwrap().last().cloned();

    // --- Phase 1: 聚合关闭 → gpt-4o 走当前供应商 A ---
    let st = send_model(&client, &proxy, "gpt-4o").await;
    assert!(st.is_success(), "phase1 status = {st}");
    let h = last_hit().expect("phase1 hit");
    assert!(
        h.path.starts_with("/a"),
        "phase1: aggregation off should use current provider A, got path {}",
        h.path
    );

    // --- Phase 2: 聚合开启 + 路由 gpt-* → B ---
    state
        .db
        .set_aggregation_enabled("claude", true)
        .expect("enable aggregation");
    state
        .db
        .upsert_model_route(&route("r1", "gpt-*", "B", None))
        .expect("add route");

    let st = send_model(&client, &proxy, "gpt-4o").await;
    assert!(st.is_success(), "phase2 status = {st}");
    let h = last_hit().unwrap();
    assert!(
        h.path.starts_with("/b"),
        "phase2: gpt-4o should route to B, got path {}",
        h.path
    );
    assert_eq!(h.model, "gpt-4o", "phase2: model name should be unchanged");

    // 未命中路由 → 回退当前供应商 A
    let st = send_model(&client, &proxy, "claude-sonnet-4").await;
    assert!(st.is_success(), "phase2 fallback status = {st}");
    let h = last_hit().unwrap();
    assert!(
        h.path.starts_with("/a"),
        "phase2: unmatched model should fall back to current A, got path {}",
        h.path
    );

    // --- Phase 3: 命中带 upstream_model 的路由 → 改写上游模型名 ---
    state
        .db
        .upsert_model_route(&route("r1", "gpt-*", "B", Some("up-model-x")))
        .expect("update route with upstream_model");

    let st = send_model(&client, &proxy, "gpt-4o").await;
    assert!(st.is_success(), "phase3 status = {st}");
    let h = last_hit().unwrap();
    assert!(
        h.path.starts_with("/b"),
        "phase3: still routes to B, got path {}",
        h.path
    );
    assert_eq!(
        h.model, "up-model-x",
        "phase3: upstream_model should override the request model name"
    );

    let _ = state.proxy_service.stop().await;
}
