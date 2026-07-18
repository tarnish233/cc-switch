//! 模型聚合路由 DAO
//!
//! 「模型聚合」功能：把不同供应商提供的模型聚合到统一的本地代理端点，
//! 代理按请求体中的模型名把请求路由到对应供应商（可选改写上游模型名）。
//!
//! 路由表按 `app_type`（客户端协议：claude/codex/gemini/grokbuild）分组，
//! 与其余代理设施（适配器、熔断器、故障转移队列）保持同一维度。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use serde::{Deserialize, Serialize};

/// 单条模型路由：把「客户端请求的模型名」映射到某个供应商。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRoute {
    /// 路由 ID
    pub id: String,
    /// 应用类型（claude/codex/gemini/grokbuild）
    pub app_type: String,
    /// 客户端请求中的模型名匹配模式：
    /// - 精确匹配（如 `gpt-4o`）
    /// - 前缀通配（如 `gpt-*`，匹配所有以 `gpt-` 开头的模型）
    /// - `*` 兜底匹配任意模型
    ///
    /// 匹配大小写不敏感；精确匹配优先于通配匹配。
    pub model_pattern: String,
    /// 目标供应商 ID
    pub provider_id: String,
    /// 目标供应商名称（冗余字段，由 join 填充，仅用于前端展示）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,
    /// 可选：转发前把请求模型名改写为该上游模型名。
    /// 为空时保持客户端请求的模型名不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
    /// 优先级（越小越优先）。用于同一模型命中多条通配规则时的排序。
    #[serde(default)]
    pub sort_index: Option<i64>,
    /// 是否启用该路由
    pub enabled: bool,
    /// 创建时间（Unix 毫秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

/// 判断 `model` 是否匹配 `pattern`（大小写不敏感）。
///
/// 返回匹配的「特异度」：数字越大越精确，`None` 表示不匹配。
/// - 精确匹配：`usize::MAX`
/// - 前缀通配（`foo*`）：匹配到的前缀长度
/// - 全兜底（`*`）：0
fn match_specificity(pattern: &str, model: &str) -> Option<usize> {
    let pattern_lc = pattern.trim().to_lowercase();
    let model_lc = model.trim().to_lowercase();

    if pattern_lc == "*" {
        return Some(0);
    }
    if let Some(prefix) = pattern_lc.strip_suffix('*') {
        if model_lc.starts_with(prefix) {
            return Some(prefix.len());
        }
        return None;
    }
    if pattern_lc == model_lc {
        return Some(usize::MAX);
    }
    None
}

