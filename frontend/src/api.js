async function req(method, url, body) {
  const res = await fetch(url, {
    method,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  let data = {}
  try {
    data = await res.json()
  } catch (e) {
    /* 非 JSON 响应 */
  }
  if (!res.ok) {
    const err = new Error(data.message || `请求失败 (${res.status})`)
    err.status = res.status
    throw err
  }
  return data
}

export const api = {
  tenants: () => req('GET', '/api/tenants'),
  doorplates: (floor) => req('GET', '/api/doorplates' + (floor ? `?floor=${floor}` : '')),
  applications: (status) => req('GET', '/api/applications' + (status ? `?status=${status}` : '')),
  application: (id) => req('GET', `/api/applications/${id}`),
  submit: (payload) => req('POST', '/api/applications', payload),
  review: (id, approve, reason) => req('POST', `/api/applications/${id}/review`, { approve, reason }),
  produce: (id, simulate_failure) => req('POST', `/api/applications/${id}/produce`, { simulate_failure }),
  install: (id, photo) => req('POST', `/api/applications/${id}/install`, { photo }),
  accept: (id, accepted_by) => req('POST', `/api/applications/${id}/accept`, { accepted_by }),
  publish: (id) => req('POST', `/api/applications/${id}/publish`),
  pause: (id, reason) => req('POST', `/api/applications/${id}/pause`, { reason }),
  resume: (id) => req('POST', `/api/applications/${id}/resume`),
  directory: () => req('GET', '/api/directory'),
  wayfinding: (floor) => req('GET', `/api/floors/${floor}/wayfinding`),
  printJobs: () => req('GET', '/api/mock/print-jobs'),
  courierNotifications: () => req('GET', '/api/mock/courier-notifications'),
  mockPrint: (application_id, content, simulate_failure) =>
    req('POST', '/api/mock/print', { application_id, content, simulate_failure }),
  mockCourier: (application_id) => req('POST', '/api/mock/courier', { application_id }),
}

export const STATUS_LABELS = {
  submitted: '已提交',
  approved: '已审核',
  producing: '制作中',
  installed: '已安装',
  accepted: '已验收',
  published: '已发布',
  paused: '已暂停',
  rejected: '已驳回',
}

export const DOOR_STATUS_LABELS = {
  available: '空闲',
  occupied: '占用中',
  forwarding: '转发过渡',
}

export const CHANGE_TYPE_LABELS = {
  move: '搬迁',
  rename: '更名',
}
