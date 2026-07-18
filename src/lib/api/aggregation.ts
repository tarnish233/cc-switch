import { invoke } from "@tauri-apps/api/core";

/**
 * 模型聚合路由：把「客户端请求的模型名」映射到某个供应商。
 * 字段与后端 `ModelRoute`（serde camelCase）一一对应。
 */
export interface ModelRoute {
  id: string;
  appType: string;
  /**
   * 客户端请求中的模型名匹配模式：
   * - 精确匹配（如 `gpt-4o`）
   * - 前缀通配（如 `gpt-*`）
   * - `*` 兜底匹配任意模型
   */
  modelPattern: string;
  /** 目标供应商 ID */
  providerId: string;
  /** 目标供应商名称（后端 join 填充，仅用于展示） */
  providerName?: string | null;
  /** 可选：转发前把请求模型名改写为该上游模型名 */
  upstreamModel?: string | null;
  /** 优先级（越小越优先） */
  sortIndex?: number | null;
  enabled: boolean;
  createdAt?: number | null;
}

export const aggregationApi = {
  /** 列出某应用的模型聚合路由 */
  async listModelRoutes(appType: string): Promise<ModelRoute[]> {
    return invoke("list_model_routes", { appType });
  },

  /** 新增或更新一条模型聚合路由 */
  async upsertModelRoute(appType: string, route: ModelRoute): Promise<void> {
    return invoke("upsert_model_route", { appType, route });
  },

  /** 删除一条模型聚合路由 */
  async deleteModelRoute(appType: string, id: string): Promise<void> {
    return invoke("delete_model_route", { appType, id });
  },

  /** 清空某应用的模型聚合路由 */
  async clearModelRoutes(appType: string): Promise<void> {
    return invoke("clear_model_routes", { appType });
  },

  /** 获取某应用的「模型聚合」开关状态 */
  async getAggregationEnabled(appType: string): Promise<boolean> {
    return invoke("get_aggregation_enabled", { appType });
  },

  /** 设置某应用的「模型聚合」开关状态 */
  async setAggregationEnabled(
    appType: string,
    enabled: boolean,
  ): Promise<void> {
    return invoke("set_aggregation_enabled", { appType, enabled });
  },
};
