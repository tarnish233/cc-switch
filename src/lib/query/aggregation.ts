import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { aggregationApi, type ModelRoute } from "@/lib/api/aggregation";

/**
 * 获取某应用的模型聚合路由列表
 */
export function useModelRoutes(appType: string) {
  return useQuery({
    queryKey: ["modelRoutes", appType],
    queryFn: () => aggregationApi.listModelRoutes(appType),
    enabled: !!appType,
  });
}

/**
 * 获取某应用的「模型聚合」开关状态
 */
export function useAggregationEnabled(appType: string) {
  return useQuery({
    queryKey: ["aggregationEnabled", appType],
    queryFn: () => aggregationApi.getAggregationEnabled(appType),
    enabled: !!appType,
  });
}

/**
 * 设置「模型聚合」开关状态
 */
export function useSetAggregationEnabled() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ appType, enabled }: { appType: string; enabled: boolean }) =>
      aggregationApi.setAggregationEnabled(appType, enabled),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["aggregationEnabled", variables.appType],
      });
    },
  });
}

/**
 * 新增/更新模型聚合路由
 */
export function useUpsertModelRoute() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ appType, route }: { appType: string; route: ModelRoute }) =>
      aggregationApi.upsertModelRoute(appType, route),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["modelRoutes", variables.appType],
      });
    },
  });
}

/**
 * 删除模型聚合路由
 */
export function useDeleteModelRoute() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ appType, id }: { appType: string; id: string }) =>
      aggregationApi.deleteModelRoute(appType, id),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["modelRoutes", variables.appType],
      });
    },
  });
}

/**
 * 清空某应用的全部模型聚合路由
 */
export function useClearModelRoutes() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (appType: string) => aggregationApi.clearModelRoutes(appType),
    onSuccess: (_, appType) => {
      queryClient.invalidateQueries({ queryKey: ["modelRoutes", appType] });
    },
  });
}
