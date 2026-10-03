<script>
  import { onMount } from 'svelte';
  import { api } from '../lib/api.js';
  import { STATUS_LABELS, STATUS_COLORS, ROOM_STATUS_LABELS } from '../lib/status.js';

  let { onSelect } = $props();
  let data = $state(null);

  async function load() {
    data = await api.guide();
  }
  onMount(load);
</script>

{#if data}
  <div class="panel">
    <h2>前台变更公告 · 旧址 / 新址 / 切换日期</h2>
    {#if data.changes.length === 0}
      <p class="muted">暂无进行中的门牌变更。</p>
    {:else}
      <table>
        <thead>
          <tr>
            <th>申请编号</th><th>租户</th><th>旧址</th><th></th><th>新址</th>
            <th>统一切换日</th><th>状态</th><th></th>
          </tr>
        </thead>
        <tbody>
          {#each data.changes as c (c.applicationId)}
            <tr class="clickable" onclick={() => onSelect(c.applicationId)}>
              <td class="mono">{c.applicationCode}</td>
              <td>{c.tenantName}</td>
              <td>{c.oldRoomCode ?? '—（新租户）'}</td>
              <td>→</td>
              <td><strong>{c.newRoomCode}</strong></td>
              <td>{c.effectiveDate}</td>
              <td>
                <span class="badge" style={`background:${STATUS_COLORS[c.status]}1a;color:${STATUS_COLORS[c.status]}`}>
                  {STATUS_LABELS[c.status]}
                </span>
              </td>
              <td class="muted">查看 ›</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  {#each data.floors as floor (floor.floor)}
    <div class="floor-title">{floor.floor} 楼层导视</div>
    <div class="room-grid">
      {#each floor.rooms as room (room.id)}
        <div class="room-card {room.status}">
          <div class="row">
            <span class="code">{room.code}</span>
            <span class="spacer"></span>
            <span class="pill" style="background:#f1f5f9;color:#475569">
              {ROOM_STATUS_LABELS[room.status]}
            </span>
          </div>
          <div class="tenant">{room.tenantName ?? '（空置）'}</div>
          <div class="label">{room.label}</div>

          {#if room.incoming}
            <div class="incoming-tag">
              即将入住：<strong>{room.incoming.tenantName}</strong><br />
              {room.incoming.applicationStatus === 'published' ? '已于 ' : '切换日 '}
              {room.incoming.effectiveDate} 生效
              <span class="mono">（{room.incoming.applicationCode}）</span>
            </div>
          {/if}

          {#if room.status === 'transition'}
            <div class="forward-tag">
              旧址过渡中：寄往本门牌的快件转发至 <strong>{room.forwardToRoomCode}</strong>，
              截止 {room.forwardUntil}；本门牌不再作为新租户地址。
              {#if room.forwardTenantName}<br />原租户：{room.forwardTenantName}{/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/each}
{:else}
  <div class="panel muted">加载中…</div>
{/if}
