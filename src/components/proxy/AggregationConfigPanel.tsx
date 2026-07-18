/**
 * 模型聚合路由管理组件
 *
 * 允许用户把不同供应商提供的模型聚合到统一的本地代理端点：
 * - 开启/关闭某应用的模型聚合
 * - 维护「模型名 → 供应商」路由表（支持精确匹配、前缀通配 `*`、可选上游模型改写）
 */

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Plus, Trash2, Loader2, Info, Layers } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { AppId } from "@/lib/api";
import type { ModelRoute } from "@/lib/api/aggregation";
import { useProvidersQuery } from "@/lib/query/queries";
import {
  useModelRoutes,
  useAggregationEnabled,
  useSetAggregationEnabled,
  useUpsertModelRoute,
  useDeleteModelRoute,
} from "@/lib/query/aggregation";
import { generateUUID } from "@/utils/uuid";

interface AggregationConfigPanelProps {
  appType: AppId;
  disabled?: boolean;
}

export function AggregationConfigPanel({
  appType,
  disabled = false,
}: AggregationConfigPanelProps) {
  const { t } = useTranslation();

  const { data: isEnabled = false } = useAggregationEnabled(appType);
  const setEnabled = useSetAggregationEnabled();

  const { data: routes = [], isLoading } = useModelRoutes(appType);
  const { data: providersData } = useProvidersQuery(appType);
  const providers = Object.values(providersData?.providers ?? {});

  const upsertRoute = useUpsertModelRoute();
  const deleteRoute = useDeleteModelRoute();

  // 新增路由表单状态
  const [modelPattern, setModelPattern] = useState("");
  const [providerId, setProviderId] = useState("");
  const [upstreamModel, setUpstreamModel] = useState("");

  const handleToggle = async (enabled: boolean) => {
    try {
      await setEnabled.mutateAsync({ appType, enabled });
      toast.success(
        enabled
          ? t("proxy.aggregation.enabled", { defaultValue: "模型聚合已开启" })
          : t("proxy.aggregation.disabled", { defaultValue: "模型聚合已关闭" }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("proxy.aggregation.toggleFailed", {
          defaultValue: "切换模型聚合失败",
        }) +
          ": " +
          String(error),
      );
    }
  };

  const handleAddRoute = async () => {
    const pattern = modelPattern.trim();
    if (!pattern || !providerId) return;

    const route: ModelRoute = {
      id: generateUUID(),
      appType,
      modelPattern: pattern,
      providerId,
      upstreamModel: upstreamModel.trim() || null,
      sortIndex: routes.length,
      enabled: true,
    };

    try {
      await upsertRoute.mutateAsync({ appType, route });
      setModelPattern("");
      setProviderId("");
      setUpstreamModel("");
      toast.success(
        t("proxy.aggregation.addSuccess", { defaultValue: "已添加路由" }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("proxy.aggregation.addFailed", { defaultValue: "添加路由失败" }) +
          ": " +
          String(error),
      );
    }
  };

  const handleToggleRoute = async (route: ModelRoute, enabled: boolean) => {
    try {
      await upsertRoute.mutateAsync({ appType, route: { ...route, enabled } });
    } catch (error) {
      toast.error(
        t("proxy.aggregation.updateFailed", { defaultValue: "更新路由失败" }) +
          ": " +
          String(error),
      );
    }
  };

  const handleDeleteRoute = async (id: string) => {
    try {
      await deleteRoute.mutateAsync({ appType, id });
      toast.success(
        t("proxy.aggregation.deleteSuccess", { defaultValue: "已删除路由" }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("proxy.aggregation.deleteFailed", { defaultValue: "删除路由失败" }) +
          ": " +
          String(error),
      );
    }
  };

  return (
    <div className="space-y-4">
      {/* 开关 */}
      <div className="flex items-center justify-between p-4 rounded-lg bg-muted/50 border border-border/50">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <Layers className="h-4 w-4 text-indigo-500" />
            <span className="text-sm font-medium">
              {t("proxy.aggregation.enableTitle", {
                defaultValue: "模型聚合",
              })}
            </span>
            {isEnabled && (
              <span className="px-2 py-0.5 text-xs rounded-full bg-emerald-500/20 text-emerald-600 dark:text-emerald-400">
                {t("common.enabled", { defaultValue: "已开启" })}
              </span>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            {t("proxy.aggregation.enableDescription", {
              defaultValue:
                "开启后，代理会按请求的模型名把请求路由到对应供应商，从而把不同供应商的模型聚合到一个端点。",
            })}
          </p>
        </div>
        <Switch
          checked={isEnabled}
          onCheckedChange={handleToggle}
          disabled={disabled || setEnabled.isPending}
        />
      </div>

      {/* 说明 */}
      <Alert className="border-indigo-500/40 bg-indigo-500/10">
        <Info className="h-4 w-4" />
        <AlertDescription className="text-sm">
          {t("proxy.aggregation.info", {
            defaultValue:
              "匹配规则：精确匹配（如 gpt-4o）优先，其次是最长前缀通配（如 gpt-*），最后是 * 兜底。未命中路由时回退到当前供应商。",
          })}
        </AlertDescription>
      </Alert>

      {/* 新增路由 */}
      <div className="grid gap-2 sm:grid-cols-[1fr_1fr_1fr_auto] sm:items-center">
        <Input
          value={modelPattern}
          onChange={(e) => setModelPattern(e.target.value)}
          placeholder={t("proxy.aggregation.modelPatternPlaceholder", {
            defaultValue: "模型名 / 通配（如 gpt-*）",
          })}
          disabled={disabled}
        />
        <Select
          value={providerId}
          onValueChange={setProviderId}
          disabled={disabled}
        >
          <SelectTrigger>
            <SelectValue
              placeholder={t("proxy.aggregation.selectProvider", {
                defaultValue: "选择供应商",
              })}
            />
          </SelectTrigger>
          <SelectContent>
            {providers.map((provider) => (
              <SelectItem key={provider.id} value={provider.id}>
                {provider.name}
              </SelectItem>
            ))}
            {providers.length === 0 && (
              <div className="px-2 py-4 text-center text-sm text-muted-foreground">
                {t("proxy.aggregation.noProviders", {
                  defaultValue: "该应用暂无供应商",
                })}
              </div>
            )}
          </SelectContent>
        </Select>
        <Input
          value={upstreamModel}
          onChange={(e) => setUpstreamModel(e.target.value)}
          placeholder={t("proxy.aggregation.upstreamModelPlaceholder", {
            defaultValue: "上游模型名（可选）",
          })}
          disabled={disabled}
        />
        <Button
          onClick={handleAddRoute}
          disabled={
            disabled ||
            !modelPattern.trim() ||
            !providerId ||
            upsertRoute.isPending
          }
          size="icon"
          variant="outline"
        >
          {upsertRoute.isPending ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Plus className="h-4 w-4" />
          )}
        </Button>
      </div>

      {/* 路由列表 */}
      {isLoading ? (
        <div className="flex items-center justify-center p-8">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </div>
      ) : routes.length === 0 ? (
        <div className="rounded-lg border border-dashed border-muted-foreground/40 p-8 text-center">
          <p className="text-sm text-muted-foreground">
            {t("proxy.aggregation.empty", {
              defaultValue: "暂无路由。添加「模型名 → 供应商」映射以启用聚合。",
            })}
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {routes.map((route) => (
            <RouteItem
              key={route.id}
              route={route}
              disabled={disabled}
              onToggle={handleToggleRoute}
              onDelete={handleDeleteRoute}
              isMutating={upsertRoute.isPending || deleteRoute.isPending}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface RouteItemProps {
  route: ModelRoute;
  disabled: boolean;
  onToggle: (route: ModelRoute, enabled: boolean) => void;
  onDelete: (id: string) => void;
  isMutating: boolean;
}

function RouteItem({
  route,
  disabled,
  onToggle,
  onDelete,
  isMutating,
}: RouteItemProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center gap-3 rounded-lg border bg-card p-3">
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 text-sm">
          <code className="rounded bg-muted px-1.5 py-0.5 text-xs font-medium">
            {route.modelPattern}
          </code>
          <span className="text-muted-foreground">→</span>
          <span className="font-medium truncate">
            {route.providerName ?? route.providerId}
          </span>
        </div>
        {route.upstreamModel && (
          <p className="mt-1 text-xs text-muted-foreground truncate">
            {t("proxy.aggregation.upstreamLabel", {
              defaultValue: "上游模型",
            })}
            : {route.upstreamModel}
          </p>
        )}
      </div>

      <Switch
        checked={route.enabled}
        onCheckedChange={(checked) => onToggle(route, checked)}
        disabled={disabled || isMutating}
        aria-label={t("common.enabled", { defaultValue: "已开启" })}
      />

      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 text-muted-foreground hover:text-destructive"
        onClick={() => onDelete(route.id)}
        disabled={disabled || isMutating}
        aria-label={t("common.delete", { defaultValue: "删除" })}
      >
        <Trash2 className="h-4 w-4" />
      </Button>
    </div>
  );
}
