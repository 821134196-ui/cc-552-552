<script>
  import { onMount } from 'svelte'
  import { api } from '../api.js'
  import StatusBadge from '../components/StatusBadge.svelte'

  let printJobs = []
  let notifications = []
  let error = ''
  let notice = ''

  // 手动触发模拟接口
  let printAppId = ''
  let printContent = ''
  let printFail = false
  let courierAppId = ''

  async function load() {
    error = ''
    try {
      ;[printJobs, notifications] = await Promise.all([api.printJobs(), api.courierNotifications()])
    } catch (e) {
      error = e.message
    }
  }

  onMount(load)

  async function triggerPrint() {
    error = ''
    notice = ''
    try {
      const job = await api.mockPrint(Number(printAppId), printContent || undefined, printFail)
      notice = `打印作业 #${job.id} 已${job.status === 'failed' ? '失败（申请已暂停）' : '完成'}`
      await load()
    } catch (e) {
      error = e.message
    }
  }

  async function triggerCourier() {
    error = ''
    notice = ''
    try {
      const n = await api.mockCourier(Number(courierAppId))
      notice = `快递通知 #${n.id} 已发送`
      await load()
    } catch (e) {
      error = e.message
    }
  }

  function pretty(payload) {
    try {
      return JSON.stringify(JSON.parse(payload), null, 2)
    } catch (e) {
      return payload
    }
  }
</script>

<h1>本地模拟接口</h1>
<p class="muted" style="margin-top:-8px">
  打印与快递通知均通过本地模拟接口完成，所有调用在此留痕。
</p>

{#if error}
  <div class="banner error">{error}</div>
{/if}
{#if notice}
  <div class="banner success">{notice}</div>
{/if}

<div class="card">
  <h2>手动触发</h2>
  <div class="form-grid">
    <div>
      <label class="field">
        <span>申请 ID（模拟打印）</span>
        <input bind:value={printAppId} placeholder="如 1" />
      </label>
      <label class="field">
        <span>打印内容（留空则按申请生成）</span>
        <textarea rows="3" bind:value={printContent}></textarea>
      </label>
      <div class="btn-row">
        <label style="display:flex;align-items:center;gap:6px">
          <input type="checkbox" style="width:auto" bind:checked={printFail} />
          模拟失败
        </label>
        <button class="primary" on:click={triggerPrint} disabled={!printAppId}>调用打印接口</button>
      </div>
    </div>
    <div>
      <label class="field">
        <span>申请 ID（模拟快递通知）</span>
        <input bind:value={courierAppId} placeholder="如 1" />
      </label>
      <button class="primary" on:click={triggerCourier} disabled={!courierAppId}>调用快递通知接口</button>
    </div>
  </div>
</div>

<div class="card">
  <h2>打印作业记录</h2>
  {#if printJobs.length === 0}
    <p class="empty">暂无打印作业</p>
  {:else}
    <table>
      <thead>
        <tr><th>#</th><th>申请</th><th>内容</th><th>状态</th><th>说明</th><th>时间</th></tr>
      </thead>
      <tbody>
        {#each printJobs as j}
          <tr>
            <td>{j.id}</td>
            <td>#{j.application_id}</td>
            <td style="white-space:pre-wrap">{j.content}</td>
            <td><StatusBadge status={j.status} /></td>
            <td>{j.message || ''}</td>
            <td>{j.created_at}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<div class="card">
  <h2>快递通知记录</h2>
  {#if notifications.length === 0}
    <p class="empty">暂无快递通知</p>
  {:else}
    {#each notifications as n}
      <p class="muted" style="margin:6px 0">#{n.id} · 申请 #{n.application_id} · {n.created_at}</p>
      <pre class="payload">{pretty(n.payload)}</pre>
    {/each}
  {/if}
</div>

<div class="btn-row">
  <button on:click={load}>刷新</button>
</div>
