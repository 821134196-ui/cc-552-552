<script>
  import { onMount } from 'svelte'
  import { api, STATUS_LABELS, CHANGE_TYPE_LABELS } from '../api.js'
  import { nav } from '../router.js'
  import StatusBadge from '../components/StatusBadge.svelte'

  export let id

  let detail = null
  let error = ''
  let notice = ''
  let busy = false

  // 操作表单
  let rejectReason = ''
  let simulateFailure = false
  let photoData = ''
  let photoName = ''
  let acceptedBy = ''
  let pauseReason = ''

  async function load() {
    error = ''
    try {
      detail = await api.application(id)
    } catch (e) {
      error = e.message
    }
  }

  onMount(load)

  async function run(fn, successMsg) {
    busy = true
    error = ''
    notice = ''
    try {
      await fn()
      if (successMsg) notice = successMsg
      await load()
    } catch (e) {
      error = e.message
    } finally {
      busy = false
    }
  }

  const approve = () => run(() => api.review(id, true), '审核已通过')
  const reject = () => {
    if (!rejectReason.trim()) {
      error = '驳回时请填写原因'
      return
    }
    run(() => api.review(id, false, rejectReason), '已驳回')
  }
  const produce = () =>
    run(
      () => api.produce(id, simulateFailure),
      simulateFailure ? undefined : '门牌制作完成'
    )
  const install = () => {
    if (!photoData) {
      error = '请先选择安装照片'
      return
    }
    run(() => api.install(id, photoData), '安装完成，照片已归档')
  }
  const accept = () => {
    if (!acceptedBy.trim()) {
      error = '请填写验收人'
      return
    }
    run(() => api.accept(id, acceptedBy.trim()), '验收通过')
  }
  const publish = () =>
    run(async () => {
      const outcome = await api.publish(id)
      if (outcome.result === 'published') notice = outcome.message
      else if (outcome.result === 'already_published') notice = `幂等保护：${outcome.message}`
      else if (outcome.result === 'paused') error = outcome.message
    })
  const pause = () => {
    if (!pauseReason.trim()) {
      error = '请填写暂停原因'
      return
    }
    run(() => api.pause(id, pauseReason.trim()), '已暂停')
  }
  const resume = () => run(() => api.resume(id), '已恢复')

  function onPhoto(e) {
    const file = e.target.files && e.target.files[0]
    if (!file) return
    photoName = file.name
    const reader = new FileReader()
    reader.onload = () => (photoData = reader.result)
    reader.readAsDataURL(file)
  }

  $: app = detail && detail.application
  $: isActive = app && ['submitted', 'approved', 'producing', 'installed', 'accepted'].includes(app.status)
</script>

