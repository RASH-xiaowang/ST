<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { toast } from 'svelte-sonner';
  import { open } from '@tauri-apps/plugin-dialog';
  import { ocrApi } from './services/ipc';
  import type { OcrConfig, OcrResource, OcrStats } from './types';
  import {
    CATEGORY_ORDER,
    COMMON_ENDPOINTS,
    STATUS_META,
    catLabel,
    prettyJson,
    statusCls,
    statusLabel,
  } from './display';
  import { Button } from '../components/ui/button';
  import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../components/ui/card';
  import { RippleButton } from 'fancy-ui-svelte';
  import LiveNumber from '../components/fancy/LiveNumber.svelte';
  import ListChecksIcon from '@lucide/svelte/icons/list-checks';
  import GitBranchIcon from '@lucide/svelte/icons/git-branch';
  import SlidersHorizontalIcon from '@lucide/svelte/icons/sliders-horizontal';
  import BookOpenTextIcon from '@lucide/svelte/icons/book-open-text';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import SaveIcon from '@lucide/svelte/icons/save';
  import { Input } from '../components/ui/input';
  import { Label } from '../components/ui/label';
  import { Switch } from '../components/ui/switch';
  import { Badge } from '../components/ui/badge';
  import { Tabs, TabsList, TabsTrigger, TabsContent } from '../components/ui/tabs';
  import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../components/ui/table';
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
  } from '../components/ui/dialog';
  import { NativeSelect, NativeSelectOption } from '../components/ui/native-select';
  import { Root as SelectRoot } from '../components/ui/select';
  import {
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from '../components/ui/select';
  import { Root as AlertDialogRoot } from '../components/ui/alert-dialog';
  import {
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
  } from '../components/ui/alert-dialog';

  type View = 'list' | 'mapping' | 'config' | 'docs';

  const TEST_IMAGES = [
    { name: '测试图 1', file: 'b08e6cd1bd7c98a0fb9b16b3f2b063a9.jpg' },
    { name: '测试图 2', file: 'd1278caa986d84c4baaba1aeeeda0ee0.jpg' },
    { name: '测试图 3', file: 'e29a19a474a625e874b823ffb093cb2b.jpg' },
    { name: '测试图 4', file: '9d9ebf156d109a7bb0000c0abce489d3.jpg' },
  ];

  const DEFAULT_CFG: OcrConfig = {
    appId: '', secretCode: '', enabled: true, bindHost: '0.0.0.0', port: 9787, token: '',
    precheckEnabled: true, precheckMinChars: 2, precheckModelDir: '', endpointMap: {},
  };

  let cfg = $state<OcrConfig>({ ...DEFAULT_CFG });
  let cfgDirty = $state(false);
  let saving = $state(false);

  let stats = $state<OcrStats | null>(null);
  let items = $state<OcrResource[]>([]);
  let total = $state(0);
  let page = $state(1);
  const PAGE_SIZE = 20;
  let filterStatus = $state('');
  let filterCategory = $state('');
  let keyword = $state('');
  let loading = $state(false);

  let detail = $state<OcrResource | null>(null);
  let detailOpen = $state(false);
  let detailTab = $state<'fields' | 'classify' | 'ocr' | 'precheck'>('fields');
  let delTarget = $state<OcrResource | null>(null);
  let view = $state<View>('list');
  let testIndex = $state(0);

  let unlisten: (() => void) | null = null;

  async function loadConfig() {
    try {
      cfg = await ocrApi.getConfig();
      cfgDirty = false;
    } catch (e) {
      toast.error(`加载配置失败: ${e}`);
    }
  }

  async function saveConfig() {
    saving = true;
    try {
      await ocrApi.setConfig(cfg);
      cfgDirty = false;
      toast.success(cfg.enabled ? `配置已保存，接收服务监听 ${cfg.bindHost}:${cfg.port}` : '配置已保存，接收服务已停用');
    } catch (e) {
      toast.error(`保存配置失败: ${e}`);
    } finally {
      saving = false;
    }
  }

  async function loadStats() {
    try {
      stats = await ocrApi.getStats();
    } catch {
      stats = null;
    }
  }

  async function loadList() {
    loading = true;
    try {
      const r = await ocrApi.listResources({
        page,
        pageSize: PAGE_SIZE,
        status: filterStatus || null,
        category: filterCategory || null,
        keyword: keyword || null,
      });
      items = r.items;
      total = r.total;
    } catch (e) {
      console.error('加载资源列表失败', e);
      items = [];
      total = 0;
    } finally {
      loading = false;
    }
  }

  function refreshAll() {
    loadStats();
    loadList();
  }

  async function simulate() {
    try {
      const id = await ocrApi.simulateTest(testIndex);
      toast.success(`已提交模拟测试 #${id}（${TEST_IMAGES[testIndex].name}）`);
      refreshAll();
    } catch (e) {
      toast.error(`模拟测试失败: ${e}`);
    }
  }

  // ── 本地批量导入 ──
  let importing = $state(false);
  async function importLocalImages() {
    if (importing) return;
    try {
      const picked = await open({
        multiple: true,
        directory: false,
        filters: [
          { name: '图片', extensions: ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'gif', 'tif', 'tiff'] },
        ],
      });
      const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
      if (!paths.length) return;
      importing = true;
      const n = await ocrApi.ingestLocalFiles(paths as string[]);
      toast.success(`已导入 ${n} 张图片，进入识别管线`);
      refreshAll();
    } catch (e) {
      toast.error(`导入失败: ${e}`);
    } finally {
      importing = false;
    }
  }

  // ── 导出 CSV ──
  let exportingCsv = $state(false);
  async function exportCsv() {
    if (exportingCsv) return;
    exportingCsv = true;
    try {
      const r = await ocrApi.exportCsv();
      toast.success(`已导出 ${r.count} 条 → ${r.filename}`);
    } catch (e) {
      toast.error(`导出失败: ${e}`);
    } finally {
      exportingCsv = false;
    }
  }

  // ── 人工校对识别字段 ──
  let editFields = $state('');
  let savingFields = $state(false);
  async function saveFields() {
    if (!detail || savingFields) return;
    savingFields = true;
    try {
      await ocrApi.updateResourceFields(detail.id, editFields);
      detail.ocrFields = editFields;
      toast.success('校对已保存');
      refreshAll();
    } catch (e) {
      toast.error(`保存失败: ${e}`);
    } finally {
      savingFields = false;
    }
  }

  // 打开详情时同步校对草稿
  $effect(() => {
    if (detail) editFields = detail.ocrFields || '';
  });

  async function openDetail(id: number) {
    try {
      detail = await ocrApi.getResource(id);
      detailTab = 'fields';
      detailOpen = true;
    } catch (e) {
      toast.error(`查看详情失败: ${e}`);
    }
  }

  async function retry(id: number) {
    try {
      await ocrApi.retryResource(id);
      toast.info(`资源 #${id} 已重新进入处理管线`);
      refreshAll();
    } catch (e) {
      toast.error(`重试失败: ${e}`);
    }
  }

  async function remove(id: number) {
    try {
      await ocrApi.deleteResource(id);
      if (detail?.id === id) detailOpen = false;
      toast.success(`资源 #${id} 已删除`);
      refreshAll();
    } catch (e) {
      toast.error(`删除失败: ${e}`);
    }
  }

  async function confirmDelete() {
    if (!delTarget) return;
    const id = delTarget.id;
    delTarget = null;
    await remove(id);
  }

  function applyFilter() {
    page = 1;
    loadList();
  }

  function goPage(p: number) {
    if (p < 1 || p > Math.max(1, Math.ceil(total / PAGE_SIZE))) return;
    page = p;
    loadList();
  }

  const totalPages = $derived(Math.max(1, Math.ceil(total / PAGE_SIZE)));
  const curlExample = $derived(
    `curl -X POST http://127.0.0.1:${cfg.port}/api/ocr/ingest \\\n` +
      `  -H "Content-Type: application/json" \\\n` +
      `  -d '{"sender_username":"user1","session_type":"group","timestamp":"2026-08-04 12:00:00","username":"张三","mediaUrl":"https://example.com/idcard.jpg"}'`
  );

  const endpointOrder = $derived([
    ...CATEGORY_ORDER.filter((c) => cfg.endpointMap[c]),
    ...Object.keys(cfg.endpointMap).filter((c) => !CATEGORY_ORDER.includes(c)),
  ]);

  function resetEndpoint(cat: string) {
    const rule = cfg.endpointMap[cat];
    if (rule) {
      rule.endpoint = '';
      rule.enabled = true;
      cfgDirty = true;
    }
  }

  function resetAllEndpoints() {
    for (const c of Object.keys(cfg.endpointMap)) {
      cfg.endpointMap[c].endpoint = '';
      cfg.endpointMap[c].enabled = true;
    }
    cfgDirty = true;
  }

  function switchView(v: View) {
    view = v;
    if (v === 'list') refreshAll();
  }

  onMount(() => {
    loadConfig();
    refreshAll();
    listen<{ id: number; status: string; category: string; error: string }>('ocr-event', (e) => {
      loadStats();
      loadList();
      if (detailOpen && detail && detail.id === e.payload.id) {
        openDetail(e.payload.id);
      }
    }).then((fn) => {
      unlisten = fn;
    });
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<div class="ocr-root">
  <header class="ocr-head">
    <div class="min-w-0">
      <div class="ocr-title">图文识别</div>
      <div class="ocr-sub">接收资源 → 开源OCR预检 → TextIn 证件分类 → 按类归档 → 对应 OCR 识别 → 结果入库</div>
    </div>
    <Badge class="shrink-0" variant={cfg.enabled ? 'default' : 'destructive'}>
      {cfg.enabled ? `接收服务 ${cfg.bindHost}:${cfg.port}` : '接收服务已停用'}
    </Badge>
  </header>

  <Tabs value={view} onValueChange={(v) => switchView(v as View)}>
    <TabsList class="h-9 w-fit">
      <TabsTrigger value="list">
        <ListChecksIcon class="size-3.5" />资源列表
        {#if total > 0}<span class="ocr-tab-count">{total}</span>{/if}
      </TabsTrigger>
      <TabsTrigger value="mapping"><GitBranchIcon class="size-3.5" />分类映射</TabsTrigger>
      <TabsTrigger value="config"><SlidersHorizontalIcon class="size-3.5" />服务配置</TabsTrigger>
      <TabsTrigger value="docs"><BookOpenTextIcon class="size-3.5" />接入文档</TabsTrigger>
    </TabsList>
  </Tabs>

  {#if view === 'list'}
    <div class="ocr-view">
      <!-- 统计条：单行展示全部指标 -->
      <Card>
        <CardContent class="p-0">
          <div class="ocr-statbar">
            <div class="ocr-stat">
              <LiveNumber
                value={stats?.total ?? 0}
                class="text-[22px] font-bold leading-[1.1] tracking-[-0.01em] tabular-nums text-[var(--foreground)]"
              />
              <span class="ocr-stat-lbl">资源总数</span>
            </div>
            {#each Object.entries(STATUS_META) as [key, meta]}
              <div class="ocr-stat">
                <LiveNumber
                  value={stats?.byStatus[key] ?? 0}
                  class="text-[22px] font-bold leading-[1.1] tracking-[-0.01em] tabular-nums text-[var(--foreground)]"
                />
                <span class="ocr-stat-lbl">{meta.label}</span>
              </div>
            {/each}
          </div>
        </CardContent>
      </Card>

      <Card class="ocr-table-card">
        <CardHeader class="pb-0">
          <CardTitle class="text-sm font-semibold">识别结果列表</CardTitle>
        </CardHeader>
        <CardContent class="space-y-3 pt-3">
          <div class="ocr-toolbar">
            <SelectRoot type="single" bind:value={filterStatus} onValueChange={applyFilter}>
              <SelectTrigger size="sm" class="w-32">
                <span>{STATUS_META[filterStatus]?.label ?? '全部状态'}</span>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">全部状态</SelectItem>
                {#each Object.entries(STATUS_META) as [key, meta]}
                  <SelectItem value={key}>{meta.label}</SelectItem>
                {/each}
              </SelectContent>
            </SelectRoot>
            <SelectRoot type="single" bind:value={filterCategory} onValueChange={applyFilter}>
              <SelectTrigger size="sm" class="w-36">
                <span>{filterCategory ? catLabel(filterCategory) : '全部分类'}</span>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">全部分类</SelectItem>
                {#each Object.keys(stats?.byCategory ?? {}) as c}
                  <SelectItem value={c}>{catLabel(c)}</SelectItem>
                {/each}
              </SelectContent>
            </SelectRoot>
            <Input
              class="h-8 w-56"
              placeholder="搜索发送人 / 用户 / 链接"
              bind:value={keyword}
              onkeydown={(e) => e.key === 'Enter' && applyFilter()}
            />
            <Button size="sm" variant="outline" onclick={applyFilter}>筛选</Button>
            <span class="text-xs tabular-nums text-muted-foreground">共 {total} 条</span>
            <div class="ml-auto flex items-center gap-2">
              <Button size="sm" variant="outline" onclick={importLocalImages} disabled={importing} class="gap-1.5" title="选择本地图片批量导入识别">
                <BookOpenTextIcon class="size-3.5" />{importing ? '导入中…' : '导入本地图片'}
              </Button>
              <Button size="sm" variant="outline" onclick={exportCsv} disabled={exportingCsv} class="gap-1.5" title="导出全部 OCR 资源为 CSV">
                <SaveIcon class="size-3.5" />{exportingCsv ? '导出中…' : '导出 CSV'}
              </Button>
              <RippleButton
                onclick={simulate}
                rippleColor="#a5f3fc"
                class="h-8 rounded-md border-0 bg-[var(--primary)] px-3.5 text-xs font-medium text-[var(--primary-foreground)] hover:opacity-90"
              >模拟测试</RippleButton>
              <NativeSelect size="sm" bind:value={testIndex} title="选择内置测试图片">
                {#each TEST_IMAGES as img, i}
                  <NativeSelectOption value={i} title={img.file}>{img.name}</NativeSelectOption>
                {/each}
              </NativeSelect>
              <Button size="sm" variant="outline" onclick={refreshAll} class="gap-1.5">
                <RefreshCwIcon class="size-3.5" />刷新
              </Button>
            </div>
          </div>

          <div class="ocr-table-wrap">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>ID</TableHead>
                  <TableHead>发送人</TableHead>
                  <TableHead>会话类型</TableHead>
                  <TableHead>用户名</TableHead>
                  <TableHead>时间戳</TableHead>
                  <TableHead>分类</TableHead>
                  <TableHead>状态</TableHead>
                  <TableHead>创建时间</TableHead>
                  <TableHead class="text-right">操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {#each items as it}
                  <TableRow>
                    <TableCell class="font-mono text-xs">#{it.id}</TableCell>
                    <TableCell>{it.senderUsername}</TableCell>
                    <TableCell>{it.sessionType || '--'}</TableCell>
                    <TableCell>{it.username}</TableCell>
                    <TableCell class="font-mono text-xs">{it.timestamp || '--'}</TableCell>
                    <TableCell>{it.category ? catLabel(it.category) : '--'}</TableCell>
                    <TableCell>
                      <Badge variant={statusCls(it.status)} title={it.error || undefined}>{statusLabel(it.status)}</Badge>
                    </TableCell>
                    <TableCell class="font-mono text-xs">{it.createdAt}</TableCell>
                    <TableCell class="min-w-[220px]">
                      <div class="flex justify-end gap-1.5 whitespace-nowrap">
                        <Button size="sm" variant="outline" onclick={() => openDetail(it.id)}>查看</Button>
                        <Button size="sm" variant="ghost" onclick={() => retry(it.id)}>重试</Button>
                        <Button size="sm" variant="destructive" onclick={() => (delTarget = it)}>删除</Button>
                      </div>
                    </TableCell>
                  </TableRow>
                {:else}
                  <TableRow>
                    <TableCell colspan={9} class="h-24 text-center text-muted-foreground">
                      {loading ? '加载中…' : '暂无资源，等待 API 推送'}
                    </TableCell>
                  </TableRow>
                {/each}
              </TableBody>
            </Table>
          </div>

          <div class="flex items-center justify-end gap-3 text-xs text-muted-foreground">
            <Button size="sm" variant="outline" disabled={page <= 1} onclick={() => goPage(page - 1)}>上一页</Button>
            <span class="tabular-nums">{page} / {totalPages}</span>
            <Button size="sm" variant="outline" disabled={page >= totalPages} onclick={() => goPage(page + 1)}>下一页</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  {/if}

  {#if view === 'mapping'}
    <div class="ocr-view">
      <Card>
        <CardHeader>
          <div class="flex items-center justify-between gap-3">
            <CardTitle class="text-sm">分类 → OCR 接口映射</CardTitle>
            <Button size="sm" variant="outline" onclick={resetAllEndpoints}>全部恢复默认</Button>
          </div>
          <CardDescription>填 TextIn 接口名走官方地址；填完整 URL 走自定义接口；留空 = 内置默认</CardDescription>
        </CardHeader>
        <CardContent class="space-y-4">
          <div class="max-h-[52vh] overflow-auto rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>分类</TableHead>
                  <TableHead>OCR 接口</TableHead>
                  <TableHead class="w-20 text-center">启用</TableHead>
                  <TableHead class="w-24 text-right">操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {#each endpointOrder as cat}
                  {@const rule = cfg.endpointMap[cat]}
                  <TableRow>
                    <TableCell>
                      <div class="flex items-center gap-2">
                        <span class="font-medium">{catLabel(cat)}</span>
                        <span class="font-mono text-xs text-muted-foreground">{cat}</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Input
                        class="h-8 min-w-64 font-mono text-xs"
                        list="ocr-endpoints"
                        placeholder="留空 = 内置默认"
                        bind:value={rule.endpoint}
                        oninput={() => (cfgDirty = true)}
                      />
                    </TableCell>
                    <TableCell class="text-center">
                      <Switch bind:checked={rule.enabled} onCheckedChange={() => (cfgDirty = true)} />
                    </TableCell>
                    <TableCell class="text-right">
                      <Button size="sm" variant="ghost" onclick={() => resetEndpoint(cat)}>默认</Button>
                    </TableCell>
                  </TableRow>
                {:else}
                  <TableRow>
                    <TableCell colspan={4} class="h-24 text-center text-muted-foreground">正在加载分类映射…</TableCell>
                  </TableRow>
                {/each}
              </TableBody>
            </Table>
          </div>
          <datalist id="ocr-endpoints">
            {#each COMMON_ENDPOINTS as ep}
              <option value={ep}></option>
            {/each}
          </datalist>
<div class="mt-auto flex items-center gap-3">
            <Button onclick={saveConfig} disabled={!cfgDirty || saving}>{saving ? '保存中…' : '保存映射'}</Button>
            <span class="text-xs text-muted-foreground">未启用或留空接口的分类，将只完成分类归档、跳过 OCR</span>
          </div>
        </CardContent>
      </Card>
    </div>
  {/if}

  {#if view === 'config'}
    <div class="ocr-view">
      <Card>
        <CardHeader>
          <CardTitle class="text-sm">TextIn 凭证与接收服务</CardTitle>
          <CardDescription>凭证在 TextIn 工作台 → 账号设置 → 开发者信息 获取</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="ocr-form-row">
            <div class="ocr-field">
              <Label for="ocr-app-id">x-ti-app-id</Label>
              <Input id="ocr-app-id" bind:value={cfg.appId} placeholder="请输入 x-ti-app-id" oninput={() => (cfgDirty = true)} />
            </div>
            <div class="ocr-field">
              <Label for="ocr-secret">x-ti-secret-code</Label>
              <Input id="ocr-secret" type="password" bind:value={cfg.secretCode} placeholder="请输入 x-ti-secret-code" oninput={() => (cfgDirty = true)} />
            </div>
            <div class="ocr-field w-28">
              <Label for="ocr-bind">监听地址</Label>
              <Input id="ocr-bind" bind:value={cfg.bindHost} placeholder="0.0.0.0" oninput={() => (cfgDirty = true)} />
            </div>
            <div class="ocr-field w-24">
              <Label for="ocr-port">端口</Label>
              <Input id="ocr-port" type="number" min="1" max="65535" bind:value={cfg.port} oninput={() => (cfgDirty = true)} />
            </div>
            <div class="ocr-field">
              <Label for="ocr-token">访问令牌</Label>
              <Input id="ocr-token" type="password" bind:value={cfg.token} placeholder="留空 = 免鉴权" oninput={() => (cfgDirty = true)} />
            </div>
            <div class="ocr-field w-36">
              <Label for="ocr-enabled">接收服务</Label>
              <div class="flex h-9 items-center gap-2">
                <Switch id="ocr-enabled" bind:checked={cfg.enabled} onCheckedChange={() => (cfgDirty = true)} />
                <span class="text-xs text-muted-foreground">启用</span>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle class="text-sm">开源 OCR 预检（RapidOCR）</CardTitle>
          <CardDescription>
            先本地识别图片文字，识别出有效文本才调用 TextIn 证件分类，过滤无文字图片，节省分类接口调用
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div class="ocr-form-row">
            <div class="ocr-field w-44">
              <Label for="ocr-precheck-enabled">启用预检过滤</Label>
              <div class="flex h-9 items-center gap-2">
                <Switch id="ocr-precheck-enabled" bind:checked={cfg.precheckEnabled} onCheckedChange={() => (cfgDirty = true)} />
                <span class="text-xs text-muted-foreground">先识别文字再分类</span>
              </div>
            </div>
            <div class="ocr-field w-32">
              <Label for="ocr-precheck-min">最小文本字符数</Label>
              <Input id="ocr-precheck-min" type="number" min="1" max="100" bind:value={cfg.precheckMinChars} oninput={() => (cfgDirty = true)} />
            </div>
            <div class="ocr-field">
              <Label for="ocr-precheck-dir">模型缓存目录</Label>
              <Input id="ocr-precheck-dir" bind:value={cfg.precheckModelDir} placeholder="留空 = %APPDATA%/st-control/rapidocr-models" oninput={() => (cfgDirty = true)} />
            </div>
          </div>
          <p class="text-xs text-muted-foreground">
            模型为开源 PP-OCRv6（Apache-2.0），首次使用自动下载到本地，之后离线识别；
            PDF 等非图片格式不预检，沿用原流程。
          </p>
        </CardContent>
      </Card>

      <div class="mt-auto flex items-center gap-3">
        <Button onclick={saveConfig} disabled={!cfgDirty || saving} class="gap-1.5">
          <SaveIcon class="size-4" />{saving ? '保存中…' : '保存配置'}
        </Button>
        {#if cfgDirty}
          <span class="text-xs text-muted-foreground">有未保存的修改</span>
        {/if}
      </div>
    </div>
  {/if}

  {#if view === 'docs'}
    <div class="ocr-view">
      <Card class="max-w-4xl">
        <CardHeader>
          <CardTitle class="text-sm">资源接入 API</CardTitle>
          <CardDescription>POST http://&lt;主机&gt;:{cfg.port}/api/ocr/ingest</CardDescription>
        </CardHeader>
        <CardContent class="space-y-5">
          <div class="space-y-2">
            <div class="text-xs font-semibold text-muted-foreground">必填参数</div>
            <div class="rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>参数</TableHead>
                    <TableHead>类型</TableHead>
                    <TableHead>说明</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  <TableRow><TableCell class="font-mono text-xs">sender_username</TableCell><TableCell>string</TableCell><TableCell>发送方用户名（必填）</TableCell></TableRow>
                  <TableRow><TableCell class="font-mono text-xs">session_type</TableCell><TableCell>string</TableCell><TableCell>会话类型，如 group / single（必填）</TableCell></TableRow>
                  <TableRow><TableCell class="font-mono text-xs">timestamp</TableCell><TableCell>string</TableCell><TableCell>资源产生时间（必填）</TableCell></TableRow>
                  <TableRow><TableCell class="font-mono text-xs">username</TableCell><TableCell>string</TableCell><TableCell>资源归属用户名（必填）</TableCell></TableRow>
                  <TableRow><TableCell class="font-mono text-xs">mediaUrl</TableCell><TableCell>string</TableCell><TableCell>http(s) 链接 / file:// / 本地路径（必填）</TableCell></TableRow>
                </TableBody>
              </Table>
            </div>
          </div>
          <div class="space-y-2">
            <div class="text-xs font-semibold text-muted-foreground">示例</div>
            <pre class="ocr-code">{curlExample}</pre>
          </div>
          <div class="space-y-2">
            <div class="text-xs font-semibold text-muted-foreground">处理流程</div>
            <ol class="ml-5 list-decimal space-y-1 text-xs text-muted-foreground">
              <li>接收资源并校验必填参数，返回 202 + id</li>
              <li>下载/读取图片，暂存 incoming 目录</li>
              <li>开源 OCR 预检：无有效文本的图片归档到 ocr/filtered 并停止（不调用证件分类）</li>
              <li>调用 TextIn 证件分类，确定证照类型</li>
              <li>按分类归档到 ocr/&lt;分类&gt;/yyyy/MM/dd/</li>
              <li>按「分类映射」配置的接口执行 OCR，结果写入数据库</li>
            </ol>
          </div>
          {#if cfg.token}
            <p class="text-xs text-muted-foreground">鉴权：请求头 <code class="font-mono text-primary">Authorization: Bearer {cfg.token}</code>，或在 body 传 access_token</p>
          {/if}
        </CardContent>
      </Card>
    </div>
  {/if}

  <Dialog bind:open={detailOpen}>
    <DialogContent class="max-w-3xl">
      {#if detail}
        <DialogHeader>
          <DialogTitle>资源 #{detail.id} 详情</DialogTitle>
          <DialogDescription>
            {detail.senderUsername} · {detail.username} · {detail.timestamp || '--'}
            {#if detail.error}
              <span class="mt-1 block text-destructive">{detail.error}</span>
            {/if}
          </DialogDescription>
        </DialogHeader>
        <div class="grid grid-cols-2 gap-x-6 gap-y-2 rounded-md border p-4 text-xs">
          <div class="flex gap-2"><span class="w-16 text-muted-foreground">分类</span><span>{detail.category ? `${catLabel(detail.category)} (${detail.category})` : '--'}</span></div>
          <div class="flex gap-2"><span class="w-16 text-muted-foreground">状态</span><Badge variant={statusCls(detail.status)}>{statusLabel(detail.status)}</Badge></div>
          <div class="col-span-2 flex gap-2"><span class="w-16 shrink-0 text-muted-foreground">链接</span><span class="break-all font-mono">{detail.mediaUrl}</span></div>
          <div class="col-span-2 flex gap-2"><span class="w-16 shrink-0 text-muted-foreground">归档</span><span class="break-all font-mono">{detail.mediaPath || '--'}</span></div>
        </div>
        <Tabs bind:value={detailTab}>
          <TabsList class="grid w-full grid-cols-4">
            <TabsTrigger value="fields">识别字段</TabsTrigger>
            <TabsTrigger value="classify">分类结果</TabsTrigger>
            <TabsTrigger value="ocr">OCR 原始返回</TabsTrigger>
            <TabsTrigger value="precheck">预检文本</TabsTrigger>
          </TabsList>
          <TabsContent value="fields">
            <textarea class="ocr-edit" bind:value={editFields} rows={10} placeholder="识别字段 JSON（可人工校对后保存）" spellcheck="false"></textarea>
            <div class="mt-2 flex justify-end gap-2">
              <Button size="sm" variant="outline" onclick={() => { editFields = detail?.ocrFields || ''; }}>还原</Button>
              <Button size="sm" onclick={saveFields} disabled={savingFields}>{savingFields ? '保存中…' : '保存校对'}</Button>
            </div>
          </TabsContent>
          <TabsContent value="classify"><pre class="ocr-pre">{prettyJson(detail.classifyRaw)}</pre></TabsContent>
          <TabsContent value="ocr"><pre class="ocr-pre">{prettyJson(detail.ocrRaw)}</pre></TabsContent>
          <TabsContent value="precheck"><pre class="ocr-pre">{detail.precheckText || '（未识别到文本）'}</pre></TabsContent>
        </Tabs>
      {/if}
    </DialogContent>
  </Dialog>

  <AlertDialogRoot open={delTarget !== null} onOpenChange={(o) => !o && (delTarget = null)}>
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>删除资源 #{delTarget?.id}</AlertDialogTitle>
        <AlertDialogDescription>
          确定删除该资源及其归档文件吗？此操作不可恢复。
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel onclick={() => (delTarget = null)}>取消</AlertDialogCancel>
        <AlertDialogAction onclick={confirmDelete}>删除</AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialogRoot>
</div>

<style>
  .ocr-root {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100%;
    overflow-y: auto;
    padding: 4px;
  }
  .ocr-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .ocr-title { font-size: 16px; font-weight: 700; }
  .ocr-sub { margin-top: 2px; font-size: 12px; color: var(--muted-foreground); }
  .ocr-tab-count {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 11.5px;
    line-height: 16px;
    font-weight: 700;
  }
  .ocr-view { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 12px; }
  /* 统计条：4 列两行，避免 8 项挤一行 */
  .ocr-statbar {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
  .ocr-stat {
    min-width: 0;
    display: flex;
    align-items: baseline;
    justify-content: center;
    gap: 8px;
    padding: 12px 10px;
    white-space: nowrap;
  }
  .ocr-stat + .ocr-stat { border-left: 1px solid var(--border); }
  .ocr-stat:nth-child(4n + 1) { border-left: none; }
  .ocr-stat:nth-child(n + 5) { border-top: 1px solid var(--border); }
  .ocr-stat-lbl {
    font-size: 12px;
    color: var(--muted-foreground);
  }
  .ocr-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .ocr-form-row {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    flex-wrap: wrap;
  }
  .ocr-field {
    flex: 1 1 0;
    min-width: 120px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ocr-code {
    overflow-x: auto;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: color-mix(in oklab, black 45%, var(--card));
    color: color-mix(in oklab, var(--foreground) 82%, var(--primary));
    padding: 12px 14px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.65;
    white-space: pre;
  }
  .ocr-pre {
    max-height: 44vh;
    overflow: auto;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: color-mix(in oklab, black 45%, var(--card));
    color: color-mix(in oklab, var(--foreground) 82%, var(--primary));
    padding: 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.65;
    white-space: pre;
  }
  .ocr-edit {
    width: 100%;
    max-height: 44vh;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: color-mix(in oklab, black 45%, var(--card));
    color: color-mix(in oklab, var(--foreground) 86%, var(--primary));
    padding: 10px 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.6;
    resize: vertical;
    outline: none;
  }
  .ocr-edit:focus { border-color: var(--primary); }
  /* 表格卡片占满剩余高度：空状态不再下方留白 */
  :global(.ocr-table-card) { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 0; }
  :global(.ocr-table-card [data-slot="card-content"]) { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 12px; }
  .ocr-table-wrap { flex: 1; min-height: 0; overflow-y: auto; }

</style>
