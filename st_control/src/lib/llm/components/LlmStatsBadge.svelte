<!-- 大模型统计徽标：提供方 / 模型计数，数据来自全局共享 llmStore，自动实时更新 -->
<script lang="ts">
  import { llmStore } from "../store.svelte";
  import { Badge } from "../../components/ui/badge";

  const enabledProviders = $derived(llmStore.config.providers.filter((p) => p.enabled).length);
  const totalModels = $derived(
    llmStore.config.providers.reduce((s, p) => s + (p.models?.length ?? 0), 0),
  );
</script>

<Badge variant="secondary" class="h-6 gap-1.5 px-2.5 text-xs">
  {enabledProviders} 个提供方 · {totalModels} 个模型
</Badge>