{#if error}
  <div class="banner error">{error}</div>
{/if}
{#if notice}
  <div class="banner success">{notice}</div>
{/if}

{#if !detail && !error}
  <div class="card"><p class="empty">加载中…</p></div>
{:else if app}
  <h1>
    申请 #{app.id}
    <StatusBadge status={app.status} />
  </h1>

  {#if app.status === 'paused'}
    <div class="banner error">
      <strong>已暂停发布：</strong>{app.pause_reason}
      <div style="margin-top:8px">
        <button class="primary" on:click={resume} disabled={busy}>问题已处理，恢复流程</button>
      </div>
    </div>
  {/if}

  <div class="card">
    <h2>申请信息</h2>
    <div class="info-grid">
      <div class="item"><div class="k">租户</div><div class="v">{app.tenant_name}</div></div>
      <div class="item"><div class="k">变更类型</div><div class="v">{CHANGE_TYPE_LABELS[app.change_type] || app.change_type}</div></div>
      {#if app.new_name}
        <div class="item"><div class="k">新名称</div><div class="v">{app.new_name}</div></div>
      {/if}
      <div class="item"><div class="k">旧址</div><div class="v">{app.old_label || '—'}</div></div>
      <div class="item"><div class="k">新址</div><div class="v">{app.target_label || '—'}</div></div>
      <div class="item"><div class="k">统一切换生效日</div><div class="v">{app.effective_date}</div></div>
      <div class="item"><div class="k">租约编号</div><div class="v">{app.lease_no}</div></div>
      <div class="item"><div class="k">租约期限</div><div class="v">{app.lease_start} ~ {app.lease_end || '长期'}</div></div>
      <div class="item"><div class="k">租约材料</div><div class="v">{app.material || '—'}</div></div>
      {#if app.accepted_by}
        <div class="item"><div class="k">验收人</div><div class="v">{app.accepted_by}</div></div>
      {/if}
      {#if app.published_at}
        <div class="item"><div class="k">发布时间</div><div class="v">{app.published_at}</div></div>
      {/if}
    </div>
    {#if app.install_photo}
      <div style="margin-top:14px">
        <div class="k muted" style="font-size:12px">安装照片</div>
        {#if app.install_photo.startsWith('data:')}
          <img class="photo-preview" src={app.install_photo} alt="安装照片" />
        {:else}
          <div>{app.install_photo}</div>
        {/if}
      </div>
    {/if}
  </div>

  <div class="card">
    <h2>流程操作</h2>
    {#if app.status === 'submitted'}
      <div class="btn-row">
        <button class="primary" on:click={approve} disabled={busy}>审核通过</button>
        <input style="max-width:260px" bind:value={rejectReason} placeholder="驳回原因" />
        <button class="danger" on:click={reject} disabled={busy}>驳回</button>
      </div>
    {:else if app.status === 'approved'}
      <div class="btn-row">
        <button class="primary" on:click={produce} disabled={busy}>开始制作（调用模拟打印接口）</button>
        <label style="display:flex;align-items:center;gap:6px">
          <input type="checkbox" style="width:auto" bind:checked={simulateFailure} />
          模拟打印失败（演示「制作有误」暂停）
        </label>
      </div>
    {:else if app.status === 'producing'}
      <div class="btn-row">
        <input type="file" accept="image/*" style="max-width:280px" on:change={onPhoto} />
        <button class="primary" on:click={install} disabled={busy || !photoData}>
          登记安装{photoName ? `（${photoName}）` : ''}
        </button>
      </div>
    {:else if app.status === 'installed'}
      <div class="btn-row">
        <input style="max-width:220px" bind:value={acceptedBy} placeholder="验收人姓名" />
        <button class="primary" on:click={accept} disabled={busy}>验收通过</button>
      </div>
    {:else if app.status === 'accepted'}
      <div class="btn-row">
        <button class="primary" on:click={publish} disabled={busy}>发布生效（统一切换）</button>
        <span class="muted">发布将校验租约生效与门牌占用，并同步导视与快递地址</span>
      </div>
    {:else if app.status === 'published'}
      <div class="banner info" style="margin-bottom:10px">
        该申请已发布。点击下方按钮可演示「重复发布只执行一次切换」——系统会幂等返回，不会二次切换。
      </div>
      <button on:click={publish} disabled={busy}>再次发布（演示幂等）</button>
    {:else if app.status === 'rejected'}
      <p class="muted">该申请已被驳回。</p>
    {:else if app.status === 'paused'}
      <p class="muted">请先处理暂停原因并恢复流程。</p>
    {/if}

    {#if isActive}
      <div style="margin-top:14px; border-top:1px solid var(--border); padding-top:14px" class="btn-row">
        <input style="max-width:260px" bind:value={pauseReason} placeholder="人工暂停原因" />
        <button class="danger" on:click={pause} disabled={busy}>暂停发布</button>
      </div>
    {/if}
  </div>

  <div class="card">
    <h2>状态流转记录</h2>
    {#if detail.events.length === 0}
      <p class="empty">暂无记录</p>
    {:else}
      <ul class="timeline">
        {#each [...detail.events].reverse() as ev}
          <li>
            <div class="t-status">
              {ev.from_status ? STATUS_LABELS[ev.from_status] || ev.from_status : '（新建）'}
              <span class="arrow">→</span>
              {STATUS_LABELS[ev.to_status] || ev.to_status}
            </div>
            {#if ev.reason}
              <div class="t-reason">{ev.reason}</div>
            {/if}
            <div class="t-meta">{ev.created_at} · 操作人：{ev.actor}</div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="card">
    <h2>打印作业（模拟接口）</h2>
    {#if detail.print_jobs.length === 0}
      <p class="empty">暂无打印作业</p>
    {:else}
      <table>
        <thead>
          <tr><th>#</th><th>内容</th><th>状态</th><th>说明</th><th>时间</th></tr>
        </thead>
        <tbody>
          {#each detail.print_jobs as j}
            <tr>
              <td>{j.id}</td>
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
    <h2>快递通知（模拟接口）</h2>
    {#if detail.courier_notifications.length === 0}
      <p class="empty">暂无快递通知</p>
    {:else}
      {#each detail.courier_notifications as n}
        <p class="muted" style="margin:6px 0">#{n.id} · {n.created_at}</p>
        <pre class="payload">{JSON.stringify(JSON.parse(n.payload), null, 2)}</pre>
      {/each}
    {/if}
  </div>

  <div class="btn-row">
    <button on:click={() => nav('/')}>返回列表</button>
    <button on:click={load} disabled={busy}>刷新</button>
  </div>
{/if}
