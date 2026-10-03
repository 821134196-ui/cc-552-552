const BASE = '/api';

async function request(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    const err = new Error(data.message || `请求失败 (${res.status})`);
    err.code = data.error;
    err.status = res.status;
    throw err;
  }
  return data;
}

export const api = {
  guide: () => request('GET', '/guide'),
  rooms: () => request('GET', '/rooms'),
  tenants: () => request('GET', '/tenants'),
  applications: () => request('GET', '/applications'),
  application: (id) => request('GET', `/applications/${id}`),
  create: (input) => request('POST', '/applications', input),
  approve: (id) => request('POST', `/applications/${id}/approve`),
  fabricate: (id, ok, note) =>
    request('POST', `/applications/${id}/fabricate`, { ok, note }),
  install: (id, installer, installPhoto) =>
    request('POST', `/applications/${id}/install`, { installer, installPhoto }),
  accept: (id, acceptedBy) =>
    request('POST', `/applications/${id}/accept`, { acceptedBy }),
  publish: (id) => request('POST', `/applications/${id}/publish`),
  resubmit: (id, patch) => request('PATCH', `/applications/${id}`, patch),
  notifications: () => request('GET', '/notifications'),
};
