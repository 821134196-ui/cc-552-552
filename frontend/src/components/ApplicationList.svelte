<script>
  import { onMount } from 'svelte';
  import { api } from '../lib/api.js';
  import { STATUS_LABELS, STATUS_COLORS } from '../lib/status.js';

  let { onSelect } = $props();
  let apps = $state([]);
  let filter = $state('all');

  async function load() {
    const d = await api.applications();
    apps = d.applications;
  }
  onMount(load);

  const shown = $derived(
    filter === 'all' ? apps : apps.filter((a) => a.status === filter)
  );
</script>

<div class="panel">
  <div class="row">
    <h2 style="margin:0">变更申请</h2>
    <span class="spacer"></span>
    <select bind:value={filter} style="padding:6px 10px;border:1px solid var(--line);border-radius:6px">
      <option value="all">全部状态</option>
      <option value="submitted">待审核</option>
      <option value="blocked">已暂停</option>
      <option value="accepted">待发布</option>
      <option value="published">已发布</option>
    </select>
    <button class="btn subtle" onclick={load}>刷新</button>
  </div>
</div>

<div class="panel">
  <table>
    <thead>
      <tr>
        <th>编号</th><th>租户</th><th>旧址</th><th></th><th>新址</th>
        <th>切换日</th><th>状态</th><th>暂停原因</th>
      </tr>
    </thead>
    <tbody>
      {#each shown as a (a.id)}
        <tr class="clickable" onclick={() => onSelect(a.id)}>
          <td class="mono">{a.code}</td>
          <td>{a.tenantName}</td>
          <td>{a.oldRoomCode ?? '—'}</td>
          <td>→</td>
          <td><strong>{a.newRoomCode}</strong></td>
          <td>{a.effectiveDate}</td>
          <td>
            <span class="badge" style={`background:${STATUS_COLORS[a.status]}1a;color:${STATUS_COLORS[a.status]}`}>
              {STATUS_LABELS[a.status]}
            </span>
          </td>
          <td class="muted">{a.blockReason ?? ''}</td>
        </tr>
      {:else}
        <tr><td colspan="8" class="muted">暂无申请</td></tr>
      {/each}
    </tbody>
  </table>
</div>
