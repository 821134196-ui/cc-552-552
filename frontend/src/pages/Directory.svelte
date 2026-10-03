<script>
  import { onMount } from 'svelte'
  import { api, CHANGE_TYPE_LABELS } from '../api.js'
  import { nav } from '../router.js'
  import StatusBadge from '../components/StatusBadge.svelte'

  let rows = []
  let error = ''
  let loading = true

  $: today = new Date().toISOString().slice(0, 10)

  onMount(async () => {
    try {
      rows = await api.directory()
    } catch (e) {
      error = e.message
    } finally {
      loading = false
    }
  })
</script>

<h1>前台导视 · 租户地址一览</h1>
<p class="muted" style="margin-top:-8px">
  供前台与快递员查询：旧址、新址与统一切换日期。切换日当天起按新址引导。
</p>

{#if error}
  <div class="banner error">{error}</div>
{/if}

<div class="card">
  {#if loading}
    <p class="empty">加载中…</p>
  {:else if rows.length === 0}
    <p class="empty">暂无变更记录</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>租户</th>
          <th>类型</th>
          <th>旧址</th>
          <th>新址</th>
          <th>切换日期</th>
          <th>状态</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as r}
          <tr class="clickable" on:click={() => nav(`/applications/${r.application_id}`)}>
            <td>
              {r.display_name}
              {#if r.change_type === 'rename' && r.display_name !== r.tenant_name}
                <span class="muted">（原 {r.tenant_name}）</span>
              {/if}
            </td>
            <td>{CHANGE_TYPE_LABELS[r.change_type] || r.change_type}</td>
            <td>{r.old_label || '—'}</td>
            <td><strong>{r.new_label || r.old_label || '—'}</strong></td>
            <td>
              {r.effective_date}
              {#if r.effective_date === today}
                <span class="badge badge-published">今日切换</span>
              {:else if r.effective_date > today && r.status !== 'published'}
                <span class="badge badge-submitted">即将切换</span>
              {/if}
            </td>
            <td><StatusBadge status={r.status} /></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
