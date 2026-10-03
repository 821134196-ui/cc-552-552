<script>
  import { onMount } from 'svelte'
  import { api, STATUS_LABELS, CHANGE_TYPE_LABELS } from '../api.js'
  import { nav } from '../router.js'
  import StatusBadge from '../components/StatusBadge.svelte'

  let items = []
  let statusFilter = ''
  let error = ''
  let loading = true

  async function load() {
    loading = true
    error = ''
    try {
      items = await api.applications(statusFilter || undefined)
    } catch (e) {
      error = e.message
    } finally {
      loading = false
    }
  }

  onMount(load)
</script>

<h1>变更申请</h1>

<div class="toolbar">
  <select bind:value={statusFilter} on:change={load}>
    <option value="">全部状态</option>
    {#each Object.entries(STATUS_LABELS) as [k, v]}
      <option value={k}>{v}</option>
    {/each}
  </select>
  <button on:click={load}>刷新</button>
  <button class="primary" on:click={() => nav('/new')}>新建申请</button>
</div>

{#if error}
  <div class="banner error">{error}</div>
{/if}

<div class="card">
  {#if loading}
    <p class="empty">加载中…</p>
  {:else if items.length === 0}
    <p class="empty">暂无申请</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>#</th>
          <th>租户</th>
          <th>类型</th>
          <th>旧址</th>
          <th>新址</th>
          <th>统一切换日</th>
          <th>状态</th>
        </tr>
      </thead>
      <tbody>
        {#each items as a}
          <tr class="clickable" on:click={() => nav(`/applications/${a.id}`)}>
            <td>{a.id}</td>
            <td>
              {a.tenant_name}
              {#if a.change_type === 'rename' && a.new_name}
                <span class="arrow">→</span>{a.new_name}
              {/if}
            </td>
            <td>{CHANGE_TYPE_LABELS[a.change_type] || a.change_type}</td>
            <td>{a.old_label || '—'}</td>
            <td>{a.target_label || '—'}</td>
            <td>{a.effective_date}</td>
            <td><StatusBadge status={a.status} /></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
