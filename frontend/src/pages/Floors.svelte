<script>
  import { onMount } from 'svelte'
  import { api, DOOR_STATUS_LABELS } from '../api.js'

  let floors = []
  let currentFloor = null
  let data = null
  let error = ''

  onMount(async () => {
    try {
      const plates = await api.doorplates()
      floors = [...new Set(plates.map((p) => p.floor))].sort((a, b) => a - b)
      if (floors.length > 0) {
        currentFloor = floors[0]
        await selectFloor(currentFloor)
      }
    } catch (e) {
      error = e.message
    }
  })

  async function selectFloor(f) {
    currentFloor = f
    error = ''
    try {
      data = await api.wayfinding(f)
    } catch (e) {
      error = e.message
    }
  }
</script>

<h1>楼层导视</h1>
<p class="muted" style="margin-top:-8px">
  实时展示各房间状态，并标出即将发生的搬迁 / 更名变化。
</p>

{#if error}
  <div class="banner error">{error}</div>
{/if}

<div class="floor-tabs">
  {#each floors as f}
    <button class:active={currentFloor === f} on:click={() => selectFloor(f)}>{f}F</button>
  {/each}
</div>

{#if data}
  <div class="room-grid">
    {#each data.doorplates as d}
      <div class="room-card {d.status}">
        <div style="display:flex;justify-content:space-between;align-items:center">
          <span class="room-no">{d.room}</span>
          <span class="badge badge-{d.status}">{DOOR_STATUS_LABELS[d.status] || d.status}</span>
        </div>
        <div class="room-tenant">{d.tenant_name || '—'}</div>

        {#if d.status === 'forwarding'}
          <div class="room-note">
            旧址过渡：快件转发至 <strong>{d.forwarding_to}</strong>，截止 {d.forwarding_until}；期间不可分配给新租户
          </div>
        {/if}

        {#if d.upcoming}
          <div class="upcoming {d.upcoming.kind}">
            {#if d.upcoming.kind === 'move_in'}
              ↓ 即将入驻：{d.upcoming.tenant_name}
              {#if d.upcoming.other_label}（原 {d.upcoming.other_label}）{/if}
              <br />切换日：{d.upcoming.effective_date}
            {:else}
              ↑ 即将迁出：{d.upcoming.tenant_name}
              {#if d.upcoming.other_label}→ {d.upcoming.other_label}{/if}
              <br />切换日：{d.upcoming.effective_date}
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{:else if !error}
  <p class="empty">加载中…</p>
{/if}
