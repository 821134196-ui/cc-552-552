<script>
  import { onMount } from 'svelte'
  import { api, DOOR_STATUS_LABELS } from '../api.js'
  import { nav } from '../router.js'

  let tenants = []
  let doorplates = []
  let error = ''
  let info = ''
  let submitting = false

  let form = {
    tenant_id: '',
    change_type: 'move',
    new_name: '',
    old_doorplate_id: '',
    target_doorplate_id: '',
    lease_no: '',
    lease_start: '',
    lease_end: '',
    material: '',
    effective_date: '',
  }

  onMount(async () => {
    try {
      ;[tenants, doorplates] = await Promise.all([api.tenants(), api.doorplates()])
    } catch (e) {
      error = e.message
    }
  })

  $: tenantDoorplates = doorplates.filter((d) => d.tenant_id === Number(form.tenant_id))
  // 目标门牌展示全部（含占用/转发），让用户能直观看到服务端拦截
  $: targetOptions = doorplates.filter((d) => d.id !== Number(form.old_doorplate_id))

  function doorLabel(d) {
    const status = DOOR_STATUS_LABELS[d.status] || d.status
    const tenant = d.tenant_name ? ` · ${d.tenant_name}` : ''
    const reserved = d.reserved_by_application ? ` · 已被申请 #${d.reserved_by_application} 占用` : ''
    return `${d.floor}F ${d.room}（${status}${tenant}${reserved}）`
  }

  async function submit() {
    error = ''
    info = ''
    submitting = true
    try {
      const payload = {
        tenant_id: Number(form.tenant_id),
        change_type: form.change_type,
        lease_no: form.lease_no,
        lease_start: form.lease_start,
        effective_date: form.effective_date,
        material: form.material,
      }
      if (form.lease_end) payload.lease_end = form.lease_end
      if (form.change_type === 'rename') {
        payload.new_name = form.new_name
        payload.old_doorplate_id = Number(form.old_doorplate_id)
      } else {
        payload.old_doorplate_id = Number(form.old_doorplate_id)
        payload.target_doorplate_id = Number(form.target_doorplate_id)
      }
      const created = await api.submit(payload)
      info = `申请 #${created.id} 已提交`
      setTimeout(() => nav(`/applications/${created.id}`), 400)
    } catch (e) {
      error = e.message
    } finally {
      submitting = false
    }
  }
</script>

<h1>新建变更申请</h1>

{#if error}
  <div class="banner error">提交被拦截：{error}</div>
{/if}
{#if info}
  <div class="banner success">{info}</div>
{/if}

<div class="card">
  <h2>租户与变更类型</h2>
  <div class="form-grid">
    <label class="field">
      <span>租户</span>
      <select bind:value={form.tenant_id}>
        <option value="">请选择租户</option>
        {#each tenants as t}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span>变更类型</span>
      <select bind:value={form.change_type}>
        <option value="move">搬迁（换门牌）</option>
        <option value="rename">更名（门牌不变）</option>
      </select>
    </label>
    {#if form.change_type === 'rename'}
      <label class="field">
        <span>新名称</span>
        <input bind:value={form.new_name} placeholder="更名后的租户名称" />
      </label>
    {/if}
  </div>
</div>

<div class="card">
  <h2>门牌</h2>
  <div class="form-grid">
    <label class="field">
      <span>{form.change_type === 'rename' ? '门牌（更名不换址）' : '旧址门牌'}</span>
      <select bind:value={form.old_doorplate_id}>
        <option value="">请选择</option>
        {#each tenantDoorplates as d}
          <option value={d.id}>{d.floor}F {d.room}</option>
        {/each}
      </select>
      {#if form.tenant_id && tenantDoorplates.length === 0}
        <span class="muted">该租户当前没有已占用的门牌</span>
      {/if}
    </label>
    {#if form.change_type === 'move'}
      <label class="field">
        <span>目标门牌（占用/转发中的会被服务端拦截）</span>
        <select bind:value={form.target_doorplate_id}>
          <option value="">请选择</option>
          {#each targetOptions as d}
            <option value={d.id}>{doorLabel(d)}</option>
          {/each}
        </select>
      </label>
    {/if}
  </div>
</div>

<div class="card">
  <h2>租约与生效日</h2>
  <div class="form-grid">
    <label class="field">
      <span>租约编号</span>
      <input bind:value={form.lease_no} placeholder="如 ZL-2026-1001" />
    </label>
    <label class="field">
      <span>租约起始日</span>
      <input type="date" bind:value={form.lease_start} />
    </label>
    <label class="field">
      <span>租约截止日（可选）</span>
      <input type="date" bind:value={form.lease_end} />
    </label>
    <label class="field">
      <span>统一切换生效日（导视 / 门牌 / 快递同日切换）</span>
      <input type="date" bind:value={form.effective_date} />
    </label>
    <label class="field">
      <span>租约材料（文件名）</span>
      <input bind:value={form.material} placeholder="如 lease-2026-1001.pdf" />
    </label>
  </div>
</div>

<div class="btn-row">
  <button class="primary" on:click={submit} disabled={submitting}>
    {submitting ? '提交中…' : '提交申请'}
  </button>
  <button on:click={() => nav('/')}>返回列表</button>
</div>
