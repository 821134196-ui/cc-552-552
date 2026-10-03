<script>
  import { onMount } from 'svelte';
  import { api } from '../lib/api.js';
  import { STATUS_LABELS, STATUS_COLORS } from '../lib/status.js';

  let { id, rooms, tenants, onBack, onChanged, onFlash } = $props();
  let app = $state(null);

  let fabNote = $state('');
  let installer = $state('王工');
  let installPhoto = $state('photos/install-001.jpg');
  let acceptedBy = $state('物业经理 李珊');

  // blocked 后修正资料
  let patch = $state({ leaseEffectiveDate: '', effectiveDate: '', newRoomId: '' });

  async function load() {
    const d = await api.application(id);
    app = d.application;
    patch.leaseEffectiveDate = app.leaseEffectiveDate;
    patch.effectiveDate = app.effectiveDate;
    patch.newRoomId = String(app.newRoomId);
  }
  onMount(load);

  async function run(label, fn) {
    try {
      await fn();
      onFlash('ok', label);
    } catch (e) {
      onFlash('error', e.message);
    }
    await load();
    await onChanged();
  }

  const fmtTime = (s) => (s ? new Date(s).toLocaleString('zh-CN', { hour12: false }) : '');
</script>

{#if app}
  <button class="btn subtle" onclick={onBack}>‹ 返回列表</button>

  <div class="panel" style="margin-top:12px">
    <div class="row">
      <h2 style="margin:0" class="mono">{app.code}</h2>
      <span class="badge" style={`background:${STATUS_COLORS[app.status]}1a;color:${STATUS_COLORS[app.status]}`}>
        {STATUS_LABELS[app.status]}
      </span>
      <span class="spacer"></span>
      <span class="muted">创建于 {fmtTime(app.createdAt)}</span>
    </div>

    {#if app.status === 'blocked'}
      <div class="alert error" style="margin-top:12px">
        <strong>已暂停发布</strong>（环节：{app.blockedStage}）：{app.blockReason}
      </div>
    {/if}

    <dl class="detail-grid" style="margin-top:12px">
      <dt>租户名称</dt><dd>{app.tenantName}</dd>
      <dt>旧址</dt>
      <dd>{app.oldRoomCode ?? '—（新租户）'}</dd>
      <dt>新址</dt>
      <dd><strong>{app.newRoomCode}</strong></dd>
      <dt>租约材料</dt><dd class="mono">{app.leaseRef}</dd>
      <dt>租约生效日</dt><dd>{app.leaseEffectiveDate}</dd>
      <dt>统一切换日</dt><dd><strong>{app.effectiveDate}</strong></dd>
      <dt>安装人</dt><dd>{app.installer ?? '—'}</dd>
      <dt>安装照片</dt>
      <dd>{#if app.installPhoto}<span class="mono">{app.installPhoto}</span>{:else}—{/if}</dd>
      <dt>验收人</dt><dd>{app.acceptedBy ?? '—'}</dd>
      <dt>验收时间</dt><dd>{fmtTime(app.acceptedAt) || '—'}</dd>
      <dt>发布时间</dt><dd>{fmtTime(app.publishedAt) || '—'}</dd>
      <dt>已执行切换</dt><dd>{app.publishedOnce ? '是（重复发布不会二次切换）' : '否'}</dd>
    </dl>
  </div>

  <!-- 物业操作区 -->
  <div class="panel">
    <h2>办理动作</h2>

    {#if app.status === 'submitted'}
      <div class="row">
        <button class="btn" onclick={() => run('已审核通过，目标门牌完成预约', () => api.approve(app.id))}>
          审核通过（占用新门牌）
        </button>
        <span class="muted">将校验：门牌未被占用/预约、非过渡转发门牌、租约不晚于切换日</span>
      </div>
    {/if}

    {#if app.status === 'approved' || (app.status === 'blocked' && app.blockedStage === 'fabrication')}
      <h3>门牌制作</h3>
      <div class="row">
        <input bind:value={fabNote} placeholder="制作备注（可说明刻错内容）"
               style="flex:1;min-width:220px;padding:8px 10px;border:1px solid var(--line);border-radius:6px" />
        <button class="btn" onclick={() => run('制作合格，已向打印车间下单（模拟）',
          () => api.fabricate(app.id, true, fabNote || '制作合格'))}>制作合格</button>
        <button class="btn danger" onclick={() => run('已记录制作问题并暂停发布',
          () => api.fabricate(app.id, false, fabNote || '门牌制作有误'))}>制作有误·暂停</button>
      </div>
    {/if}

    {#if app.status === 'fabricated'}
      <h3>现场安装（照片与安装人存入申请）</h3>
      <div class="grid cols-3">
        <label class="field">安装人<input bind:value={installer} /></label>
        <label class="field">安装照片路径/URL<input bind:value={installPhoto} /></label>
      </div>
      <button class="btn" onclick={() => run('安装信息已保存',
        () => api.install(app.id, installer, installPhoto))}>确认安装完成</button>
    {/if}

    {#if app.status === 'installed'}
      <h3>物业验收</h3>
      <div class="row">
        <label class="field" style="margin:0">验收人
          <input bind:value={acceptedBy} style="width:220px" />
        </label>
        <button class="btn" onclick={() => run('验收通过，可发布',
          () => api.accept(app.id, acceptedBy))}>验收通过</button>
      </div>
    {/if}

    {#if app.status === 'accepted'}
      <h3>统一发布</h3>
      <div class="row">
        <button class="btn" onclick={() => run('已在统一切换日完成门牌/导视/快递更新',
          () => api.publish(app.id))}>立即发布切换</button>
        <span class="muted">
          新址门牌启用并入住；旧址进入快件转发过渡期（14 天），不再作为新租户地址。
        </span>
      </div>
    {/if}

    {#if app.status === 'published'}
      <div class="alert ok">
        已发布生效。重复点按“发布”不会产生第二次切换，也不会重复通知打印车间/快递网点。
      </div>
      <button class="btn ghost" onclick={() => run('重复发布已被忽略（幂等）',
        () => api.publish(app.id))}>再次发布（验证幂等）</button>
    {/if}

    {#if app.status === 'blocked' && app.blockedStage !== 'fabrication'}
      <h3>修正资料后重新提交</h3>
      <div class="grid cols-3">
        <label class="field">新门牌
          <select bind:value={patch.newRoomId}>
            {#each rooms as r (r.id)}
              <option value={r.id}>{r.floor} {r.code}</option>
            {/each}
          </select>
        </label>
        <label class="field">租约生效日<input type="date" bind:value={patch.leaseEffectiveDate} /></label>
        <label class="field">统一切换日<input type="date" bind:value={patch.effectiveDate} /></label>
      </div>
      <button class="btn" onclick={() => run('资料已修正，重新进入待审核',
        () => api.resubmit(app.id, {
          newRoomId: Number(patch.newRoomId),
          leaseEffectiveDate: patch.leaseEffectiveDate,
          effectiveDate: patch.effectiveDate,
        }))}>修正并重新提交</button>
    {/if}
  </div>

  <div class="grid cols-2">
    <div class="panel">
      <h2>状态流转</h2>
      <ul class="timeline">
        {#each app.events as e (e.id)}
          <li class={e.toStatus === 'blocked' ? 'blocked' : ''}>
            <div>
              <strong>{STATUS_LABELS[e.toStatus] ?? e.toStatus}</strong>
              <span class="muted"> · {e.actor}</span>
            </div>
            {#if e.note}<div class="muted">{e.note}</div>{/if}
            <div class="when">{fmtTime(e.createdAt)}</div>
          </li>
        {/each}
      </ul>
    </div>

    <div class="panel">
      <h2>打印 / 快递模拟接口记录</h2>
      {#if app.notifications.length === 0}
        <p class="muted">尚无外部通知。制作合格后产生打印单；发布时产生快递改址与旧址转发。</p>
      {:else}
        <table>
          <thead><tr><th>类型</th><th>接收方</th><th>结果</th></tr></thead>
          <tbody>
            {#each app.notifications as n (n.id)}
              <tr>
                <td><span class="pill" style={n.kind === 'print'
                  ? 'background:#ede9fe;color:#6d28d9'
                  : 'background:#dbeafe;color:#1e40af'}>
                  {n.kind === 'print' ? '打印车间' : '快递网点'}
                </span></td>
                <td class="mono" style="font-size:12px">{n.target}</td>
                <td class="mono" style="font-size:12px">{n.result}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </div>
{:else}
  <div class="panel muted">加载中…</div>
{/if}