impl Database {
    /// 列出某应用的全部模型路由（含供应商名称，按优先级排序）。
    pub fn list_model_routes(&self, app_type: &str) -> Result<Vec<ModelRoute>, AppError> {
        let conn = lock_conn!(self.conn);

        let mut stmt = conn
            .prepare(
                "SELECT r.id, r.app_type, r.model_pattern, r.provider_id, p.name,
                        r.upstream_model, r.sort_index, r.enabled, r.created_at
                 FROM model_routes r
                 LEFT JOIN providers p
                   ON p.id = r.provider_id AND p.app_type = r.app_type
                 WHERE r.app_type = ?1
                 ORDER BY COALESCE(r.sort_index, 999999), r.created_at ASC, r.id ASC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let routes = stmt
            .query_map([app_type], |row| {
                Ok(ModelRoute {
                    id: row.get(0)?,
                    app_type: row.get(1)?,
                    model_pattern: row.get(2)?,
                    provider_id: row.get(3)?,
                    provider_name: row.get(4)?,
                    upstream_model: row.get(5)?,
                    sort_index: row.get(6)?,
                    enabled: row.get::<_, i32>(7)? != 0,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(routes)
    }

    /// 新增或更新一条模型路由（按 (id, app_type) UPSERT）。
    pub fn upsert_model_route(&self, route: &ModelRoute) -> Result<(), AppError> {
        let model_pattern = route.model_pattern.trim();
        if model_pattern.is_empty() {
            return Err(AppError::Config("模型匹配模式不能为空".to_string()));
        }
        if route.provider_id.trim().is_empty() {
            return Err(AppError::Config("目标供应商不能为空".to_string()));
        }

        let upstream_model = route
            .upstream_model
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO model_routes
                (id, app_type, model_pattern, provider_id, upstream_model, sort_index, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id, app_type) DO UPDATE SET
                model_pattern = excluded.model_pattern,
                provider_id   = excluded.provider_id,
                upstream_model = excluded.upstream_model,
                sort_index    = excluded.sort_index,
                enabled       = excluded.enabled",
            rusqlite::params![
                route.id,
                route.app_type,
                model_pattern,
                route.provider_id,
                upstream_model,
                route.sort_index,
                route.enabled as i32,
                route.created_at,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// 删除一条模型路由。
    pub fn delete_model_route(&self, app_type: &str, id: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM model_routes WHERE id = ?1 AND app_type = ?2",
            rusqlite::params![id, app_type],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 清空某应用的全部模型路由。
    pub fn clear_model_routes(&self, app_type: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute("DELETE FROM model_routes WHERE app_type = ?1", [app_type])
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 为请求的模型名查找最匹配的启用路由。
    ///
    /// 匹配规则：精确匹配优先，其次是最长前缀通配，最后是 `*` 兜底；
    /// 特异度相同时按 `sort_index` 排序取第一个。
    pub fn find_model_route(
        &self,
        app_type: &str,
        model: &str,
    ) -> Result<Option<ModelRoute>, AppError> {
        let routes = self.list_model_routes(app_type)?;

        let best = routes
            .into_iter()
            .filter(|r| r.enabled)
            .filter_map(|r| match_specificity(&r.model_pattern, model).map(|spec| (spec, r)))
            // list_model_routes 已按 sort_index 升序返回；这里用稳定的 max_by
            // 只比较特异度，特异度相同则保留更靠前（更高优先级）的那条。
            .fold(
                None::<(usize, ModelRoute)>,
                |acc, (spec, route)| match acc {
                    Some((best_spec, _)) if best_spec >= spec => acc,
                    _ => Some((spec, route)),
                },
            );

        Ok(best.map(|(_, route)| route))
    }

    /// 读取某应用的「模型聚合」开关（存于 settings 表）。
    pub fn get_aggregation_enabled(&self, app_type: &str) -> Result<bool, AppError> {
        let key = format!("aggregation_enabled_{app_type}");
        Ok(self
            .get_setting(&key)?
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false))
    }

    /// 设置某应用的「模型聚合」开关。
    pub fn set_aggregation_enabled(&self, app_type: &str, enabled: bool) -> Result<(), AppError> {
        let key = format!("aggregation_enabled_{app_type}");
        self.set_setting(&key, if enabled { "true" } else { "false" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_db() -> Database {
        Database::memory().expect("in-memory db")
    }

    fn route(id: &str, pattern: &str, provider: &str, sort: i64) -> ModelRoute {
        ModelRoute {
            id: id.to_string(),
            app_type: "claude".to_string(),
            model_pattern: pattern.to_string(),
            provider_id: provider.to_string(),
            provider_name: None,
            upstream_model: None,
            sort_index: Some(sort),
            enabled: true,
            created_at: Some(0),
        }
    }

    #[test]
    fn upsert_list_delete_roundtrip() {
        let db = make_db();
        db.upsert_model_route(&route("r1", "gpt-4o", "p-openai", 0))
            .unwrap();
        db.upsert_model_route(&route("r2", "claude-*", "p-claude", 1))
            .unwrap();

        let routes = db.list_model_routes("claude").unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].id, "r1");

        // update in place
        let mut updated = route("r1", "gpt-4o-mini", "p-openai", 0);
        updated.enabled = false;
        db.upsert_model_route(&updated).unwrap();
        let routes = db.list_model_routes("claude").unwrap();
        assert_eq!(routes.len(), 2);
        let r1 = routes.iter().find(|r| r.id == "r1").unwrap();
        assert_eq!(r1.model_pattern, "gpt-4o-mini");
        assert!(!r1.enabled);

        db.delete_model_route("claude", "r1").unwrap();
        assert_eq!(db.list_model_routes("claude").unwrap().len(), 1);

        db.clear_model_routes("claude").unwrap();
        assert!(db.list_model_routes("claude").unwrap().is_empty());
    }

    #[test]
    fn exact_match_beats_wildcard() {
        let db = make_db();
        db.upsert_model_route(&route("wild", "*", "p-default", 0))
            .unwrap();
        db.upsert_model_route(&route("prefix", "gpt-*", "p-openai", 1))
            .unwrap();
        db.upsert_model_route(&route("exact", "gpt-4o", "p-exact", 2))
            .unwrap();

        let hit = db.find_model_route("claude", "gpt-4o").unwrap().unwrap();
        assert_eq!(hit.provider_id, "p-exact");
    }

    #[test]
    fn longest_prefix_wins() {
        let db = make_db();
        db.upsert_model_route(&route("short", "gpt-*", "p-short", 0))
            .unwrap();
        db.upsert_model_route(&route("long", "gpt-4*", "p-long", 1))
            .unwrap();

        let hit = db.find_model_route("claude", "gpt-4o").unwrap().unwrap();
        assert_eq!(hit.provider_id, "p-long");
    }

    #[test]
    fn wildcard_fallback_and_case_insensitive() {
        let db = make_db();
        db.upsert_model_route(&route("wild", "*", "p-default", 0))
            .unwrap();
        let hit = db
            .find_model_route("claude", "Some-Random-Model")
            .unwrap()
            .unwrap();
        assert_eq!(hit.provider_id, "p-default");

        db.clear_model_routes("claude").unwrap();
        db.upsert_model_route(&route("exact", "GPT-4O", "p-openai", 0))
            .unwrap();
        let hit = db.find_model_route("claude", "gpt-4o").unwrap().unwrap();
        assert_eq!(hit.provider_id, "p-openai");
    }

    #[test]
    fn disabled_routes_are_ignored() {
        let db = make_db();
        let mut r = route("r1", "gpt-4o", "p-openai", 0);
        r.enabled = false;
        db.upsert_model_route(&r).unwrap();
        assert!(db.find_model_route("claude", "gpt-4o").unwrap().is_none());
    }

    #[test]
    fn no_match_returns_none() {
        let db = make_db();
        db.upsert_model_route(&route("r1", "gpt-4o", "p-openai", 0))
            .unwrap();
        assert!(db
            .find_model_route("claude", "claude-sonnet-4")
            .unwrap()
            .is_none());
    }

    #[test]
    fn deleting_provider_cascades_to_its_routes() {
        use crate::provider::Provider;
        let db = make_db();

        // 保存一个供应商并为它建立路由
        let provider = Provider::with_id(
            "p-openai".to_string(),
            "OpenAI".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.upsert_model_route(&route("r1", "gpt-*", "p-openai", 0))
            .unwrap();
        // 另一个供应商的路由不应被影响
        db.upsert_model_route(&route("r2", "claude-*", "p-other", 1))
            .unwrap();
        assert_eq!(db.list_model_routes("claude").unwrap().len(), 2);

        // 删除供应商应级联删除其路由，保留其它路由
        db.delete_provider("claude", "p-openai").unwrap();
        let remaining = db.list_model_routes("claude").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "r2");
    }
}
