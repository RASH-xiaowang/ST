// 确保 Vite 开发端口 1420 空闲后再启动。
// `npm run tauri dev` 的 beforeDevCommand 会先运行本脚本：清理上次异常退出
// 残留的 vite 进程，避免 "Port 1420 is already in use" 导致启动失败。
import { execSync } from 'node:child_process';

const PORT = 1420;

function findListeners() {
  try {
    // 注意不能用 `-p tcp`：部分 Windows 上会过滤掉 IPv6 监听（[::1]:1420）
    const out = execSync(`netstat -ano | findstr :${PORT}`, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const pids = new Set();
    for (const line of out.split(/\r?\n/)) {
      if (!/LISTENING/i.test(line)) continue;
      const m = line.trim().match(/(\d+)\s*$/);
      if (m) pids.add(m[1]);
    }
    return [...pids];
  } catch {
    return []; // netstat 无匹配 = 端口空闲
  }
}

function processName(pid) {
  try {
    const out = execSync(`tasklist /FI "PID eq ${pid}" /FO CSV /NH`, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    return out.split(',')[0]?.replaceAll('"', '').trim() || '';
  } catch {
    return '';
  }
}

for (const pid of findListeners()) {
  const name = processName(pid);
  // 只清理 node（vite）残留，避免误杀其它占用 1420 的服务
  if (!/^node(\.exe)?$/i.test(name)) {
    console.warn(
      `[ensure-port-1420] 端口 ${PORT} 被非 node 进程占用（PID=${pid} ${name}），请手动处理`
    );
    continue;
  }
  try {
    process.kill(Number(pid));
    console.log(`[ensure-port-1420] 已清理残留 vite 进程 PID=${pid}`);
  } catch (e) {
    console.warn(`[ensure-port-1420] 结束 PID ${pid} 失败: ${e.message}`);
  }
}
