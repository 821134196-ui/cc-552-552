<script>
  import { route } from './router.js'
  import Applications from './pages/Applications.svelte'
  import ApplicationNew from './pages/ApplicationNew.svelte'
  import ApplicationDetail from './pages/ApplicationDetail.svelte'
  import Directory from './pages/Directory.svelte'
  import Floors from './pages/Floors.svelte'
  import Mock from './pages/Mock.svelte'

  const links = [
    { href: '/', label: '变更申请' },
    { href: '/new', label: '新建申请' },
    { href: '/directory', label: '前台导视' },
    { href: '/floors', label: '楼层导视' },
    { href: '/mock', label: '模拟接口' },
  ]

  $: path = $route
  $: detailMatch = path.match(/^\/applications\/(\d+)$/)

  function isActive(href, path) {
    if (href === '/') return path === '/' || path === '/applications' || /^\/applications\//.test(path)
    return path === href || path.startsWith(href + '/')
  }
</script>

<div class="layout">
  <header class="topbar">
    <div class="brand">写字楼门牌变更与引导系统</div>
    <nav>
      {#each links as l}
        <a href={'#' + l.href} class:active={isActive(l.href, path)}>{l.label}</a>
      {/each}
    </nav>
  </header>
  <main>
    {#if path === '/' || path === '/applications'}
      <Applications />
    {:else if path === '/new'}
      <ApplicationNew />
    {:else if detailMatch}
      <ApplicationDetail id={Number(detailMatch[1])} />
    {:else if path === '/directory'}
      <Directory />
    {:else if path === '/floors'}
      <Floors />
    {:else if path === '/mock'}
      <Mock />
    {:else}
      <div class="card"><p class="empty">页面不存在</p></div>
    {/if}
  </main>
</div>
