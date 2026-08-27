<script lang="ts">
  /**
   * 帮助文档组件
   * 功能：显示知识库使用指南、快捷键、常见问题
   */
  import KbIcon from './KbIcon.svelte';
  import { Button } from '../components/ui/button';

  interface Props {
    open: boolean;
    onClose: () => void;
  }
  let { open, onClose }: Props = $props();

  let activeSection = $state<string>('quickstart');

  const sections = [
    { id: 'quickstart', label: '快速开始', icon: 'sparkle' },
    { id: 'upload', label: '文档上传', icon: 'upload' },
    { id: 'search', label: '检索问答', icon: 'search' },
    { id: 'wiki', label: 'Wiki 知识库', icon: 'wiki' },
    { id: 'faq', label: 'FAQ 问答', icon: 'list' },
    { id: 'permissions', label: '权限管理', icon: 'shield' },
    { id: 'shortcuts', label: '快捷键', icon: 'keyboard' },
  ];
</script>

{#if open}
  <div class="help-overlay" onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} role="dialog" aria-modal="true" aria-label="帮助文档" tabindex="-1">
    <div class="help-modal" role="presentation" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="help-header">
        <span class="help-title"><KbIcon name="info" size={18} />知识库使用指南</span>
        <Button variant="ghost" size="icon-sm" onclick={onClose} aria-label="关闭"><KbIcon name="close" size={16} /></Button>
      </div>
      <div class="help-body">
        <nav class="help-nav">
          {#each sections as s}
            <button class="help-nav-item" class:active={activeSection === s.id} onclick={() => activeSection = s.id}>
              <KbIcon name={s.icon} size={14} />{s.label}
            </button>
          {/each}
        </nav>
        <div class="help-content">
          {#if activeSection === 'quickstart'}
            <h3>快速开始</h3>
            <ol>
              <li><strong>创建知识库</strong>：点击首页「新建知识库」，输入名称和描述。</li>
              <li><strong>上传文档</strong>：进入知识库后，拖拽文件或点击「上传文件」，支持 PDF/Word/TXT/Markdown 等格式。</li>
              <li><strong>等待处理</strong>：文档会自动解析 → 分片 → 向量化，状态变为「就绪」后即可检索。</li>
              <li><strong>开始问答</strong>：进入「AI 问答」，选择知识库，输入问题即可获得带引用的回答。</li>
            </ol>
          {:else if activeSection === 'upload'}
            <h3>文档上传</h3>
            <ul>
              <li><strong>支持格式</strong>：PDF、Word(doc/docx)、TXT、Markdown、Excel、PPT、图片(OCR)等。</li>
              <li><strong>文件大小</strong>：单文件最大 200MB。</li>
              <li><strong>文件夹上传</strong>：支持拖拽文件夹，自动按目录结构创建分类。</li>
              <li><strong>网页抓取</strong>：支持输入 URL 自动抓取网页内容。</li>
              <li><strong>去重机制</strong>：相同内容的文件会自动跳过，避免重复。</li>
              <li><strong>嵌入模型</strong>：如需语义检索，请在「设置 → 模型设置」中配置 Embeddings 模型。</li>
            </ul>
          {:else if activeSection === 'search'}
            <h3>检索与问答</h3>
            <ul>
              <li><strong>检索模式</strong>：混合（默认，BM25 + 向量）、仅向量、仅全文。</li>
              <li><strong>混合检索</strong>：自动融合 BM25 关键词匹配与向量语义相似度，中文整句会自动提取关键词回退。</li>
              <li><strong>RAG 问答</strong>：基于检索结果调用 LLM 生成回答，支持流式输出和引用标注。</li>
              <li><strong>停止生成</strong>：生成过程中可点击「停止」按钮中断，或等待 120 秒超时自动停止。</li>
              <li><strong>引用查看</strong>：点击回答中的引用编号可跳转查看原文。</li>
            </ul>
          {:else if activeSection === 'wiki'}
            <h3>Wiki 知识库</h3>
            <ul>
              <li><strong>自动提炼</strong>：点击「提炼」按钮，LLM 会从文档中自动生成 Wiki 页面（多页 + 双链）。</li>
              <li><strong>预览模式</strong>：勾选「预览」后点击按钮，仅显示将要处理的文档数量，不执行。</li>
              <li><strong>实体开关</strong>：勾选「实体」控制是否自动创建实体页面与目录。</li>
              <li><strong>手动创建</strong>：点击「新建」手动创建 Wiki 页面，支持 Markdown 编辑。</li>
              <li><strong>知识图谱</strong>：点击「图谱」查看页面间的链接关系网络。</li>
              <li><strong>版本控制</strong>：每次编辑自动保存版本，支持回滚到历史版本。</li>
            </ul>
          {:else if activeSection === 'faq'}
            <h3>FAQ 问答</h3>
            <ul>
              <li><strong>作用</strong>：FAQ 问答对在检索时优先命中，直接给出标准答案，无需 LLM 生成。</li>
              <li><strong>添加</strong>：在知识库工作区点击「FAQ」按钮，输入问题和答案。</li>
              <li><strong>批量导入</strong>：支持 JSON 格式批量导入，每项包含 question、answer、category 字段。</li>
              <li><strong>使用场景</strong>：适合常见问题的标准答案，如「如何重置密码」「支持哪些格式」等。</li>
            </ul>
          {:else if activeSection === 'permissions'}
            <h3>权限管理</h3>
            <ul>
              <li><strong>用户管理</strong>：管理员可在「设置」页面创建用户、重置密码、设置管理员。</li>
              <li><strong>成员管理</strong>：在知识库工作区点击「成员」按钮，添加用户并分配角色（管理员/编辑者/查看者）。</li>
              <li><strong>ACL 规则</strong>：在知识库工作区点击「权限」按钮，设置对象级权限（文档/文件夹/知识库）。</li>
              <li><strong>角色说明</strong>：owner（所有者）> admin（管理员）> editor（编辑者）> viewer（查看者）。</li>
              <li><strong>deny 优先</strong>：ACL 规则中 deny 优先于 allow。</li>
            </ul>
          {:else if activeSection === 'shortcuts'}
            <h3>快捷键</h3>
            <table class="help-shortcuts">
              <tbody>
                <tr><td><kbd>Ctrl</kbd> + <kbd>K</kbd></td><td>全局搜索</td></tr>
                <tr><td><kbd>Ctrl</kbd> + <kbd>B</kbd></td><td>折叠/展开侧边栏</td></tr>
                <tr><td><kbd>Enter</kbd></td><td>发送消息（AI 问答）</td></tr>
                <tr><td><kbd>Escape</kbd></td><td>关闭弹窗</td></tr>
              </tbody>
            </table>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .help-overlay {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0,0,0,0.4);
    display: grid; place-items: center;
  }
  .help-modal {
    width: min(960px, 95vw);
    max-height: 85vh;
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border);
    border-radius: 12px;
    display: flex; flex-direction: column;
    overflow: hidden;
  }
  .help-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--kb-border-subtle);
  }
  .help-title { font-size: 15px; font-weight: 700; display: flex; align-items: center; gap: 8px; }
  .help-body { flex: 1; display: flex; min-height: 0; }
  .help-nav {
    width: 150px; flex-shrink: 0;
    border-right: 1px solid var(--kb-border-subtle);
    padding: 8px 6px;
    display: flex; flex-direction: column; gap: 2px;
    overflow-y: auto;
  }
  .help-nav-item {
    display: flex; align-items: center; gap: 6px;
    padding: 7px 10px; border: none; border-radius: 6px;
    background: transparent; font-size: 12.5px; color: var(--kb-text-2);
    cursor: pointer; text-align: left;
    transition: background 0.12s;
  }
  .help-nav-item:hover { background: var(--kb-hover); }
  .help-nav-item.active { background: var(--kb-hover-strong); color: var(--kb-accent-bright); font-weight: 600; }
  .help-content {
    flex: 1; padding: 18px 22px; overflow-y: auto;
    font-size: 13px; line-height: 1.7; color: var(--kb-text-2);
  }
  .help-content h3 { font-size: 16px; font-weight: 700; color: var(--kb-text); margin: 0 0 14px; }
  .help-content ol, .help-content ul { padding-left: 20px; margin: 0; }
  .help-content li { margin-bottom: 6px; }
  .help-content strong { color: var(--kb-text); }
  .help-shortcuts { width: 100%; border-collapse: collapse; }
  .help-shortcuts td { padding: 6px 10px; border-bottom: 1px solid var(--kb-border-subtle); }
  .help-shortcuts kbd {
    font-family: var(--font-mono); font-size: 11.5px;
    padding: 2px 6px; border-radius: 4px;
    background: var(--kb-surface); border: 1px solid var(--kb-border);
  }
</style>
