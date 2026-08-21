<script lang="ts">
  // ============================================================
  // ToolCard — 工具调用卡片（DSH ui-tool 迁移：按工具类型渲染专用卡）
  // bash/终端卡（命令 + 输出）、read 读文件卡（行号）、diff 卡（编辑对比）、
  // web 检索卡（引用列表）；其余工具回退通用参数/结果卡。
  // ============================================================
  import CheckIcon from "@lucide/svelte/icons/check";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import TerminalIcon from "@lucide/svelte/icons/terminal";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import DiffIcon from "@lucide/svelte/icons/git-compare";
  import GlobeIcon from "@lucide/svelte/icons/globe";
  import SearchIcon from "@lucide/svelte/icons/search";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import WorkflowIcon from "@lucide/svelte/icons/workflow";

  let { name = "", args = "", result = "", ok = true }: {
    name?: string;
    args?: string;
    result?: string;
    ok?: boolean;
  } = $props();

  let copiedText = $state("");

  /** 解析参数 JSON（失败返回 null） */
  function parsedArgs(): Record<string, unknown> | null {
    try {
      const v = JSON.parse(args);
      return typeof v === "object" && v !== null ? (v as Record<string, unknown>) : null;
    } catch {
      return null;
    }
  }

  function strArg(key: string): string {
    const v = parsedArgs()?.[key];
    return typeof v === "string" ? v : "";
  }

  function prettyText(s?: string): string {
    if (!s) return "";
    try {
      return JSON.stringify(JSON.parse(s), null, 2);
    } catch {
      return s;
    }
  }

  function truncate(s: string, n: number): string {
    if (s.length <= n) return s;
    return s.slice(0, n) + "…";
  }

  async function copyText(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* 剪贴板不可用时静默忽略 */
    }
    copiedText = text.slice(0, 20);
    window.setTimeout(() => {
      if (copiedText === text.slice(0, 20)) copiedText = "";
    }, 1500);
  }

  /** 读文件卡：内容按行加行号 */
  const readLines = $derived.by(() => {
    const lines = result.split("\n");
    const width = String(lines.length).length;
    return lines.map((l, i) => `${String(i + 1).padStart(width, " ")} │ ${l}`);
  });

  const isBash = $derived(name === "exec_command" || name === "shell_run" || name === "bash" || name === "pwsh");
  const isRead = $derived(name === "read_file");
  const isEdit = $derived(name === "edit_file");
  const isWrite = $derived(name === "write_file");
  const isSre = $derived(name === "str_replace_editor");
  const isWeb = $derived(name === "web_search" || name === "fetch_web_page");
  const isSearch = $derived(name === "grep" || name === "glob" || name === "search_knowledge_base" || name === "session_search" || name === "session_event_search");
  const isTodo = $derived(name === "todo_write");
  const isSkill = $derived(name === "skill_load" || name === "skill_list");
  const isWorkflow = $derived(name === "workflow_run_js");

  /** 编排卡：结果 JSON（可能带 [日志] 前缀——取最后一个 \n\n 之后的部分） */
  const workflowValue = $derived.by(() => {
    const idx = result.lastIndexOf("\n\n");
    const jsonPart = idx >= 0 ? result.slice(idx + 2) : result;
    try {
      return JSON.parse(jsonPart) as unknown;
    } catch {
      return null;
    }
  });

  /** 搜索卡：结果按行展示（grep 的 path:line 匹配行 / glob 的路径列表） */
  const searchLines = $derived.by(() => {
    if (!result) return [];
    return result.split("\n").filter((l) => l.trim().length > 0).slice(0, 60);
  });

  /** 任务卡：todo_write 的 items 数组（含 status） */
  const todoItems = $derived.by(() => {
    try {
      const v = JSON.parse(args);
      const items = Array.isArray(v) ? v : v.items;
      if (!Array.isArray(items)) return null;
      return items.map((it: unknown) => {
        const o = (it ?? {}) as Record<string, unknown>;
        return {
          content: typeof o.content === "string" ? o.content : String(o.content ?? ""),
          status: typeof o.status === "string" ? o.status : "",
        };
      });
    } catch {
      return null;
    }
  });
</script>

