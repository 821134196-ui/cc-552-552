import { writable } from 'svelte/store'

function current() {
  return location.hash.replace(/^#/, '') || '/'
}

export const route = writable(current())

window.addEventListener('hashchange', () => route.set(current()))

export function nav(path) {
  location.hash = path
}
