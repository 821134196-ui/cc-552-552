<script>
  import { onMount } from 'svelte';
  import { api } from './lib/api.js';
  import GuideView from './components/GuideView.svelte';
  import ApplicationList from './components/ApplicationList.svelte';
  import ApplicationDetail from './components/ApplicationDetail.svelte';
  import ApplicationForm from './components/ApplicationForm.svelte';
  import NotificationsView from './components/NotificationsView.svelte';

  let tab = $state('guide'); // guide | list | new | detail | notifications
  let selectedId = $state(null);
  let rooms = $state([]);
  let tenants = $state([]);
  let banner = $state(null); // {type, text}

  function flash(type, text) {
    banner = { type, text };
    setTimeout(() => (banner = null), 5000);
  }

  function go(next, id = null) {
    tab = next;
    selectedId = id;
  }

  async function loadBase() {
    const [r, t] = await Promise.all([api.rooms(), api.tenants()]);
    rooms = r.rooms;
    tenants = t.tenants;
  }

  onMount(loadBase);
</script>

<header class="topbar">
  <h1>写字楼租户门牌变更与引导更新系统</h1>
  <p>统一审核 · 门牌制作安装验收 · 同一生效日切换导视/门牌/快递地址</p>
</header>

<nav class="tabs">
  <button class={tab === 'guide' ? 'active' : ''} onclick={() => go('guide')}>楼层导视</button>
  <button class={tab === 'list' || tab === 'detail' ? 'active' : ''} onclick={() => go('list')}>
    变更申请
  </button>
  <button class={tab === 'new' ? 'active' : ''} onclick={() => go('new')}>新建申请</button>
  <button class={tab === 'notifications' ? 'active' : ''} onclick={() => go('notifications')}>
    打印 / 快递模拟记录
  </button>
</nav>

<main>
  {#if banner}
    <div class="alert {banner.type}">{banner.text}</div>
  {/if}

  {#if tab === 'guide'}
    <GuideView onSelect={(id) => go('detail', id)} />
  {:else if tab === 'list'}
    <ApplicationList onSelect={(id) => go('detail', id)} />
  {:else if tab === 'new'}
    <ApplicationForm {rooms} {tenants}
      onCreated={(id) => go('detail', id)}
      onError={(m) => flash('error', m)} />
  {:else if tab === 'detail'}
    <ApplicationDetail id={selectedId} {rooms} {tenants}
      onBack={() => go('list')}
      onChanged={() => loadBase()}
      onFlash={flash} />
  {:else if tab === 'notifications'}
    <NotificationsView />
  {/if}
</main>
