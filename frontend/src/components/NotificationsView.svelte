<script>
  import { onMount } from 'svelte';
  import { api } from '../lib/api.js';

  let items = $state([]);

  onMount(async () => {
    const d = await api.notifications();
    items = d.notifications;
  });

  const fmt = (s) => new Date(s).toLocaleString('zh-CN', { hour12: false });
</script>

<div class="panel">
  <h2>本地模拟接口调用记录</h2>
  <p class="muted" style="margin-top:0">
    打印车间与快递网点均为本地模拟接口，不产生真实外发。重复发布不会新增记录。
  </p>
  <table>
    <thead>
      <tr><th>时间</th><th>申请</th><th>类型</th><th>接口</th><th>内容</th><th>回执</th></tr>
    </thead>
    <tbody>
      {#each items as n (n.id)}
        <tr>
          <td class="muted" style="white-space:nowrap">{fmt(n.createdAt)}</td>
          <td class="mono">{n.applicationId ?? '—'}</td>
          <td>
            <span class="pill" style={n.kind === 'print'
              ? 'background:#ede9fe;color:#6d28d9'
              : 'background:#dbeafe;color:#1e40af'}>
              {n.kind === 'print' ? '打印' : '快递'}
            </span>
          </td>
          <td class="mono" style="font-size:12px">{n.target}</td>
          <td class="mono" style="font-size:11px;max-width:420px;word-break:break-all">
            {n.payload}
          </td>
          <td class="mono" style="font-size:11px">{n.result}</td>
        </tr>
      {:else}
        <tr><td colspan="6" class="muted">暂无记录</td></tr>
      {/each}
    </tbody>
  </table>
</div>
