import type { ApiRequest, MultipartRequest } from '@swmansion/smelter-core';

export async function sendRequest(baseUrl: string, request: ApiRequest): Promise<object> {
  const response = await fetch(new URL(request.route, baseUrl), {
    method: request.method,
    body: request.body && JSON.stringify(request.body),
    headers: {
      'Content-Type': 'application/json',
    },
  });
  if (response.status >= 400) {
    const err: any = new Error(`Request to Smelter server failed.`);
    err.response = response;
    try {
      err.body = await response.json();
    } catch {
      err.body = await response.text();
    }
    throw err;
  }
  return (await response.json()) as object;
}

export async function sendMultipartRequest(
  baseUrl: string,
  request: MultipartRequest
): Promise<object> {
  const response = await fetch(new URL(request.route, baseUrl), {
    method: request.method,
    body: request.body as FormData,
  });
  if (response.status >= 400) {
    const err: any = new Error(`Request to Smelter server failed.`);
    err.response = response;
    try {
      err.body = await response.json();
    } catch {
      err.body = await response.text();
    }
    throw err;
  }
  return (await response.json()) as object;
}
