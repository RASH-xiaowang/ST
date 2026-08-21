<!--
  提供方 / 模型 联动选择器（共享组件）
  数据源：全局 llmStore —— 大模型管理里任何配置变更都会实时反映到这里。
  行为：切换提供方时自动把模型重置为该提供方默认模型；支持占位文案、模型能力标签后缀。
-->
<script lang="ts">
  import { llmStore } from "../store.svelte";
  import { NativeSelect, NativeSelectOption } from "../../components/ui/native-select";

  let {
    providerId = $bindable(""),
    model = $bindable(""),
    enabledOnly = false,
    providerPlaceholder = "选择提供方",
    modelPlaceholder = "选择模型",
    providerClass = "",
    modelClass = "",
    layout = "row",
    onProviderChange = () => {},
    optionSuffix = () => "",
  }: {
    providerId?: string;
    model?: string;
    /** 是否只显示已启用的提供方（默认显示全部，与原有各面板行为一致） */
    enabledOnly?: boolean;
    providerPlaceholder?: string;
    modelPlaceholder?: string;
    providerClass?: string;
    modelClass?: string;
    /** row：两个下拉并排；grid：两列等宽 */
    layout?: "row" | "grid";
    onProviderChange?: (providerId: string) => void;
    /** 模型选项的后缀（如能力标签），可选 */
    optionSuffix?: (model: string) => string;
  } = $props();

  const providers = $derived(
    llmStore.config.providers.filter((p) => (enabledOnly ? p.enabled : true)),
  );
  const current = $derived(providers.find((p) => p.id === providerId) ?? null);
  const currentModels = $derived(current?.models ?? []);

  function handleProviderChange() {
    const p = providers.find((x) => x.id === providerId);
    model = p?.default_model ?? "";
    onProviderChange(providerId);
  }
</script>

<span class="ms-root {layout === 'grid' ? 'ms-grid' : ''}">
  <NativeSelect class={providerClass} bind:value={providerId} onchange={handleProviderChange}>
    <NativeSelectOption value="">{providerPlaceholder}</NativeSelectOption>
    {#each providers as p (p.id)}
      <NativeSelectOption value={p.id}>
        {p.name}{llmStore.config.default_provider_id === p.id ? "（默认）" : ""}
      </NativeSelectOption>
    {/each}
  </NativeSelect>

  <NativeSelect class={modelClass} bind:value={model} disabled={!current}>
    {#if current && currentModels.length > 0}
      {#each currentModels as m (m)}
        <NativeSelectOption value={m}>{m}{optionSuffix(m)}</NativeSelectOption>
      {/each}
    {:else}
      <NativeSelectOption value="">{modelPlaceholder}</NativeSelectOption>
    {/if}
    {#if model && !currentModels.includes(model)}
      <NativeSelectOption value={model}>{model}{optionSuffix(model)}</NativeSelectOption>
    {/if}
  </NativeSelect>
</span>

<style>
  .ms-root {
    display: inline-flex;
    gap: 10px;
    min-width: 0;
  }
  .ms-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    width: 100%;
  }
</style>
