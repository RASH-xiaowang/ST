<script lang="ts">
  import { onMount } from "svelte";
  import { llmApi } from "./services/ipc";
  import { llmStore, refreshLlmConfig } from "./store.svelte";
  import ProviderConfigTab from "./components/ProviderConfigTab.svelte";
  import ModelManagerTab from "./components/ModelManagerTab.svelte";
  import UsageCostTab from "./components/UsageCostTab.svelte";
  import AiRolesPanel from "./components/AiRolesPanel.svelte";
  import AiCopyPanel from "../copywriting/AiCopyPanel.svelte";
  import LlmStatsBadge from "./components/LlmStatsBadge.svelte";
  import PanelHeader from "../components/PanelHeader.svelte";
  import { Tabs, TabsList, TabsTrigger, TabsContent } from "../components/ui/tabs";
  import BrainCircuitIcon from "@lucide/svelte/icons/brain-circuit";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import { RippleButton } from "fancy-ui-svelte";

  // 说明：全局调用已与侧边栏「AI 聊天」合并（同一 GlobalChatTab 组件），
  // 大模型管理不再重复提供聊天入口，避免双入口。
  // AI 角色与 AI 文案并入本面板（原独立侧边栏入口），与提供方/模型/用量同属 AI 能力配置。
  type Tab = "config" | "models" | "usage" | "roles" | "copy";
  const TABS: Array<{ id: Tab; label: string }> = [
    { id: "usage", label: "流量与成本" },
    { id: "config", label: "接入配置" },
    { id: "models", label: "模型管理" },
    { id: "roles", label: "AI 角色" },
    { id: "copy", label: "AI 文案" },
  ];

  let curTab = $state<Tab>("usage");
  let configPath = $state("");
  const config = $derived(llmStore.config);
  const loading = $derived(llmStore.loading);
  const loadError = $derived(llmStore.error);

  async function loadConfig() {
    try {
      await refreshLlmConfig();
      configPath = await llmApi.getConfigPath();
    } catch { /* 错误信息已写入 llmStore.error */ }
  }

  async function reloadConfig() {
    await loadConfig();
  }

  onMount(() => {
    loadConfig();
  });
</script>

<div class="llm-root">
  {#snippet headIcon()}
    <BrainCircuitIcon class="size-4.5" />
  {/snippet}
  {#snippet headBadge()}
    <LlmStatsBadge />
  {/snippet}
  {#snippet headActions()}
    <span class="hidden max-w-xs truncate font-mono text-xs text-muted-foreground xl:inline" title={configPath}>
      {configPath ? configPath.split(/[\\/]/).pop() : '—'}
    </span>
    <RippleButton
      onclick={reloadConfig}
      disabled={loading}
      rippleColor="#22d3ee"
      class="h-8 rounded-md border border-[var(--border)] bg-[var(--card)] px-3 text-xs font-medium text-[var(--foreground)] hover:bg-[var(--muted)]"
    >
      <RefreshCwIcon class="size-3.5 {loading ? 'animate-spin' : ''}" />
      {loading ? "加载中…" : "刷新"}
    </RippleButton>
  {/snippet}
  <PanelHeader title="大模型管理" icon={headIcon} badge={headBadge} actions={headActions} />

  <Tabs bind:value={curTab} class="llm-tabs">
    <TabsList>
      {#each TABS as t}
        <TabsTrigger value={t.id}>{t.label}</TabsTrigger>
      {/each}
    </TabsList>
    <TabsContent value="config" class="llm-content">
      {#if loadError}
        <div class="rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">{loadError}</div>
      {:else if loading}
        <div class="py-10 text-center text-sm text-muted-foreground">大模型配置加载中…</div>
      {:else}
        <ProviderConfigTab {config} onConfigChange={reloadConfig} />
      {/if}
    </TabsContent>
    <TabsContent value="models" class="llm-content">
      {#if loadError}
        <div class="rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">{loadError}</div>
      {:else if loading}
        <div class="py-10 text-center text-sm text-muted-foreground">大模型配置加载中…</div>
      {:else}
        <ModelManagerTab {config} onConfigChange={reloadConfig} />
      {/if}
    </TabsContent>
    <TabsContent value="usage" class="llm-content">
      <UsageCostTab />
    </TabsContent>
    <TabsContent value="roles" class="llm-content">
      <AiRolesPanel />
    </TabsContent>
    <TabsContent value="copy" class="llm-content">
      <AiCopyPanel embedded />
    </TabsContent>
  </Tabs>
</div>

<style>
  .llm-root {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  :global(.llm-tabs) {
    padding: 10px 20px 0;
    /* tabs 容器占满面板剩余高度，llm-content 的 flex:1 才能生效，
       否则空状态下方会出现大段留白（实测 710px 空白） */
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  :global(.llm-content) {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 20px 20px;
  }
</style>
