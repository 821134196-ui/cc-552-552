<script>
  import { api } from '../lib/api.js';

  let { rooms, tenants, onCreated, onError } = $props();

  const today = new Date().toISOString().slice(0, 10);
  let form = $state({
    tenantName: '',
    tenantId: '',
    oldRoomId: '',
    newRoomId: '',
    leaseRef: '',
    leaseEffectiveDate: today,
    effectiveDate: today,
  });
  let submitting = $state(false);

  async function submit() {
    submitting = true;
    try {
      const payload = {
        tenantName: form.tenantName,
        tenantId: form.tenantId ? Number(form.tenantId) : null,
        oldRoomId: form.oldRoomId ? Number(form.oldRoomId) : null,
        newRoomId: Number(form.newRoomId),
        leaseRef: form.leaseRef,
        leaseEffectiveDate: form.leaseEffectiveDate,
        effectiveDate: form.effectiveDate,
      };
      const d = await api.create(payload);
      onCreated(d.application.id);
    } catch (e) {
      onError(e.message);
    } finally {
      submitting = false;
    }
  }
</script>

<div class="panel" style="max-width:720px">
  <h2>新建门牌变更 / 更名申请</h2>
  <p class="muted" style="margin-top:0">
    提交后由物业审核。审核通过即预约目标门牌；制作、安装、验收完成后，
    将在统一切换日同时更新实体门牌、楼层导视、前台名册与快递地址。
  </p>

  <div class="grid cols-2">
    <label class="field">
      变更后租户名称（更名即填新名称）
      <input bind:value={form.tenantName} placeholder="如：云岚科技有限公司" />
    </label>
    <label class="field">
      既有租户（新租户留空）
      <select bind:value={form.tenantId}>
        <option value="">— 新租户 —</option>
        {#each tenants as t (t.id)}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      旧门牌（更名/搬迁；无则留空）
      <select bind:value={form.oldRoomId}>
        <option value="">— 无旧址 —</option>
        {#each rooms as r (r.id)}
          <option value={r.id}>{r.floor} {r.code}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      新门牌 *
      <select bind:value={form.newRoomId}>
        <option value="">请选择</option>
        {#each rooms as r (r.id)}
          <option value={r.id}>{r.floor} {r.code}（{r.status === 'vacant' ? '空置' : r.status}）</option>
        {/each}
      </select>
    </label>
    <label class="field">
      租约材料编号 *
      <input bind:value={form.leaseRef} placeholder="如 L-2026-001" />
    </label>
    <label class="field">
      租约生效日 *
      <input type="date" bind:value={form.leaseEffectiveDate} />
    </label>
    <label class="field">
      统一切换日 *
      <input type="date" bind:value={form.effectiveDate} />
    </label>
  </div>

  <div class="row" style="margin-top:8px">
    <button class="btn" disabled={submitting} onclick={submit}>
      {submitting ? '提交中…' : '提交申请'}
    </button>
    <span class="muted">租约生效日晚于切换日、目标门牌被占用时，审核将被暂停并提示原因。</span>
  </div>
</div>
