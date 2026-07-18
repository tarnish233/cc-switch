//! 模型聚合路由命令
//!
//! 管理「模型聚合」功能：把不同供应商提供的模型聚合到统一的本地代理端点，
//! 代理按请求体中的模型名把请求路由到对应供应商。路由表按 `app_type` 分组。

use crate::database::ModelRoute;
use crate::store::AppState;

/// 列出某应用的模型聚合路由
#[tauri::command]
pub async fn list_model_routes(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Vec<ModelRoute>, String> {
    state
        .db
        .list_model_routes(&app_type)
        .map_err(|e| e.to_string())
}

/// 新增或更新一条模型聚合路由
#[tauri::command]
pub async fn upsert_model_route(
    state: tauri::State<'_, AppState>,
    app_type: String,
    route: ModelRoute,
) -> Result<(), String> {
    // 强制路由归属于 URL/命令携带的 app_type，避免前端传入不一致
    let mut route = route;
    route.app_type = app_type.clone();

    // 校验目标供应商存在
    let exists = state
        .db
        .get_provider_by_id(&route.provider_id, &app_type)
        .map_err(|e| e.to_string())?
        .is_some();
    if !exists {
        return Err(format!("目标供应商不存在: {}", route.provider_id));
    }

    // 新建路由补充创建时间
    if route.created_at.is_none() {
        route.created_at = Some(chrono::Utc::now().timestamp_millis());
    }

    state
        .db
        .upsert_model_route(&route)
        .map_err(|e| e.to_string())
}

/// 删除一条模型聚合路由
#[tauri::command]
pub async fn delete_model_route(
    state: tauri::State<'_, AppState>,
    app_type: String,
    id: String,
) -> Result<(), String> {
    state
        .db
        .delete_model_route(&app_type, &id)
        .map_err(|e| e.to_string())
}

/// 清空某应用的模型聚合路由
#[tauri::command]
pub async fn clear_model_routes(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<(), String> {
    state
        .db
        .clear_model_routes(&app_type)
        .map_err(|e| e.to_string())
}

/// 获取某应用的「模型聚合」开关状态
#[tauri::command]
pub async fn get_aggregation_enabled(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<bool, String> {
    state
        .db
        .get_aggregation_enabled(&app_type)
        .map_err(|e| e.to_string())
}

/// 设置某应用的「模型聚合」开关状态
///
/// 开启时要求该应用已启用代理接管（与自动故障转移一致）：未接管时聚合路由不会生效。
#[tauri::command]
pub async fn set_aggregation_enabled(
    state: tauri::State<'_, AppState>,
    app_type: String,
    enabled: bool,
) -> Result<(), String> {
    if enabled {
        let config = state
            .db
            .get_proxy_config_for_app(&app_type)
            .await
            .map_err(|e| e.to_string())?;
        if !config.enabled {
            return Err("需要先启用该应用的代理接管，再开启模型聚合".to_string());
        }
    }

    log::info!("[Aggregation] set enabled: app_type='{app_type}', enabled={enabled}");

    state
        .db
        .set_aggregation_enabled(&app_type, enabled)
        .map_err(|e| e.to_string())
}