{#if isBash}
  <div class="tc tc-terminal">
    <div class="tc-head">
      <TerminalIcon class="size-3.5" />
      <span class="tc-head-title">终端</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
      <button class="tc-copy" onclick={() => copyText(result)} title="复制输出">
        {#if copiedText === result.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
      </button>
    </div>
    <div class="tc-cmd">$ {strArg("command") || "（无命令）"}</div>
    {#if result}
      <pre class="tc-out">{result}</pre>
    {/if}
  </div>
{:else if isRead}
  <div class="tc tc-read">
    <div class="tc-head">
      <FileTextIcon class="size-3.5" />
      <span class="tc-head-title">{strArg("path") || "读文件"}</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
      <button class="tc-copy" onclick={() => copyText(result)} title="复制内容">
        {#if copiedText === result.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
      </button>
    </div>
    <pre class="tc-read-lines">{readLines.join("\n")}</pre>
  </div>
{:else if isEdit || isWrite}
  <div class="tc tc-diff">
    <div class="tc-head">
      <DiffIcon class="size-3.5" />
      <span class="tc-head-title">{strArg("path") || (isEdit ? "编辑文件" : "写文件")}</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
      <button class="tc-copy" onclick={() => copyText(prettyText(args))} title="复制参数">
        {#if copiedText === args.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
      </button>
    </div>
    {#if isEdit}
      {#if strArg("old_string")}
        <div class="tc-diff-row del"><span class="tc-diff-mark">−</span><pre>{strArg("old_string")}</pre></div>
      {/if}
      {#if strArg("new_string")}
        <div class="tc-diff-row add"><span class="tc-diff-mark">+</span><pre>{strArg("new_string")}</pre></div>
      {/if}
    {:else}
      <div class="tc-diff-row add"><span class="tc-diff-mark">+</span><pre>{strArg("content")}</pre></div>
    {/if}
    {#if result && result !== strArg("content")}
      <div class="tc-result-note">{result}</div>
    {/if}
  </div>
{:else if isWeb}
  <div class="tc tc-web">
    <div class="tc-head">
      <GlobeIcon class="size-3.5" />
      <span class="tc-head-title">网页检索{strArg("query") ? `：${strArg("query")}` : ""}</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
      <button class="tc-copy" onclick={() => copyText(result)} title="复制结果">
        {#if copiedText === result.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
      </button>
    </div>
    {#if result}
      <pre class="tc-out tc-web-out">{result}</pre>
    {/if}
  </div>
{:else if isSearch}
  <div class="tc tc-search">
    <div class="tc-head">
      <SearchIcon class="size-3.5" />
      <span class="tc-head-title">搜索{strArg("query") || strArg("pattern") ? `：${strArg("query") || strArg("pattern")}` : ""}</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
      <button class="tc-copy" onclick={() => copyText(result)} title="复制结果">
        {#if copiedText === result.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
      </button>
    </div>
    {#if searchLines.length > 0}
      <div class="tc-search-lines">
        {#each searchLines as line (line)}
          <div class="tc-search-line">{line}</div>
        {/each}
      </div>
    {:else}
      <div class="tc-result-note">{result || "（无匹配）"}</div>
    {/if}
  </div>
{:else if isTodo}
  <div class="tc tc-todo">
    <div class="tc-head">
      <CheckIcon class="size-3.5" />
      <span class="tc-head-title">任务清单</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
    </div>
    {#if todoItems}
      <div class="tc-todo-list">
        {#each todoItems as item, i (i)}
          <div class="tc-todo-item" class:done={item.status === "completed"} class:doing={item.status === "in_progress"}>
            <span class="tc-todo-status">
              {#if item.status === "completed"}✓{:else if item.status === "in_progress"}▶{:else}○{/if}
            </span>
            <span class="tc-todo-text">{item.content}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="tc-result-note">{result || "（无任务）"}</div>
    {/if}
  </div>
{:else if isSkill}
  <div class="tc tc-skill">
    <div class="tc-head">
      <SparklesIcon class="size-3.5" />
      <span class="tc-head-title">技能{strArg("id") || strArg("name") ? `：${strArg("id") || strArg("name")}` : ""}</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
    </div>
    <div class="tc-result-note">{truncate(result, 500)}</div>
  </div>
{:else if isWorkflow}
  <div class="tc tc-workflow">
    <div class="tc-head">
      <WorkflowIcon class="size-3.5" />
      <span class="tc-head-title">编排脚本（workflow_run_js）</span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
      <button class="tc-copy" onclick={() => copyText(result)} title="复制结果">
        {#if copiedText === result.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
      </button>
    </div>
    {#if workflowValue !== null}
      {#if Array.isArray(workflowValue)}
        <div class="tc-workflow-rows">
          {#each workflowValue as v, i (i)}
            <div class="tc-workflow-row"><span class="tc-workflow-key">[{i}]</span><pre>{typeof v === "string" ? v : JSON.stringify(v)}</pre></div>
          {/each}
        </div>
      {:else if typeof workflowValue === "object" && workflowValue !== null}
        <div class="tc-workflow-rows">
          {#each Object.entries(workflowValue as Record<string, unknown>) as [k, v] (k)}
            <div class="tc-workflow-row"><span class="tc-workflow-key">{k}</span><pre>{typeof v === "string" ? v : JSON.stringify(v)}</pre></div>
          {/each}
        </div>
      {:else}
        <pre class="tc-out">{String(workflowValue)}</pre>
      {/if}
    {:else}
      <pre class="tc-out">{result || "（无输出）"}</pre>
    {/if}
  </div>
{:else if isSre}
  <div class="tc tc-sre">
    <div class="tc-head">
      <DiffIcon class="size-3.5" />
      <span class="tc-head-title">
        编辑器 · {strArg("command") || "?"}{strArg("path") ? `：${strArg("path")}` : ""}
      </span>
      {#if ok}<span class="tc-badge ok">完成</span>{:else}<span class="tc-badge err">失败</span>{/if}
      <button class="tc-copy" onclick={() => copyText(prettyText(args))} title="复制参数">
        {#if copiedText === args.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
      </button>
    </div>
    {#if strArg("command") === "view"}
      {#if result}
        <pre class="tc-out tc-sre-view">{result}</pre>
      {:else}
        <div class="tc-result-note">（空文件或无输出）</div>
      {/if}
    {:else if strArg("command") === "create"}
      {#if strArg("file_text")}
        <div class="tc-diff-row add"><span class="tc-diff-mark">+</span><pre>{strArg("file_text")}</pre></div>
      {/if}
      {#if result}<div class="tc-result-note">{result}</div>{/if}
    {:else if strArg("command") === "str_replace"}
      {#if strArg("old_str")}
        <div class="tc-diff-row del"><span class="tc-diff-mark">−</span><pre>{strArg("old_str")}</pre></div>
      {/if}
      {#if strArg("new_str")}
        <div class="tc-diff-row add"><span class="tc-diff-mark">+</span><pre>{strArg("new_str")}</pre></div>
      {/if}
      {#if result}<div class="tc-result-note">{result}</div>{/if}
    {:else if strArg("command") === "insert"}
      <div class="tc-result-note">
        {#if strArg("insert_line") !== ""}第 {strArg("insert_line")} 行后插入{/if}
        {#if strArg("new_str")}<pre class="tc-sre-insert">{strArg("new_str")}</pre>{/if}
      </div>
      {#if result}<div class="tc-result-note">{result}</div>{/if}
    {:else}
      {#if result}
        <pre class="tc-out">{result}</pre>
      {:else}
        <div class="tc-result-note">{ok ? "（无输出）" : "执行失败"}</div>
      {/if}
    {/if}
  </div>
{:else}
  <div class="tc tc-generic">
    <div class="tc-field">
      <div class="tc-field-head"><span>参数</span></div>
      <pre class="tc-pre">{prettyText(args) || "（无参数）"}</pre>
    </div>
    <div class="tc-field">
      <div class="tc-field-head"><span>{ok ? "结果" : "错误"}</span></div>
      <pre class="tc-pre">{result || "（无输出）"}</pre>
    </div>
  </div>
{/if}

<style>
  .tc { border: 1px solid rgba(128, 128, 128, .22); border-radius: 8px; overflow: hidden; }
  .tc-head {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 10px;
    background: rgba(128, 128, 128, .07);
    font-size: 11px; font-weight: 600;
    color: var(--hns-muted, #888);
  }
  .tc-head > :global(svg) { color: var(--hns-accent, #4176e6); flex: none; }
  .tc-head-title {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--hns-text, inherit);
    font-family: ui-monospace, Consolas, monospace;
    font-size: 10.5px;
  }
  .tc-badge { flex: none; font-size: 9.5px; font-weight: 700; border-radius: 4px; padding: 0 5px; }
  .tc-badge.ok { color: #2ea043; background: rgba(46, 160, 67, .12); }
  .tc-badge.err { color: #d1242f; background: rgba(209, 36, 47, .12); }
  .tc-workflow-rows { display: flex; flex-direction: column; }
  .tc-workflow-row {
    display: flex; gap: 8px; padding: 4px 10px;
    border-top: 1px solid rgba(128, 128, 128, .1);
    font-size: 11.5px;
  }
  .tc-workflow-row:first-child { border-top: 0; }
  .tc-workflow-key {
    flex: none; font-family: ui-monospace, Consolas, monospace;
    font-weight: 700; color: var(--hns-accent, #4176e6);
    min-width: 40px;
  }
  .tc-workflow-row pre { margin: 0; white-space: pre-wrap; word-break: break-word; }
  .tc-badge.ok { color: #2ea043; background: rgba(46, 160, 67, .14); }
  .tc-badge.err { color: #d73a49; background: rgba(215, 58, 73, .14); }
  .tc-copy {
    flex: none; display: inline-flex; align-items: center; gap: 3px;
    font-size: 10px; color: var(--hns-muted, #888);
    background: transparent; border: 0; cursor: pointer;
  }
  .tc-copy:hover { color: var(--hns-text, inherit); }
  .tc-cmd {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 11px; color: var(--hns-text, inherit);
    padding: 6px 10px;
    background: rgba(128, 128, 128, .06);
    white-space: pre-wrap; word-break: break-all;
  }
  .tc-out {
    margin: 0;
    padding: 8px 10px;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 10.5px; line-height: 1.6;
    color: var(--hns-text, inherit);
    white-space: pre-wrap; word-break: break-all;
    max-height: 300px; overflow: auto;
  }
  .tc-read-lines {
    margin: 0;
    padding: 8px 10px;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 10.5px; line-height: 1.6;
    color: var(--hns-muted, #888);
    white-space: pre; overflow: auto;
    max-height: 300px;
  }
  .tc-diff-row {
    display: flex; gap: 6px;
    padding: 3px 10px;
    font-size: 10.5px;
  }
  .tc-diff-row pre {
    margin: 0;
    flex: 1; min-width: 0;
    font-family: ui-monospace, Consolas, monospace;
    white-space: pre-wrap; word-break: break-all;
  }
  .tc-diff-row.del { background: rgba(215, 58, 73, .08); color: #d73a49; }
  .tc-diff-row.add { background: rgba(46, 160, 67, .08); color: #2ea043; }
  .tc-diff-mark { flex: none; font-family: ui-monospace, monospace; font-weight: 700; }
  .tc-result-note { padding: 5px 10px; font-size: 10.5px; color: var(--hns-muted, #888); border-top: 1px solid rgba(128, 128, 128, .12); }
  .tc-field { padding: 8px 10px; }
  .tc-field + .tc-field { border-top: 1px solid rgba(128, 128, 128, .12); }
  .tc-field-head { font-size: 10px; color: var(--hns-muted, #888); margin-bottom: 4px; }
  /* ─── 搜索卡（grep/glob/知识库检索：路径/匹配行列表） ─── */
  .tc-search-lines { max-height: 300px; overflow: auto; }
  .tc-search-line {
    padding: 3px 10px;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 10.5px;
    color: var(--hns-text, inherit);
    white-space: pre-wrap;
    word-break: break-all;
    border-bottom: 1px solid rgba(128, 128, 128, .07);
  }
  .tc-search-line:last-child { border-bottom: 0; }
  /* ─── 任务卡（todo_write：状态圆点列表） ─── */
  .tc-todo-list { padding: 4px 10px 8px; max-height: 260px; overflow: auto; }
  .tc-todo-item {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 3px 0;
    font-size: 11.5px;
    color: var(--hns-text, inherit);
  }
  .tc-todo-item.done .tc-todo-text { color: var(--hns-muted, #888); text-decoration: line-through; }
  .tc-todo-status { flex: none; font-size: 11px; line-height: 1.5; }
  .tc-todo-item.done .tc-todo-status { color: #2ea043; }
  .tc-todo-item.doing .tc-todo-status { color: var(--hns-accent, #4176e6); }
  .tc-todo-text { flex: 1; min-width: 0; word-break: break-word; }
  /* ─── 技能卡（skill_load：指令说明） ─── */
  .tc-skill .tc-result-note { white-space: pre-wrap; }
  /* ─── str_replace_editor 卡（四命令编辑器） ─── */
  .tc-sre .tc-sre-view { font-family: ui-monospace, Consolas, monospace; font-size: 10px; }
  .tc-sre-insert { margin: 4px 0 0; font-family: ui-monospace, Consolas, monospace; font-size: 10.5px; white-space: pre-wrap; word-break: break-all; }
  .tc-pre {
    margin: 0;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 10.5px; line-height: 1.55;
    white-space: pre-wrap; word-break: break-all;
    max-height: 200px; overflow: auto;
    color: var(--hns-text, inherit);
  }
</style>
